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
const KEEP_TIMES: [Timespec; 2] = [
    Timespec {
        seconds: 0,
        nanoseconds: UTIME_OMIT,
    },
    Timespec {
        seconds: 0,
        nanoseconds: UTIME_OMIT,
    },
];
// What Landlock answers for a right its ruleset handles and no rule grants.
const EACCES: u32 = 13;
// ioctl(2) FS_IOC_GETFLAGS, a read that only the filter refuses with EPERM.
const FS_IOC_GETFLAGS: usize = 0x8008_6601;

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
    fn fsetxattr(
        fd: c_int,
        name: *const c_char,
        value: *const c_void,
        size: usize,
        flags: c_int,
    ) -> c_int;
    fn ioctl(fd: i32, request: usize, ...) -> i32;
}

// 0 for a call that worked, otherwise the errno it set.
fn errno_of(result: c_int) -> i32 {
    if result == 0 {
        0
    } else {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
    }
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

fn soft_limit(resource: c_int) -> u64 {
    let mut limit = RLimit {
        current: 0,
        maximum: 0,
    };
    unsafe { getrlimit(resource, &mut limit) };
    limit.current
}

// What descriptor 0, 1 or 2 points at, such as "/dev/null" or "socket:[123]".
fn target(fd: c_int) -> String {
    std::fs::read_link(format!("/proc/self/fd/{}", fd))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// Holds the input read-only, the report write-only and a socket on 0, the way a job arrives in the worker, runs confine() and writes what it can still do to the report.
fn probe(dir: &str, busy: bool) -> ! {
    let victim = format!("{}/victim.mp4", dir);
    let opened = std::fs::File::open(&victim)
        .expect("the probe could not open its input")
        .into_raw_fd();
    let written = std::fs::OpenOptions::new()
        .write(true)
        .open(format!("{}/report", dir))
        .expect("the probe could not open its report")
        .into_raw_fd();
    let (reader, report) = if busy {
        let moved = unsafe {
            (
                fcntl(opened, F_DUPFD_CLOEXEC, BUSY_FD),
                fcntl(written, F_DUPFD_CLOEXEC, BUSY_FD),
            )
        };
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
    let mode = std::fs::metadata(&victim)
        .map(|m| m.permissions().mode() & MODE_BITS)
        .unwrap_or(0);
    let chmod_before = unsafe { fchmod(reader, mode) } == 0;
    let times_before = unsafe { futimens(reader, &KEEP_TIMES) } == 0;
    let length = std::fs::metadata(&victim)
        .map(|m| m.len() as i64)
        .unwrap_or(-1);
    let victim_path = std::ffi::CString::new(victim.as_str()).expect("the victim path holds a NUL");
    let truncate_before = unsafe { truncate(victim_path.as_ptr(), length) } == 0;
    let fork_before = forked_and_reaped() == 0;
    if confine(reader, report, abi).is_none() {
        unsafe { _exit(CHILD_MACHINE) };
    }
    let chmod = unsafe { fchmod(INPUT_FD, mode) } == 0;
    let times = unsafe { futimens(INPUT_FD, &KEEP_TIMES) } == 0;
    let truncate_errno = errno_of(unsafe { truncate(INPUT_PATH.as_ptr(), 0) });
    let xattr = errno_of(unsafe {
        fsetxattr(
            INPUT_FD,
            c"user.flea_probe".as_ptr(),
            b"1".as_ptr() as *const c_void,
            1,
            0,
        )
    });
    let mut flags: c_long = 0;
    let ioctl_errno = errno_of(unsafe { ioctl(INPUT_FD, FS_IOC_GETFLAGS, &mut flags) });
    let fork_errno = forked_and_reaped();
    let thread = std::thread::Builder::new()
        .spawn(|| true)
        .map(|handle| handle.join().unwrap_or(false))
        .unwrap_or(false);
    let open: Vec<c_int> = (0..FD_CEILING)
        .filter(|fd| unsafe { fcntl(*fd, F_GETFD, 0 as c_int) } >= 0)
        .collect();
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
            std::io::stderr()
                .write_all(
                    format!(
                        "SKIP {}: this kernel has no Landlock that can deny a truncation\n",
                        THIS_TEST
                    )
                    .as_bytes(),
                )
                .ok();
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
        assert_eq!(
            std::fs::read_to_string(&report).unwrap(),
            expected,
            "busy {} probe exited {:?}: {}",
            busy,
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"original");
    }
}

#[test]
fn a_library_that_is_not_there_is_refused() {
    assert!(Decoder::load(c"libflea-definitely-not-here.so.9").is_none());
}

#[test]
fn the_ruleset_denies_truncation_and_never_a_read() {
    assert_ne!(
        LANDLOCK_WRITE_RIGHTS & LANDLOCK_TRUNCATE_V3,
        0,
        "a job could truncate its input"
    );
    // Execute, read file and read dir are bits 0, 2 and 3, and none of them may ever be denied.
    let reads = (1 << 0) | (1 << 2) | (1 << 3);
    assert_eq!(LANDLOCK_WRITE_RIGHTS & reads, 0);
}
