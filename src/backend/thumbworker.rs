// `flea --thumb-worker`: libffmpegthumbnailer linked once, then one forked and confined child per video; see AGENTS.md "Thumbnail worker".
use crate::backend::fdpass;
use crate::backend::sandbox;
use crate::backend::thumbs::JOB_TIMEOUT;
use std::ffi::{c_void, CStr};
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::raw::{c_char, c_int, c_long, c_ulong};
use std::os::unix::process::ExitStatusExt;
use std::time::Instant;

// The worker's first byte: it can serve, or which of its two requirements is missing.
pub const READY: u8 = b'R';
pub const NO_LIBRARY: u8 = b'L';
pub const NO_LANDLOCK: u8 = b'K';
// Each job's one verdict: a thumbnail was written, the file failed, or this machine failed the job.
pub const SUCCEEDED: u8 = b'S';
pub const FAILED: u8 = b'F';
pub const NOT_STARTED: u8 = b'N';
// A request is the thumbnail size as four little-endian bytes and then the film-strip flag.
pub const REQUEST_BYTES: usize = 5;
// The ffmpegthumbnailer 2.x soname, the same library /usr/bin/ffmpegthumbnailer links.
pub const SONAME: &CStr = c"libffmpegthumbnailer.so.4";
// The child keeps the two job files here, so the path libav is handed is a constant.
const INPUT_FD: RawFd = 3;
const OUTPUT_FD: RawFd = 4;
const FIRST_UNUSED_FD: u32 = 5;
const INPUT_PATH: &CStr = c"/proc/self/fd/3";
// Descriptors are parked above this while the low numbers are rearranged, so no dup2 lands on one still needed.
const PARK_FD: c_int = 100;
// The child's exit codes: a verdict on the file, or a failure of this machine that judges nothing.
const CHILD_OK: i32 = 0;
const CHILD_REFUSED: i32 = 1;
const CHILD_MACHINE: i32 = 2;
// A larger request is not a thumbnail, and the pool only ever asks for THUMB_SIZE.
const MAX_SIZE: u32 = 1024;

// dlopen(3) RTLD_NOW, so a library missing a symbol fails at load and never inside a child.
const RTLD_NOW: c_int = 2;
// prctl(2): not dumpable, so no process of this user can ptrace the worker or read its descriptors; no new privileges, which Landlock requires.
const PR_SET_DUMPABLE: c_int = 4;
const PR_SET_NO_NEW_PRIVS: c_int = 38;
// setrlimit(2) resources, and the two values prlimit applies on the exec path.
const RLIMIT_CPU: c_int = 0;
const RLIMIT_AS: c_int = 9;
// Landlock's two syscalls, numbered alike on x86_64 and aarch64, and the flag that asks create_ruleset for the ABI version.
const SYS_LANDLOCK_CREATE_RULESET: c_long = 444;
const SYS_LANDLOCK_RESTRICT_SELF: c_long = 446;
const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
// Every filesystem right that writes, creates, removes, renames or truncates; reads are not handled, so libraries and the input still open.
const LANDLOCK_WRITE_V1: u64 = (1 << 1) | (0x1ff << 4);
const LANDLOCK_REFER_V2: u64 = 1 << 13;
const LANDLOCK_TRUNCATE_V3: u64 = 1 << 14;
const LANDLOCK_WRITE_RIGHTS: u64 = LANDLOCK_WRITE_V1 | LANDLOCK_REFER_V2 | LANDLOCK_TRUNCATE_V3;
// Truncation is a right only from ABI 3, and a job that could truncate its input would be weaker than the exec path's --ro-bind.
const LANDLOCK_TRUNCATE_ABI: i64 = 3;
// From ABI 6 a scoped child can signal nothing outside its own domain, so not a sibling and not the worker.
const LANDLOCK_SCOPE_SIGNAL: u64 = 1 << 1;
const LANDLOCK_SCOPE_SIGNAL_ABI: i64 = 6;
// poll(2) POLLIN, and EINTR, which is a retry.
const POLLIN: i16 = 1;
const EINTR: i32 = 4;
const SIGKILL: c_int = 9;
// fcntl(2) F_DUPFD_CLOEXEC.
const F_DUPFD_CLOEXEC: c_int = 1030;
// prctl(2) PR_SET_SECCOMP with a classic BPF filter, which the no_new_privs lock_down sets first allows.
const PR_SET_SECCOMP: c_int = 22;
const SECCOMP_MODE_FILTER: u64 = 2;
// seccomp_data holds the call number at byte 0 and the audit arch at byte 4 on every architecture.
const SECCOMP_DATA_NR: u32 = 0;
const SECCOMP_DATA_ARCH: u32 = 4;
// The low word of seccomp_data.args[0], where clone(2) keeps its flags on little-endian x86_64 and aarch64.
const SECCOMP_DATA_ARG0_LOW: u32 = 16;
// The only calling convention a job may use, from linux/audit.h; a compat 32-bit call carries another arch and is killed.
#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH_NATIVE: u32 = 0xc000_003e;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH_NATIVE: u32 = 0xc000_00b7;
// A new architecture adds its own arch and call numbers from its asm/unistd_64.h rather than inheriting x86_64's.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("the thumbnail worker's call filter needs this architecture's audit arch and call numbers");
// An x32 call carries the x86_64 arch and this bit in its number, so every x32 call is refused rather than read past the list.
// aarch64 numbers no call this high, so there the check refuses nothing a job could make.
const X32_SYSCALL_BIT: u32 = 0x4000_0000;
// BPF opcodes: load a word of seccomp_data, jump on equal or greater-or-equal against a constant, return a constant.
const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JEQ_K: u16 = 0x15;
const BPF_JGE_K: u16 = 0x35;
const BPF_JSET_K: u16 = 0x45;
const BPF_RET_K: u16 = 0x06;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
const EPERM: u32 = 1;
const ENOSYS: u32 = 38;
// Landlock leaves a file's mode, owner, times, xattrs and ioctl flags to its owner, which --ro-bind refused with EROFS; numbers from each arch's asm/unistd_64.h, io_uring for its xattr ops.
#[cfg(target_arch = "x86_64")]
const METADATA_WRITES: &[(&str, u32)] = &[
    ("ioctl", 16),
    ("chmod", 90),
    ("fchmod", 91),
    ("chown", 92),
    ("fchown", 93),
    ("lchown", 94),
    ("utime", 132),
    ("setxattr", 188),
    ("lsetxattr", 189),
    ("fsetxattr", 190),
    ("removexattr", 197),
    ("lremovexattr", 198),
    ("fremovexattr", 199),
    ("utimes", 235),
    ("fchownat", 260),
    ("futimesat", 261),
    ("fchmodat", 268),
    ("utimensat", 280),
    ("io_uring_setup", 425),
    ("fchmodat2", 452),
    ("setxattrat", 463),
    ("removexattrat", 466),
    ("file_setattr", 469),
];
// aarch64 has only the *at forms of chmod, chown and utime, so its list is shorter and names no call it lacks.
#[cfg(target_arch = "aarch64")]
const METADATA_WRITES: &[(&str, u32)] = &[
    ("setxattr", 5),
    ("lsetxattr", 6),
    ("fsetxattr", 7),
    ("removexattr", 14),
    ("lremovexattr", 15),
    ("fremovexattr", 16),
    ("ioctl", 29),
    ("fchmod", 52),
    ("fchmodat", 53),
    ("fchownat", 54),
    ("fchown", 55),
    ("utimensat", 88),
    ("io_uring_setup", 425),
    ("fchmodat2", 452),
    ("setxattrat", 463),
    ("removexattrat", 466),
    ("file_setattr", 469),
];
// A process a job starts would outlive the SIGKILL finish() sends, which the exec path's own bwrap init never allowed.
#[cfg(target_arch = "x86_64")]
const NEW_PROCESSES: &[(&str, u32)] = &[("fork", 57), ("vfork", 58)];
// aarch64 has no fork or vfork call; its processes start only through clone, which the thread check below gates.
#[cfg(target_arch = "aarch64")]
const NEW_PROCESSES: &[(&str, u32)] = &[];
// clone(2) is allowed only for a thread, which dies with the job; clone3 answers ENOSYS because its flags sit behind a pointer, and glibc falls back to clone.
#[cfg(target_arch = "x86_64")]
const SYS_CLONE: u32 = 56;
#[cfg(target_arch = "aarch64")]
const SYS_CLONE: u32 = 220;
const SYS_CLONE3: u32 = 435;
const CLONE_THREAD: u32 = 0x0001_0000;

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

#[repr(C)]
struct RLimit {
    current: u64,
    maximum: u64,
}

// struct sock_filter and struct sock_fprog from linux/filter.h.
#[repr(C)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

#[repr(C)]
struct SockFprog {
    len: u16,
    filter: *const SockFilter,
}

// struct landlock_ruleset_attr; a kernel older than a field accepts it while that field is zero.
#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

// video_thumbnailer from ffmpegthumbnailer 2.3's videothumbnailerc.h; only overlay_film_strip is written.
#[repr(C)]
struct VideoThumbnailer {
    thumbnail_size: c_int,
    seek_percentage: c_int,
    seek_time: *mut c_char,
    overlay_film_strip: c_int,
    workaround_bugs: c_int,
    thumbnail_image_quality: c_int,
    thumbnail_image_type: c_int,
    av_format_context: *mut c_void,
    maintain_aspect_ratio: c_int,
    prefer_embedded_metadata: c_int,
    tdata: *mut c_void,
}

// image_data from the same header; the library fills it with the encoded PNG.
#[repr(C)]
struct ImageData {
    ptr: *mut u8,
    size: c_int,
    width: c_int,
    height: c_int,
    source: c_int,
    internal: *mut c_void,
}

type Create = unsafe extern "C" fn() -> *mut VideoThumbnailer;
type SetSize = unsafe extern "C" fn(*mut VideoThumbnailer, c_int, c_int) -> c_int;
type CreateImageData = unsafe extern "C" fn() -> *mut ImageData;
type ToBuffer = unsafe extern "C" fn(*mut VideoThumbnailer, *const c_char, *mut ImageData) -> c_int;
type ForceCpuCount = unsafe extern "C" fn(c_int);

// std already links the system libc, so every symbol is declared here rather than taking a crate.
extern "C" {
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn prctl(option: c_int, ...) -> c_int;
    fn setrlimit(resource: c_int, limit: *const RLimit) -> c_int;
    fn syscall(number: c_long, ...) -> c_long;
    fn fork() -> c_int;
    fn _exit(code: c_int) -> !;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn pidfd_open(pid: c_int, flags: u32) -> c_int;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: c_int) -> c_int;
    fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    fn dup2(old: c_int, new: c_int) -> c_int;
    fn close_range(first: u32, last: u32, flags: c_int) -> c_int;
}

struct Decoder {
    create: Create,
    set_size: SetSize,
    create_image_data: CreateImageData,
    to_buffer: ToBuffer,
}

impl Decoder {
    // corner: the handle is never dlclose()d, because every child borrows it until this process exits.
    fn load(soname: &CStr) -> Option<Decoder> {
        unsafe {
            let library = dlopen(soname.as_ptr(), RTLD_NOW);
            if library.is_null() {
                return None;
            }
            let entry = |symbol: &CStr| {
                let found = dlsym(library, symbol.as_ptr());
                (!found.is_null()).then_some(found)
            };
            // One libav thread per video: the scale filter sizes its pool from av_cpu_count().
            if let Some(force) = entry(c"av_cpu_force_count") {
                std::mem::transmute::<*mut c_void, ForceCpuCount>(force)(1);
            }
            Some(Decoder {
                create: std::mem::transmute::<*mut c_void, Create>(entry(c"video_thumbnailer_create")?),
                set_size: std::mem::transmute::<*mut c_void, SetSize>(entry(c"video_thumbnailer_set_size")?),
                create_image_data: std::mem::transmute::<*mut c_void, CreateImageData>(entry(
                    c"video_thumbnailer_create_image_data",
                )?),
                to_buffer: std::mem::transmute::<*mut c_void, ToBuffer>(entry(
                    c"video_thumbnailer_generate_thumbnail_to_buffer",
                )?),
            })
        }
    }

    // `-s N` is set_size(N, N), measured against the CLI on portrait clips where (N, 0) differs, and `-f` is overlay_film_strip.
    fn generate(&self, size: c_int, film_strip: bool) -> i32 {
        unsafe {
            let thumbnailer = (self.create)();
            let image = (self.create_image_data)();
            if thumbnailer.is_null() || image.is_null() {
                return CHILD_MACHINE;
            }
            (self.set_size)(thumbnailer, size, size);
            (*thumbnailer).overlay_film_strip = c_int::from(film_strip);
            if (self.to_buffer)(thumbnailer, INPUT_PATH.as_ptr(), image) != 0 || (*image).ptr.is_null() || (*image).size <= 0 {
                return CHILD_REFUSED;
            }
            let png = std::slice::from_raw_parts((*image).ptr, (*image).size as usize);
            let mut out = std::fs::File::from_raw_fd(OUTPUT_FD);
            if out.write_all(png).is_err() {
                return CHILD_MACHINE;
            }
            CHILD_OK
        }
    }
}

// The Landlock ABI version, or None on a kernel whose Landlock cannot deny a truncation, which gets no worker.
fn landlock_abi() -> Option<i64> {
    let abi = unsafe {
        syscall(SYS_LANDLOCK_CREATE_RULESET, std::ptr::null::<RulesetAttr>(), 0usize, LANDLOCK_CREATE_RULESET_VERSION)
    };
    (abi >= LANDLOCK_TRUNCATE_ABI).then_some(abi)
}

// After this the child can open nothing for writing, truncate, create or remove nothing, and signal nothing outside itself.
fn lock_down(abi: i64) -> Option<()> {
    // corner: before ABI 6 there is no signal scope, so a child there could still signal its siblings.
    let scoped = if abi >= LANDLOCK_SCOPE_SIGNAL_ABI { LANDLOCK_SCOPE_SIGNAL } else { 0 };
    let attr = RulesetAttr { handled_access_fs: LANDLOCK_WRITE_RIGHTS, handled_access_net: 0, scoped };
    unsafe {
        if prctl(PR_SET_NO_NEW_PRIVS, 1 as c_ulong, 0 as c_ulong, 0 as c_ulong, 0 as c_ulong) != 0 {
            return None;
        }
        let ruleset = syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const RulesetAttr, std::mem::size_of::<RulesetAttr>(), 0u32);
        if ruleset < 0 {
            return None;
        }
        let ruleset = OwnedFd::from_raw_fd(ruleset as RawFd);
        (syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset.as_raw_fd(), 0u32) == 0).then_some(())
    }
}

// A foreign arch is killed, an x32 call, a listed call or a clone that is not a thread answers EPERM, clone3 answers ENOSYS, and every other call runs.
fn call_filter() -> Option<Vec<SockFilter>> {
    let refused: Vec<u32> = METADATA_WRITES.iter().chain(NEW_PROCESSES).map(|(_, number)| *number).collect();
    let x32_check = 4;
    let first_listed = x32_check + 1;
    let clone3_check = first_listed + refused.len();
    let clone_check = clone3_check + 1;
    let thread_check = clone_check + 2;
    let allow = thread_check + 1;
    let refuse = allow + 1;
    let no_such_call = refuse + 1;
    // A BPF jump counts the instructions it skips after the one that jumps.
    let skip = |from: usize, to: usize| u8::try_from(to - from - 1).ok();
    let op = |code, jt, jf, k| SockFilter { code, jt, jf, k };
    let mut program = vec![
        op(BPF_LD_W_ABS, 0, 0, SECCOMP_DATA_ARCH),
        op(BPF_JEQ_K, 1, 0, AUDIT_ARCH_NATIVE),
        op(BPF_RET_K, 0, 0, SECCOMP_RET_KILL_PROCESS),
        op(BPF_LD_W_ABS, 0, 0, SECCOMP_DATA_NR),
        op(BPF_JGE_K, skip(x32_check, refuse)?, 0, X32_SYSCALL_BIT),
    ];
    for (i, number) in refused.iter().enumerate() {
        program.push(op(BPF_JEQ_K, skip(first_listed + i, refuse)?, 0, *number));
    }
    program.push(op(BPF_JEQ_K, skip(clone3_check, no_such_call)?, 0, SYS_CLONE3));
    program.push(op(BPF_JEQ_K, 0, skip(clone_check, allow)?, SYS_CLONE));
    program.push(op(BPF_LD_W_ABS, 0, 0, SECCOMP_DATA_ARG0_LOW));
    program.push(op(BPF_JSET_K, skip(thread_check, allow)?, skip(thread_check, refuse)?, CLONE_THREAD));
    program.push(op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW));
    program.push(op(BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO | EPERM));
    program.push(op(BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO | ENOSYS));
    (program.len() == no_such_call + 1).then_some(program)
}

fn install_call_filter() -> Option<()> {
    let program = call_filter()?;
    let filter = SockFprog { len: u16::try_from(program.len()).ok()?, filter: program.as_ptr() };
    (unsafe { prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER as c_ulong, &filter as *const SockFprog as c_ulong, 0 as c_ulong, 0 as c_ulong) } == 0).then_some(())
}

// The child's whole descriptor table becomes /dev/null on 0 to 2, the input on 3 and the output on 4.
fn keep_only_the_job(input: RawFd, output: RawFd) -> Option<()> {
    let null = std::fs::OpenOptions::new().read(true).write(true).open("/dev/null").ok()?.into_raw_fd();
    unsafe {
        let parked = [fcntl(null, F_DUPFD_CLOEXEC, PARK_FD), fcntl(input, F_DUPFD_CLOEXEC, PARK_FD), fcntl(output, F_DUPFD_CLOEXEC, PARK_FD)];
        if parked.iter().any(|fd| *fd < 0) {
            return None;
        }
        let [null, input, output] = parked;
        for (from, to) in [(null, 0), (null, 1), (null, 2), (input, INPUT_FD), (output, OUTPUT_FD)] {
            if dup2(from, to) != to {
                return None;
            }
        }
        (close_range(FIRST_UNUSED_FD, u32::MAX, 0) == 0).then_some(())
    }
}

fn set_limit(resource: c_int, value: u64) -> Option<()> {
    let limit = RLimit { current: value, maximum: value };
    (unsafe { setrlimit(resource, &limit) } == 0).then_some(())
}

// Everything that has to hold before a byte of the file is decoded; any failure judges no file.
fn confine(input: RawFd, output: RawFd, abi: i64) -> Option<()> {
    keep_only_the_job(input, output)?;
    set_limit(RLIMIT_CPU, u64::from(sandbox::CPU_SECONDS))?;
    set_limit(RLIMIT_AS, sandbox::ADDRESS_SPACE_BYTES)?;
    lock_down(abi)?;
    install_call_filter()
}

// Runs in the forked child and never returns: the exit code is the verdict, and a panic is caught so it never unwinds into the worker's loop.
fn child(input: RawFd, output: RawFd, size: c_int, film_strip: bool, decoder: &Decoder, abi: i64) -> ! {
    let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match confine(input, output, abi) {
        None => CHILD_MACHINE,
        Some(()) => decoder.generate(size, film_strip),
    }));
    unsafe { _exit(verdict.unwrap_or(CHILD_MACHINE)) }
}

struct Running {
    pid: c_int,
    pidfd: OwnedFd,
    reply: OwnedFd,
    deadline: Instant,
}

// Reaps one child and answers its job; a child killed by a signal, the deadline's included, ran on the file and failed.
fn finish(job: Running, past_deadline: bool) {
    if past_deadline {
        let _ = crate::procsignal::signal_process(job.pid, SIGKILL);
    }
    let mut status: c_int = 0;
    let verdict = if unsafe { waitpid(job.pid, &mut status, 0) } != job.pid {
        NOT_STARTED
    } else {
        match std::process::ExitStatus::from_raw(status).code() {
            Some(CHILD_OK) if !past_deadline => SUCCEEDED,
            Some(CHILD_MACHINE) => NOT_STARTED,
            _ => FAILED,
        }
    };
    // corner: a backend that gave up on this job has closed its end, and there is no one left to tell.
    let _ = fdpass::send_byte(&job.reply, verdict);
}

// Sample request: payload [0, 1, 0, 0, 1] (size 256, film strip on), descriptors [input, output, reply].
fn start(message: fdpass::Received, decoder: &Decoder, abi: i64, running: &mut Vec<Running>) {
    let mut fds = message.fds.into_iter();
    let (Some(input), Some(output), Some(reply), None) = (fds.next(), fds.next(), fds.next(), fds.next()) else {
        return;
    };
    let payload = message.payload;
    let size = if payload.len() == REQUEST_BYTES { u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) } else { 0 };
    if size == 0 || size > MAX_SIZE {
        let _ = fdpass::send_byte(&reply, NOT_STARTED);
        return;
    }
    let film_strip = payload[4] != 0;
    let pid = unsafe { fork() };
    if pid == 0 {
        child(input.as_raw_fd(), output.as_raw_fd(), size as c_int, film_strip, decoder, abi);
    }
    if pid < 0 {
        let _ = fdpass::send_byte(&reply, NOT_STARTED);
        return;
    }
    drop(input);
    drop(output);
    let raw = unsafe { pidfd_open(pid, 0) };
    if raw < 0 {
        // corner: with no descriptor there is no deadline to enforce, so the child is ended now and judges nothing.
        let _ = crate::procsignal::signal_process(pid, SIGKILL);
        let mut status: c_int = 0;
        unsafe { waitpid(pid, &mut status, 0) };
        let _ = fdpass::send_byte(&reply, NOT_STARTED);
        return;
    }
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    running.push(Running { pid, pidfd, reply, deadline: Instant::now() + JOB_TIMEOUT });
}

// Milliseconds to the nearest deadline, rounded up, or -1 to wait for a request with no child running.
fn poll_timeout(running: &[Running]) -> c_int {
    match running.iter().map(|r| r.deadline).min() {
        None => -1,
        Some(at) => at.saturating_duration_since(Instant::now()).as_millis().saturating_add(1).min(c_int::MAX as u128) as c_int,
    }
}

fn serve(requests: &OwnedFd, decoder: &Decoder, abi: i64) -> i32 {
    let mut running: Vec<Running> = Vec::new();
    loop {
        let mut fds: Vec<PollFd> = Vec::with_capacity(running.len() + 1);
        fds.push(PollFd { fd: requests.as_raw_fd(), events: POLLIN, revents: 0 });
        for job in &running {
            fds.push(PollFd { fd: job.pidfd.as_raw_fd(), events: POLLIN, revents: 0 });
        }
        let ready = unsafe { poll(fds.as_mut_ptr(), fds.len(), poll_timeout(&running)) };
        if ready < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(EINTR) {
                continue;
            }
            return 1;
        }
        // Children first, so a verdict is never held behind a request.
        let now = Instant::now();
        let mut still = Vec::with_capacity(running.len());
        for (job, polled) in running.into_iter().zip(fds.iter().skip(1)) {
            if polled.revents != 0 {
                finish(job, false);
            } else if now >= job.deadline {
                finish(job, true);
            } else {
                still.push(job);
            }
        }
        running = still;
        if fds[0].revents != 0 {
            match fdpass::recv(requests.as_raw_fd()) {
                Ok(Some(message)) => start(message, decoder, abi, &mut running),
                // The backend closed its end, so there will be no more work; bwrap's own init ends any child still running.
                Ok(None) => return 0,
                Err(e) if e.raw_os_error() == Some(EINTR) => {}
                // A malformed packet is dropped with its descriptors; anything else is a socket that no longer works.
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {}
                Err(_) => return 1,
            }
        }
    }
}

// The whole process: stdin is the request socket, and the first byte says whether this worker can serve.
pub fn run() -> i32 {
    let requests = unsafe { OwnedFd::from_raw_fd(0) };
    unsafe { prctl(PR_SET_DUMPABLE, 0 as c_ulong, 0 as c_ulong, 0 as c_ulong, 0 as c_ulong) };
    let Some(decoder) = Decoder::load(SONAME) else {
        let _ = fdpass::send_byte(&requests, NO_LIBRARY);
        return 1;
    };
    let Some(abi) = landlock_abi() else {
        let _ = fdpass::send_byte(&requests, NO_LANDLOCK);
        return 1;
    };
    if fdpass::send_byte(&requests, READY).is_err() {
        return 1;
    }
    serve(&requests, &decoder, abi)
}

#[cfg(test)]
#[path = "thumbworker_threads_tests.rs"]
mod threads_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    // The probe confines a second copy of this test binary, so nothing it locks down can reach the harness running every other test.
    const CONFINE_PROBE: &str = "FLEA_CONFINE_PROBE";
    const CONFINE_BUSY: &str = "FLEA_CONFINE_BUSY";
    const THIS_TEST: &str = "backend::thumbworker::tests::a_confined_child_holds_only_its_job_and_can_write_or_signal_nothing";
    // Every descriptor below this is looked at, which covers PARK_FD and anything a leak could leave.
    const FD_CEILING: c_int = 1024;
    // fcntl(2) F_GETFD, which fails only on a descriptor that is not open.
    const F_GETFD: c_int = 1;
    // recvmsg hands an idle worker a job on 3 and 4 and a busy one higher up, so the probe runs once from each.
    const BUSY_FD: c_int = 40;
    // The permission bits fchmod takes, so the probe can set a file's own mode back as a no-op.
    const MODE_BITS: u32 = 0o7777;
    // utimensat(2) UTIME_OMIT for both times: a call that changes nothing and succeeds unconfined.
    const UTIME_OMIT: i64 = (1 << 30) - 2;
    const KEEP_TIMES: [Timespec; 2] = [Timespec { seconds: 0, nanoseconds: UTIME_OMIT }, Timespec { seconds: 0, nanoseconds: UTIME_OMIT }];
    // What Landlock answers for a right its ruleset handles and no rule grants.
    const EACCES: u32 = 13;
    // ioctl(2) FS_IOC_GETFLAGS, a read that only the filter refuses with EPERM.
    const FS_IOC_GETFLAGS: usize = 0x8008_6601;
    // A child that ends at once is readable on its pidfd long before this; it bounds a broken test, it times nothing.
    const CHILD_WAIT_MS: c_int = 5000;

    #[repr(C)]
    struct Timespec {
        seconds: i64,
        nanoseconds: i64,
    }

    extern "C" {
        fn getppid() -> c_int;
        fn getrlimit(resource: c_int, limit: *mut RLimit) -> c_int;
        fn fchmod(fd: c_int, mode: u32) -> c_int;
        fn truncate(path: *const c_char, length: i64) -> c_int;
        fn futimens(fd: c_int, times: *const [Timespec; 2]) -> c_int;
        fn fsetxattr(fd: c_int, name: *const c_char, value: *const c_void, size: usize, flags: c_int) -> c_int;
        fn ioctl(fd: i32, request: usize, ...) -> i32;
        fn getpid() -> c_int;
        fn pause() -> c_int;
    }

    // 0 for a call that worked, otherwise the errno it set.
    fn errno_of(result: c_int) -> i32 {
        if result == 0 { 0 } else { std::io::Error::last_os_error().raw_os_error().unwrap_or(-1) }
    }

    // 0 when a process could be forked and reaped, otherwise the errno fork set; the forked process only exits.
    fn forked_and_reaped() -> i32 {
        let pid = unsafe { fork() };
        if pid == 0 {
            unsafe { _exit(CHILD_OK) };
        }
        if pid < 0 {
            return errno_of(pid);
        }
        let mut status: c_int = 0;
        unsafe { waitpid(pid, &mut status, 0) };
        0
    }

    // How a job's child ends in the verdict test.
    enum Ends {
        Exits(i32),
        Dies,
        Hangs,
    }

    // Forks a child that ends the given way, waits until it has ended without reaping it, and hands finish() the job.
    fn job_that(ends: &Ends, reply: OwnedFd) -> Running {
        let pid = unsafe { fork() };
        if pid == 0 {
            match ends {
                Ends::Exits(code) => unsafe { _exit(*code) },
                Ends::Dies => drop(crate::procsignal::signal_process(unsafe { getpid() }, SIGKILL)),
                Ends::Hangs => loop {
                    unsafe { pause() };
                },
            };
            unsafe { _exit(CHILD_OK) };
        }
        assert!(pid > 0, "the test could not fork");
        let pidfd = unsafe { OwnedFd::from_raw_fd(pidfd_open(pid, 0)) };
        if !matches!(ends, Ends::Hangs) {
            let mut ended = PollFd { fd: pidfd.as_raw_fd(), events: POLLIN, revents: 0 };
            assert_eq!(unsafe { poll(&mut ended, 1, CHILD_WAIT_MS) }, 1, "the child never ended");
        }
        Running { pid, pidfd, reply, deadline: Instant::now() }
    }

    // Where the kernel's own numbers live, Arch's first, then Debian and Ubuntu's multiarch copy, so the table is checked against something it did not write.
    #[cfg(target_arch = "x86_64")]
    const UNISTD: &[&str] = &["/usr/include/asm/unistd_64.h", "/usr/include/x86_64-linux-gnu/asm/unistd_64.h"];
    // arm64 headers before Linux 6.11 generate no unistd_64.h; arm64 uses the generic table, so asm-generic holds its numbers.
    #[cfg(target_arch = "aarch64")]
    const UNISTD: &[&str] = &["/usr/include/asm/unistd_64.h", "/usr/include/aarch64-linux-gnu/asm/unistd_64.h", "/usr/include/asm-generic/unistd.h"];
    // The compat 32-bit arch this kernel could also run, 32-bit x86 or 32-bit ARM, which the filter must kill.
    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH_FOREIGN: u32 = 0x4000_0003;
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH_FOREIGN: u32 = 0x4000_0028;
    // Where the clone flag bits live, so the thread bit the filter tests is checked against the kernel's too.
    const SCHED_H: &str = "/usr/include/linux/sched.h";
    // The exit signal glibc's fork() and posix_spawn put in clone's low byte, from asm/signal.h.
    const SIGCHLD: u32 = 17;
    // Every call a job must never make, named independently of the table under test.
    #[cfg(target_arch = "x86_64")]
    const MUST_REFUSE: &[&str] = &[
        "ioctl", "chmod", "fchmod", "chown", "fchown", "lchown", "utime", "setxattr", "lsetxattr", "fsetxattr",
        "removexattr", "lremovexattr", "fremovexattr", "utimes", "fchownat", "futimesat", "fchmodat", "utimensat",
        "io_uring_setup", "fchmodat2", "setxattrat", "removexattrat", "file_setattr", "fork", "vfork",
    ];
    // The same calls on aarch64, which has no chmod, chown, lchown, utime, utimes, futimesat, fork or vfork to refuse.
    #[cfg(target_arch = "aarch64")]
    const MUST_REFUSE: &[&str] = &[
        "ioctl", "fchmod", "fchown", "setxattr", "lsetxattr", "fsetxattr", "removexattr", "lremovexattr",
        "fremovexattr", "fchownat", "fchmodat", "utimensat", "io_uring_setup", "fchmodat2", "setxattrat",
        "removexattrat", "file_setattr",
    ];

    // The newest call the tables name (Linux 6.17); headers without it predate the tables and cannot check them.
    const NEWEST_CALL: &str = "file_setattr";

    // Sample input line: "#define __NR_fchmod 91".
    fn kernel_numbers() -> (&'static str, std::collections::HashMap<String, u32>) {
        let (path, header) = UNISTD
            .iter()
            .find_map(|path| match std::fs::read_to_string(path) {
                Ok(text) => Some((*path, text)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => panic!("{} did not read: {}", path, e),
            })
            .unwrap_or_else(|| panic!("no kernel header at {}: install linux-api-headers or linux-libc-dev", UNISTD.join(" or ")));
        let mut numbers = std::collections::HashMap::new();
        for line in header.lines() {
            let mut words = line.split_whitespace();
            if let (Some("#define"), Some(name), Some(number)) = (words.next(), words.next(), words.next()) {
                if let (Some(call), Ok(number)) = (name.strip_prefix("__NR_"), number.parse()) {
                    numbers.insert(call.to_string(), number);
                }
            }
        }
        // Every kernel names read, so a header that yields no read is a parse failure, never an old kernel.
        assert!(numbers.contains_key("read"), "{} yielded no __NR_read, so the parse failed", path);
        (path, numbers)
    }

    // Sample input line: "#define CLONE_THREAD\t0x00010000\t/* Same thread group? */"
    fn clone_bits() -> std::collections::HashMap<String, u32> {
        let header = std::fs::read_to_string(SCHED_H).unwrap_or_else(|e| panic!("{} did not read: {}", SCHED_H, e));
        let mut bits = std::collections::HashMap::new();
        for line in header.lines() {
            let mut words = line.split_whitespace();
            if let (Some("#define"), Some(name), Some(value)) = (words.next(), words.next(), words.next()) {
                if let (true, Some(Ok(value))) = (name.starts_with("CLONE_"), value.strip_prefix("0x").map(|hex| u32::from_str_radix(hex, 16))) {
                    bits.insert(name.to_string(), value);
                }
            }
        }
        bits
    }

    // Runs the filter the way the kernel does for one call and returns what it answers, so no call is ever made.
    fn verdict_of(program: &[SockFilter], arch: u32, number: u32, first_argument: u32) -> u32 {
        let mut at = 0;
        let mut loaded = 0;
        loop {
            let step = &program[at];
            let here = at;
            let jump = |taken: bool| here + 1 + usize::from(if taken { step.jt } else { step.jf });
            at = match step.code {
                BPF_LD_W_ABS => {
                    loaded = match step.k {
                        SECCOMP_DATA_ARCH => arch,
                        SECCOMP_DATA_NR => number,
                        SECCOMP_DATA_ARG0_LOW => first_argument,
                        other => panic!("the filter loads offset {}, which this walk does not model", other),
                    };
                    here + 1
                }
                BPF_JEQ_K => jump(loaded == step.k),
                BPF_JGE_K => jump(loaded >= step.k),
                BPF_JSET_K => jump(loaded & step.k != 0),
                BPF_RET_K => return step.k,
                other => panic!("the filter uses opcode {:#x}, which this walk does not model", other),
            };
        }
    }

    // Headers naming the newest call must name every call, so a name they lack is a typo; older ones leave the newer calls unchecked (Ok(None)).
    fn kernel_number(numbers: &std::collections::HashMap<String, u32>, header: &str, call: &str) -> Result<Option<u32>, String> {
        match numbers.get(call) {
            Some(number) => Ok(Some(*number)),
            None if numbers.contains_key(NEWEST_CALL) => Err(format!("{} has no __NR_{}", header, call)),
            None => Ok(None),
        }
    }

    #[test]
    fn older_headers_leave_newer_calls_unchecked_and_current_ones_fail_a_missing_name() {
        assert!(MUST_REFUSE.contains(&NEWEST_CALL), "the newest call is one the filter must refuse");
        let current: std::collections::HashMap<String, u32> = [(NEWEST_CALL.to_string(), 1), ("fchmod".to_string(), 2)].into();
        let older: std::collections::HashMap<String, u32> = [("fchmod".to_string(), 2)].into();
        assert_eq!(kernel_number(&current, "h", "fchmod"), Ok(Some(2)));
        assert_eq!(kernel_number(&older, "h", "fchmod"), Ok(Some(2)), "older headers still check the calls they name");
        assert_eq!(kernel_number(&older, "h", "setxattrat"), Ok(None));
        assert_eq!(kernel_number(&current, "h", "fchmdo"), Err("h has no __NR_fchmdo".to_string()), "a typo against current headers fails");
    }

    #[test]
    fn every_call_that_changes_a_file_or_starts_a_process_is_refused() {
        const THIS: &str = "backend::thumbworker::tests::every_call_that_changes_a_file_or_starts_a_process_is_refused";
        let (header, numbers) = kernel_numbers();
        let program = call_filter().expect("the filter did not build");
        let known = |call: &str| kernel_number(&numbers, header, call).unwrap_or_else(|why| panic!("{}", why));
        let number = |call: &str| known(call).unwrap_or_else(|| panic!("{} has no __NR_{}", header, call));
        let refused = SECCOMP_RET_ERRNO | EPERM;
        let mut unchecked = Vec::new();
        for (call, listed) in METADATA_WRITES.iter().chain(NEW_PROCESSES) {
            match known(call) {
                Some(kernel) => assert_eq!(*listed, kernel, "the table gives {} the number {}", call, listed),
                None => unchecked.push(*call),
            }
        }
        for &call in MUST_REFUSE {
            match known(call) {
                Some(kernel) => assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, kernel, 0), refused, "{} is not refused", call),
                None => unchecked.push(call),
            }
        }
        if !unchecked.is_empty() {
            unchecked.sort_unstable();
            unchecked.dedup();
            std::io::stderr().write_all(format!("PARTIAL {}: {} predates __NR_{}, so {} went unchecked\n", THIS, header, NEWEST_CALL, unchecked.join(", ")).as_bytes()).ok();
        }
        let bits = clone_bits();
        let flags = |names: &[&str]| names.iter().map(|name| *bits.get(*name).unwrap_or_else(|| panic!("{} has no {}", SCHED_H, name))).fold(0, |all, bit| all | bit);
        assert_eq!(CLONE_THREAD, flags(&["CLONE_THREAD"]), "the filter's thread bit is not the kernel's");
        // The flags glibc's pthread_create, fork() and posix_spawn pass to clone on this box.
        let thread = flags(&["CLONE_VM", "CLONE_FS", "CLONE_FILES", "CLONE_SIGHAND", "CLONE_THREAD", "CLONE_SYSVSEM", "CLONE_SETTLS", "CLONE_PARENT_SETTID", "CLONE_CHILD_CLEARTID"]);
        let fork = flags(&["CLONE_CHILD_SETTID", "CLONE_CHILD_CLEARTID"]) | SIGCHLD;
        let spawn = flags(&["CLONE_VM", "CLONE_VFORK"]) | SIGCHLD;
        assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone"), thread), SECCOMP_RET_ALLOW, "a thread must start");
        assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone"), fork), refused, "a forked process must not");
        assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone"), spawn), refused, "a spawned process must not");
        assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, number("clone3"), 0), SECCOMP_RET_ERRNO | ENOSYS);
        assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, number("read"), 0), SECCOMP_RET_ALLOW, "an ordinary call must run");
        assert_eq!(verdict_of(&program, AUDIT_ARCH_NATIVE, number("read") | X32_SYSCALL_BIT, 0), refused, "an x32 call must not");
        assert_eq!(verdict_of(&program, AUDIT_ARCH_FOREIGN, number("read"), 0), SECCOMP_RET_KILL_PROCESS);
    }

    #[test]
    fn a_verdict_is_read_from_how_the_child_ended() {
        let cases = [
            (Ends::Exits(CHILD_OK), false, SUCCEEDED),
            (Ends::Exits(CHILD_REFUSED), false, FAILED),
            (Ends::Exits(CHILD_MACHINE), false, NOT_STARTED),
            (Ends::Dies, false, FAILED),
            (Ends::Exits(CHILD_OK), true, FAILED),
            (Ends::Hangs, true, FAILED),
        ];
        for (ends, past_deadline, verdict) in cases {
            let (mine, theirs) = fdpass::pair().unwrap();
            finish(job_that(&ends, mine), past_deadline);
            let answer = fdpass::recv(theirs.as_raw_fd()).unwrap().expect("a verdict");
            assert_eq!(answer.payload, vec![verdict], "past deadline {} wanted {}", past_deadline, verdict as char);
        }
    }

    fn soft_limit(resource: c_int) -> u64 {
        let mut limit = RLimit { current: 0, maximum: 0 };
        unsafe { getrlimit(resource, &mut limit) };
        limit.current
    }

    // What descriptor 0, 1 or 2 points at, such as "/dev/null" or "socket:[123]".
    fn target(fd: c_int) -> String {
        std::fs::read_link(format!("/proc/self/fd/{}", fd)).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
    }

    // Holds the input read-only, the report write-only and a socket on 0, the way a job arrives in the worker, runs confine() and writes what it can still do to the report.
    fn probe(dir: &str, busy: bool) -> ! {
        let victim = format!("{}/victim.mp4", dir);
        let opened = std::fs::File::open(&victim).expect("the probe could not open its input").into_raw_fd();
        let written = std::fs::OpenOptions::new().write(true).open(format!("{}/report", dir)).expect("the probe could not open its report").into_raw_fd();
        let (reader, report) = if busy {
            let moved = unsafe { (fcntl(opened, F_DUPFD_CLOEXEC, BUSY_FD), fcntl(written, F_DUPFD_CLOEXEC, BUSY_FD)) };
            drop(unsafe { (OwnedFd::from_raw_fd(opened), OwnedFd::from_raw_fd(written)) });
            moved
        } else {
            (opened, written)
        };
        if reader < 0 || report < 0 {
            unsafe { _exit(CHILD_MACHINE) };
        }
        // Lowest-free numbering puts an idle job on 3 and 4 only when nothing was inherited, so a shifted idle arm fails rather than rerunning the busy one.
        if !busy && (reader, report) != (INPUT_FD, OUTPUT_FD) {
            println!("probe=idle-arm-on {} {} fd4={}", reader, report, target(4));
            unsafe { _exit(CHILD_MACHINE) };
        }
        let (_peer, planted) = fdpass::pair().expect("the probe could not make its socket");
        if unsafe { dup2(planted.as_raw_fd(), 0) } != 0 {
            unsafe { _exit(CHILD_MACHINE) };
        }
        let socket_before = target(0).starts_with("socket:");
        let Some(abi) = landlock_abi() else {
            println!("probe=no-landlock");
            std::process::exit(0);
        };
        let writable = |path: &str| std::fs::OpenOptions::new().write(true).open(path).is_ok();
        let wrote_before = writable(&format!("/proc/self/fd/{}", reader));
        let signalled_before = crate::procsignal::signal_process(unsafe { getppid() }, 0).is_ok();
        let mode = std::fs::metadata(&victim).map(|m| m.permissions().mode() & MODE_BITS).unwrap_or(0);
        let chmod_before = unsafe { fchmod(reader, mode) } == 0;
        let times_before = unsafe { futimens(reader, &KEEP_TIMES) } == 0;
        let length = std::fs::metadata(&victim).map(|m| m.len() as i64).unwrap_or(-1);
        let victim_path = std::ffi::CString::new(victim.as_str()).expect("the victim path holds a NUL");
        let truncate_before = unsafe { truncate(victim_path.as_ptr(), length) } == 0;
        let fork_before = forked_and_reaped() == 0;
        if confine(reader, report, abi).is_none() {
            unsafe { _exit(CHILD_MACHINE) };
        }
        let chmod = unsafe { fchmod(INPUT_FD, mode) } == 0;
        let times = unsafe { futimens(INPUT_FD, &KEEP_TIMES) } == 0;
        let truncate_errno = errno_of(unsafe { truncate(INPUT_PATH.as_ptr(), 0) });
        let xattr = errno_of(unsafe { fsetxattr(INPUT_FD, c"user.flea_probe".as_ptr(), b"1".as_ptr() as *const c_void, 1, 0) });
        let mut flags: c_long = 0;
        let ioctl_errno = errno_of(unsafe { ioctl(INPUT_FD, FS_IOC_GETFLAGS, &mut flags) });
        let fork_errno = forked_and_reaped();
        let thread = std::thread::Builder::new().spawn(|| true).map(|handle| handle.join().unwrap_or(false)).unwrap_or(false);
        let open: Vec<c_int> = (0..FD_CEILING).filter(|fd| unsafe { fcntl(*fd, F_GETFD, 0 as c_int) } >= 0).collect();
        let nulls = (0..3).all(|fd| target(fd) == "/dev/null");
        let input_on_3 = target(INPUT_FD).ends_with("/victim.mp4");
        let facts = format!(
            "socket_before={} nulls={} input_on_3={} before={} reopened={} direct={} readable={} signalled_before={} signalled={} chmod_before={} chmod={} times_before={} times={} truncate_before={} truncate_errno={} xattr_errno={} ioctl_errno={} fork_before={} fork_errno={} thread={} fds={:?} cpu={} as={}",
            socket_before,
            nulls,
            input_on_3,
            wrote_before,
            writable("/proc/self/fd/3"),
            writable(&victim),
            std::fs::File::open("/proc/self/fd/3").is_ok(),
            signalled_before,
            crate::procsignal::signal_process(unsafe { getppid() }, 0).is_ok(),
            chmod_before,
            chmod,
            times_before,
            times,
            truncate_before,
            truncate_errno,
            xattr,
            ioctl_errno,
            fork_before,
            fork_errno,
            thread,
            open,
            soft_limit(RLIMIT_CPU),
            soft_limit(RLIMIT_AS)
        );
        let mut out = unsafe { std::fs::File::from_raw_fd(OUTPUT_FD) };
        let _ = out.write_all(facts.as_bytes());
        unsafe { _exit(CHILD_OK) }
    }

    #[test]
    fn a_confined_child_holds_only_its_job_and_can_write_or_signal_nothing() {
        if let Ok(dir) = std::env::var(CONFINE_PROBE) {
            probe(&dir, std::env::var(CONFINE_BUSY).is_ok_and(|v| v == "1"));
        }
        let dir = TestDir::new("worker-confine");
        let victim = dir.file("victim.mp4", "original");
        for busy in [false, true] {
            let report = dir.file("report", "");
            let mut command = Command::new(std::env::current_exe().unwrap());
            // SAFETY: the child only marks inherited fds close-on-exec before it execs this test binary.
            unsafe {
                command.pre_exec(|| {
                    const CLOSE_RANGE_CLOEXEC: c_int = 4;
                    if close_range(3, u32::MAX, CLOSE_RANGE_CLOEXEC) == 0 {
                        Ok(())
                    } else {
                        Err(std::io::Error::last_os_error())
                    }
                });
            }
            let out = command
                .args(["--exact", THIS_TEST, "--test-threads=1", "--nocapture"])
                .env(CONFINE_PROBE, dir.path())
                .env(CONFINE_BUSY, if busy { "1" } else { "0" })
                .output()
                .unwrap();
            if String::from_utf8_lossy(&out.stdout).contains("probe=no-landlock") {
                std::io::stderr().write_all(format!("SKIP {}: this kernel has no Landlock that can deny a truncation\n", THIS_TEST).as_bytes()).ok();
                return;
            }
            let scoped = landlock_abi().is_some_and(|abi| abi >= LANDLOCK_SCOPE_SIGNAL_ABI);
            // The *_before facts are the negative controls: unconfined, the same process holds a socket on 0, writes, signals, chmods, sets times, truncates and forks.
            let expected = format!(
                "socket_before=true nulls=true input_on_3=true before=true reopened=false direct=false readable=true signalled_before=true signalled={} chmod_before=true chmod=false times_before=true times=false truncate_before=true truncate_errno={} xattr_errno={} ioctl_errno={} fork_before=true fork_errno={} thread=true fds=[0, 1, 2, 3, 4] cpu={} as={}",
                !scoped,
                EACCES,
                EPERM,
                EPERM,
                EPERM,
                sandbox::CPU_SECONDS,
                sandbox::ADDRESS_SPACE_BYTES
            );
            assert_eq!(std::fs::read_to_string(&report).unwrap(), expected, "busy {} probe exited {:?}: {}", busy, out.status, String::from_utf8_lossy(&out.stdout));
            assert_eq!(std::fs::read(&victim).unwrap(), b"original");
        }
    }

    #[test]
    fn a_library_that_is_not_there_is_refused() {
        assert!(Decoder::load(c"libflea-definitely-not-here.so.9").is_none());
    }

    #[test]
    fn the_ruleset_denies_truncation_and_never_a_read() {
        assert_ne!(LANDLOCK_WRITE_RIGHTS & LANDLOCK_TRUNCATE_V3, 0, "a job could truncate its input");
        // Execute, read file and read dir are bits 0, 2 and 3, and none of them may ever be denied.
        let reads = (1 << 0) | (1 << 2) | (1 << 3);
        assert_eq!(LANDLOCK_WRITE_RIGHTS & reads, 0);
    }
}
