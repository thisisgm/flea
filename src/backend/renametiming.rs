// Opt-in protocol timing in a test child; production binaries contain none of this module.
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::Duration;

static ENABLED: AtomicBool = AtomicBool::new(false);
static RENAMING: AtomicBool = AtomicBool::new(false);
static LOOP_TID: AtomicI32 = AtomicI32::new(0);

#[repr(C)]
struct Timespec {
    seconds: std::ffi::c_long,
    nanos: std::ffi::c_long,
}

extern "C" {
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    fn gettid() -> i32;
}

const CLOCK_MONOTONIC: i32 = 1;
const CLOCK_THREAD_CPUTIME_ID: i32 = 3;

fn nanos(clock: i32) -> u64 {
    let mut time = Timespec {
        seconds: 0,
        nanos: 0,
    };
    assert_eq!(unsafe { clock_gettime(clock, &mut time) }, 0);
    time.seconds as u64 * 1_000_000_000 + time.nanos as u64
}

pub fn mark(stage: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if matches!(stage, "syscall_begin" | "journal_begin") {
        LOOP_TID.store(unsafe { gettid() }, Ordering::Relaxed);
    }
    let at = nanos(CLOCK_MONOTONIC);
    let cpu = nanos(CLOCK_THREAD_CPUTIME_ID);
    let sched = std::fs::read_to_string("/proc/thread-self/schedstat").unwrap_or_default();
    let delay = sched.split_whitespace().nth(1).unwrap_or("0");
    eprintln!(
        r#"{{"rename_timing":true,"stage":"{}","mono_ns":{},"cpu_ns":{},"runqueue_ns":{},"tid":{}}}"#,
        stage,
        at,
        cpu,
        delay,
        unsafe { gettid() }
    );
}

pub fn input(line: &str) {
    if ENABLED.load(Ordering::Relaxed) {
        match crate::json::field_str(line, "c").as_deref() {
            Some("rename") => {
                RENAMING.store(true, Ordering::Relaxed);
                mark("input_rename");
            }
            Some("list") => mark("input_list"),
            _ => {}
        }
    }
}

pub fn begin() {
    if ENABLED.load(Ordering::Relaxed) {
        RENAMING.store(true, Ordering::Relaxed);
        mark("dispatch_rename");
    }
}

pub fn end() {
    mark("rename_done");
    RENAMING.store(false, Ordering::Relaxed);
}

fn enable() -> thread::JoinHandle<()> {
    LOOP_TID.store(unsafe { gettid() }, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    thread::spawn(|| {
        let mut samples = 0;
        while ENABLED.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
            if !RENAMING.load(Ordering::Relaxed) || samples >= 200 {
                continue;
            }
            let tid = LOOP_TID.load(Ordering::Relaxed);
            let root = format!("/proc/self/task/{tid}");
            let wait = std::fs::read_to_string(format!("{root}/wchan")).unwrap_or_default();
            let call = std::fs::read_to_string(format!("{root}/syscall")).unwrap_or_default();
            eprintln!(
                r#"{{"rename_timing":true,"stage":"sample","mono_ns":{},"wait":"{}","syscall":"{}"}}"#,
                nanos(CLOCK_MONOTONIC),
                crate::json::escape(wait.trim()),
                crate::json::escape(call.trim())
            );
            samples += 1;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_child() {
        if std::env::var_os("FLEA_RENAME_TIMING_CHILD").is_none() {
            return;
        }
        let sampler = enable();
        let code = crate::backend::run::run();
        ENABLED.store(false, Ordering::Relaxed);
        sampler.join().expect("timing sampler stopped");
        assert_eq!(code, 0);
    }

    #[test]
    #[ignore = "manual twenty-run 100k protocol timing through the host broker"]
    fn protocol_100k_twenty() {
        let root = crate::backend::testdir::TestDir::new("rename-timing");
        let status = std::process::Command::new("python3")
            .arg("tests/rename-timing.py")
            .arg(std::env::current_exe().expect("the test binary"))
            .arg(root.path())
            .args(["--runs", "20", "--files", "100000", "--test-binary"])
            .status()
            .expect("protocol timing driver starts");
        assert!(status.success(), "protocol timing driver failed: {status}");
    }
}
