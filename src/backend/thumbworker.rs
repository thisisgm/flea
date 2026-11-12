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
pub const CANCEL_VIDEO: u8 = b'C';
pub const VIDEO_CANCELLED: u8 = b'X';
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
compile_error!(
    "the thumbnail worker's call filter needs this architecture's audit arch and call numbers"
);
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
                create: std::mem::transmute::<*mut c_void, Create>(entry(
                    c"video_thumbnailer_create",
                )?),
                set_size: std::mem::transmute::<*mut c_void, SetSize>(entry(
                    c"video_thumbnailer_set_size",
                )?),
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
            if (self.to_buffer)(thumbnailer, INPUT_PATH.as_ptr(), image) != 0
                || (*image).ptr.is_null()
                || (*image).size <= 0
            {
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
        syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    (abi >= LANDLOCK_TRUNCATE_ABI).then_some(abi)
}

// After this the child can open nothing for writing, truncate, create or remove nothing, and signal nothing outside itself.
fn lock_down(abi: i64) -> Option<()> {
    // corner: before ABI 6 there is no signal scope, so a child there could still signal its siblings.
    let scoped = if abi >= LANDLOCK_SCOPE_SIGNAL_ABI {
        LANDLOCK_SCOPE_SIGNAL
    } else {
        0
    };
    let attr = RulesetAttr {
        handled_access_fs: LANDLOCK_WRITE_RIGHTS,
        handled_access_net: 0,
        scoped,
    };
    unsafe {
        if prctl(
            PR_SET_NO_NEW_PRIVS,
            1 as c_ulong,
            0 as c_ulong,
            0 as c_ulong,
            0 as c_ulong,
        ) != 0
        {
            return None;
        }
        let ruleset = syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            &attr as *const RulesetAttr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        );
        if ruleset < 0 {
            return None;
        }
        let ruleset = OwnedFd::from_raw_fd(ruleset as RawFd);
        (syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset.as_raw_fd(), 0u32) == 0).then_some(())
    }
}

// A foreign arch is killed, an x32 call, a listed call or a clone that is not a thread answers EPERM, clone3 answers ENOSYS, and every other call runs.
fn call_filter() -> Option<Vec<SockFilter>> {
    let refused: Vec<u32> = METADATA_WRITES
        .iter()
        .chain(NEW_PROCESSES)
        .map(|(_, number)| *number)
        .collect();
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
    program.push(op(
        BPF_JEQ_K,
        skip(clone3_check, no_such_call)?,
        0,
        SYS_CLONE3,
    ));
    program.push(op(BPF_JEQ_K, 0, skip(clone_check, allow)?, SYS_CLONE));
    program.push(op(BPF_LD_W_ABS, 0, 0, SECCOMP_DATA_ARG0_LOW));
    program.push(op(
        BPF_JSET_K,
        skip(thread_check, allow)?,
        skip(thread_check, refuse)?,
        CLONE_THREAD,
    ));
    program.push(op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW));
    program.push(op(BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO | EPERM));
    program.push(op(BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO | ENOSYS));
    (program.len() == no_such_call + 1).then_some(program)
}

fn install_call_filter() -> Option<()> {
    let program = call_filter()?;
    let filter = SockFprog {
        len: u16::try_from(program.len()).ok()?,
        filter: program.as_ptr(),
    };
    (unsafe {
        prctl(
            PR_SET_SECCOMP,
            SECCOMP_MODE_FILTER as c_ulong,
            &filter as *const SockFprog as c_ulong,
            0 as c_ulong,
            0 as c_ulong,
        )
    } == 0)
        .then_some(())
}

// The child's whole descriptor table becomes /dev/null on 0 to 2, the input on 3 and the output on 4.
fn keep_only_the_job(input: RawFd, output: RawFd) -> Option<()> {
    let null = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .ok()?
        .into_raw_fd();
    unsafe {
        let parked = [
            fcntl(null, F_DUPFD_CLOEXEC, PARK_FD),
            fcntl(input, F_DUPFD_CLOEXEC, PARK_FD),
            fcntl(output, F_DUPFD_CLOEXEC, PARK_FD),
        ];
        if parked.iter().any(|fd| *fd < 0) {
            return None;
        }
        let [null, input, output] = parked;
        for (from, to) in [
            (null, 0),
            (null, 1),
            (null, 2),
            (input, INPUT_FD),
            (output, OUTPUT_FD),
        ] {
            if dup2(from, to) != to {
                return None;
            }
        }
        (close_range(FIRST_UNUSED_FD, u32::MAX, 0) == 0).then_some(())
    }
}

fn set_limit(resource: c_int, value: u64) -> Option<()> {
    let limit = RLimit {
        current: value,
        maximum: value,
    };
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
fn child(
    input: RawFd,
    output: RawFd,
    size: c_int,
    film_strip: bool,
    decoder: &Decoder,
    abi: i64,
) -> ! {
    let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match confine(input, output, abi) {
            None => CHILD_MACHINE,
            Some(()) => decoder.generate(size, film_strip),
        }
    }));
    unsafe { _exit(verdict.unwrap_or(CHILD_MACHINE)) }
}

#[path = "thumbworker_jobs.rs"]
mod jobs;
use jobs::serve;
#[cfg(test)]
use jobs::{finish, Running};

// The whole process: stdin is the request socket, and the first byte says whether this worker can serve.
pub fn run() -> i32 {
    let requests = unsafe { OwnedFd::from_raw_fd(0) };
    unsafe {
        prctl(
            PR_SET_DUMPABLE,
            0 as c_ulong,
            0 as c_ulong,
            0 as c_ulong,
            0 as c_ulong,
        )
    };
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
#[path = "thumbworker_cancel_tests.rs"]
mod cancel_tests;

#[cfg(test)]
#[path = "thumbworker_confine_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "thumbworker_filter_tests.rs"]
mod filter_tests;

#[cfg(test)]
#[path = "thumbworker_verdict_tests.rs"]
mod verdict_tests;
