use super::*;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

// A run of our own to signal: the test never names a pid it did not start.
fn sleeper() -> std::process::Child {
    Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .expect("sleep starts")
}

// Waits for the run to end, at most five seconds, so a missed signal fails rather than hangs.
fn ended_by(child: &mut std::process::Child) -> Option<i32> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(status) = child.try_wait().expect("the child can be waited on") {
            return status.signal();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

#[test]
fn an_exit_from_a_run_already_replaced_keeps_the_current_runs_pid() {
    let mut child = sleeper();
    let mut pids = Pids::default();
    started(&pids.claim(Proc::Radio, 2), child.id());
    pids.ended(Proc::Radio, 1);
    pids.stop(Proc::Radio);
    assert_eq!(
        ended_by(&mut child),
        Some(SIGTERM),
        "the current run is still signalled"
    );
}

#[test]
fn a_run_whose_exit_was_heard_is_never_signalled() {
    let mut child = sleeper();
    let mut pids = Pids::default();
    started(&pids.claim(Proc::Receive, 3), child.id());
    pids.ended(Proc::Receive, 3);
    pids.stop(Proc::Receive);
    std::thread::sleep(Duration::from_millis(300));
    let alive = child
        .try_wait()
        .expect("the child can be waited on")
        .is_none();
    let _ = child.kill();
    let _ = child.wait();
    assert!(alive, "stop signalled a pid whose exit was already heard");
}

#[test]
fn a_window_closing_with_airdrop_on_terminates_every_held_run() {
    let (mut radio, mut receive) = (sleeper(), sleeper());
    let mut pids = Pids::default();
    started(&pids.claim(Proc::Radio, 1), radio.id());
    started(&pids.claim(Proc::Receive, 1), receive.id());
    drop(pids);
    assert_eq!(ended_by(&mut radio), Some(SIGTERM));
    assert_eq!(ended_by(&mut receive), Some(SIGTERM));
}

#[test]
fn a_run_stopped_before_its_thread_spawned_it_is_terminated_once_it_starts() {
    let mut pids = Pids::default();
    let run = pids.claim(Proc::Radio, 4);
    pids.stop(Proc::Radio);
    let mut child = sleeper();
    started(&run, child.id());
    assert_eq!(ended_by(&mut child), Some(SIGTERM));
}

#[test]
fn a_window_closing_before_a_run_started_still_terminates_it() {
    let mut pids = Pids::default();
    let run = pids.claim(Proc::Browse, 1);
    drop(pids);
    let mut child = sleeper();
    started(&run, child.id());
    assert_eq!(ended_by(&mut child), Some(SIGTERM));
}

// A stand-in radio in a marked root: on SIGTERM it takes `secs` to give Wi-Fi back, then writes `gave-back`.
fn slow_radio(tag: &str, secs: &str) -> (std::path::PathBuf, std::process::Child) {
    let root = std::env::temp_dir().join(format!("flea-pids-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    let done = root.join("gave-back");
    let script = format!(
        "trap 'sleep {secs}; echo yes > \"{}\"; exit 0' TERM; echo up > \"{}\"; while :; do sleep 0.05; done",
        done.display(),
        root.join("up").display()
    );
    let child = Command::new("sh")
        .args(["-c", &script])
        .stdin(Stdio::null())
        .spawn()
        .expect("sh starts");
    let started = Instant::now();
    while !root.join("up").exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    (root, child)
}

#[test]
fn a_window_closing_waits_for_owly_to_give_wifi_back() {
    let (root, mut radio) = slow_radio("close", "0.4");
    let mut pids = Pids::default();
    started(&pids.claim(Proc::Radio, 1), radio.id());
    drop(pids);
    let gave_back = root.join("gave-back").exists();
    let _ = radio.wait();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        gave_back,
        "the window let go before the radio's teardown finished"
    );
}

#[test]
fn a_radio_that_never_exits_holds_a_closing_window_only_the_teardown_bound() {
    let (root, mut radio) = slow_radio("stuck", "30");
    let mut pids = Pids::default();
    started(&pids.claim(Proc::Radio, 1), radio.id());
    let started = Instant::now();
    drop(pids);
    let held = started.elapsed();
    let _ = radio.kill();
    let _ = radio.wait();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        held >= TEARDOWN && held < TEARDOWN + Duration::from_secs(2),
        "{held:?}"
    );
}

// Runs only as the child of the next test: holds a slow radio, then SIGTERMs itself.
#[test]
fn sigterm_child() {
    let Some(root) = std::env::var_os("FLEA_PIDS_SIGTERM_ROOT") else {
        return;
    };
    let mut pids = Pids::default();
    let pid: u32 = std::fs::read_to_string(std::path::Path::new(&root).join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    started(&pids.claim(Proc::Radio, 1), pid);
    // SAFETY: the test process signals itself; the handler started() installed answers it.
    unsafe { raise(SIGTERM) };
    unreachable!("SIGTERM's default action ends the process once owly is down");
}

#[test]
fn flea_killed_by_sigterm_waits_for_owly_to_give_wifi_back_then_dies_by_it() {
    let (root, mut radio) = slow_radio("sigterm", "0.4");
    std::fs::write(root.join("pid"), radio.id().to_string()).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "g2::send_share::airdrop_pids::tests::sigterm_child",
            "--test-threads=1",
        ])
        .env("FLEA_PIDS_SIGTERM_ROOT", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let gave_back = root.join("gave-back").exists();
    let _ = radio.kill();
    let _ = radio.wait();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(status.signal(), Some(SIGTERM), "{status:?}");
    assert!(gave_back, "Flea died before the radio gave Wi-Fi back");
}

// Forcing the pid's reuse needs its own pid namespace (`unshare -Urpf --mount-proc`); elsewhere it checks the Stop alone.
#[test]
fn a_stop_after_the_reap_never_signals_the_pids_next_owner() {
    let mut pids = Pids::default();
    let run = pids.claim(Proc::Browse, 1);
    let mut gone = Command::new("true").stdin(Stdio::null()).spawn().unwrap();
    let pid = gone.id();
    started(&run, pid);
    // The Stop lands after the kernel reaped the run and before its thread calls reaped(), where only the pidfd guards.
    gone.wait().unwrap();
    let reused = std::fs::write("/proc/sys/kernel/ns_last_pid", (pid - 1).to_string()).is_ok();
    let mut victim = sleeper();
    if reused {
        assert_eq!(victim.id(), pid, "the kernel handed the reaped pid on");
    }
    pids.stop(Proc::Browse);
    std::thread::sleep(Duration::from_millis(300));
    let alive = victim.try_wait().unwrap().is_none();
    let _ = victim.kill();
    let _ = victim.wait();
    assert!(
        alive,
        "a process holding pid {pid} after the reap was signalled"
    );
    reaped(&run);
    let held = run.lock().unwrap();
    assert!(
        held.pidfd.is_none() && held.pid.is_none(),
        "the reap leaves nothing a later Stop could signal"
    );
}
