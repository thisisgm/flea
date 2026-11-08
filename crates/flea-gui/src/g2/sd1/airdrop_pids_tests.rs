use super::*;
use std::os::unix::process::ExitStatusExt;
use std::time::{Duration, Instant};

const SIGTERM: i32 = 15;

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
    pids.hold(Proc::Radio, 2, child.id());
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
    pids.hold(Proc::Receive, 3, child.id());
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
    pids.hold(Proc::Radio, 1, radio.id());
    pids.hold(Proc::Receive, 1, receive.id());
    drop(pids);
    assert_eq!(ended_by(&mut radio), Some(SIGTERM));
    assert_eq!(ended_by(&mut receive), Some(SIGTERM));
}
