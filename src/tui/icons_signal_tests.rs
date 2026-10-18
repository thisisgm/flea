use super::*;

static TRACE_KILL: AtomicBool = AtomicBool::new(false);
static REAPED_KILL: AtomicBool = AtomicBool::new(false);
static GROUP_KILLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[export_name = "kill"]
unsafe extern "C" fn trace_kill(pid: i32, signal: i32) -> i32 {
    if TRACE_KILL.load(Ordering::Relaxed) && pid < -1 {
        let reaped = !std::path::Path::new(&format!("/proc/{}", -pid)).exists();
        REAPED_KILL.store(reaped, Ordering::Relaxed);
        GROUP_KILLS.fetch_add(1, Ordering::Relaxed);
        println!("intercepted group signal {signal}; leader already absent from /proc: {reaped}");
        return 0;
    }
    extern "C" { fn syscall(number: std::ffi::c_long, ...) -> std::ffi::c_long; }
    #[cfg(target_arch = "x86_64")]
    const KILL_SYSCALL: std::ffi::c_long = 62;
    #[cfg(target_arch = "aarch64")]
    const KILL_SYSCALL: std::ffi::c_long = 129;
    unsafe { syscall(KILL_SYSCALL, pid, signal) as i32 }
}
#[test]
fn never_signal_a_reaped_group_leader() {
    if crate::tui::render::frame_tests::isolated_test("tui::icons::tests::signal_tests::never_signal_a_reaped_group_leader", &[]) { return; }
    REAPED_KILL.store(false, Ordering::Relaxed);
    GROUP_KILLS.store(0, Ordering::Relaxed);
    TRACE_KILL.store(true, Ordering::Relaxed);
    let result = output(Command::new("sh").args(["-c", "printf covered"]), &AtomicBool::new(false));
    TRACE_KILL.store(false, Ordering::Relaxed);
    assert_eq!(result.as_deref(), Some("covered"));
    assert_eq!(GROUP_KILLS.load(Ordering::Relaxed), 1, "the group signal never reached the traced kill, so nothing here was measured");
    assert!(!REAPED_KILL.load(Ordering::Relaxed), "group kill used a leader PID already reaped by try_wait");
}
