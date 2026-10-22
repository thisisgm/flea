//! Real children through ThreadRunner: the passphrase rides stdin only, a withheld input closes empty, and a kill lands fast.
use super::process::{Event, Job, Runner, ThreadRunner};
use super::secret::Secret;
use std::time::{Duration, Instant};

const CANARY: &str = "S3cr3t-canary-1006";

fn finish(
    job: Job,
    before: impl FnOnce(&super::process::Handle),
) -> (super::process::Outcome, bool) {
    let handle = ThreadRunner.start(job);
    before(&handle);
    let mut started = false;
    loop {
        match handle
            .events
            .recv_blocking()
            .expect("the runner always reports an exit")
        {
            Event::Started => started = true,
            Event::Exited(outcome) => return (outcome, started),
        }
    }
}

fn sh(script: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), script.into()]
}

#[test]
fn stdin_carries_the_secret_and_nothing_else_does() {
    let job = Job {
        argv: sh("cat; printf '|%s' \"$0\" \"$*\""),
        env: Vec::new(),
        stdin: Some(Secret::from_bytes(CANARY.as_bytes())),
    };
    assert!(
        !format!("{job:?}").contains(CANARY),
        "the job never prints the passphrase"
    );
    let (outcome, started) = finish(job, |_| {});
    assert!(started);
    assert_eq!(outcome.code, 0);
    assert_eq!(
        outcome.stdout,
        format!("{CANARY}|sh|"),
        "the child reads it on stdin and its argv holds none"
    );
}

#[test]
fn withheld_input_closes_stdin_empty() {
    let job = Job {
        argv: sh("sleep 0.2; cat; echo done"),
        env: Vec::new(),
        stdin: Some(Secret::from_bytes(CANARY.as_bytes())),
    };
    let (outcome, _) = finish(job, |h| h.withhold_input());
    assert_eq!(
        outcome.stdout, "done\n",
        "an empty EOF lets a cat-until-EOF child exit"
    );
}

#[test]
fn kill_ends_a_hung_child_within_its_bound() {
    let job = Job {
        argv: sh("sleep 30"),
        env: Vec::new(),
        stdin: None,
    };
    let at = Instant::now();
    let (outcome, _) = finish(job, |h| h.kill());
    let took = at.elapsed();
    assert_ne!(outcome.code, 0);
    assert!(
        took < Duration::from_secs(2),
        "cancel to exit took {took:?}"
    );
    println!("cancel-to-exit {took:?}");
}

#[test]
fn a_command_that_cannot_start_still_exits() {
    let job = Job {
        argv: vec!["/nonexistent/udisksctl".into()],
        env: Vec::new(),
        stdin: Some(Secret::from_bytes(b"x")),
    };
    let (outcome, started) = finish(job, |_| {});
    assert!(!started);
    assert_eq!(outcome.code, -1);
}

#[test]
fn c_locale_pins_the_english_output_udisksctl_is_parsed_from() {
    let job = Job::c_locale(sh("printf %s \"$LC_ALL\""));
    assert_eq!(finish(job, |_| {}).0.stdout, "C");
}

#[test]
fn a_killed_jobs_late_exit_never_clears_or_answers_for_a_newer_one() {
    use super::process::{Handle, Slot};
    let mut slot = Slot::default();
    let first = slot.start(Handle::detached().0);
    slot.kill();
    assert!(
        slot.handle().is_none(),
        "the kill drops the killed job's handle"
    );
    let second = slot.start(Handle::detached().0);
    assert!(
        !slot.ended(first),
        "the killed lsblk's exit is not the new poller's answer"
    );
    assert!(
        slot.handle().is_some(),
        "and it leaves the newer handle held, so a cancel can still kill it"
    );
    assert!(slot.ended(second), "the newer job's own exit ends it");
    assert!(slot.handle().is_none());
}

#[test]
fn a_replaced_jobs_late_exit_leaves_the_replacement_held() {
    use super::process::{Handle, Slot};
    let mut slot = Slot::default();
    let first = slot.start(Handle::detached().0);
    let second = slot.start(Handle::detached().0);
    assert!(!slot.ended(first));
    assert!(slot.handle().is_some());
    assert!(slot.ended(second));
}

#[test]
fn a_bound_stop_keeps_the_jobs_number_so_its_exit_still_answers() {
    use super::process::{Handle, Slot};
    let mut slot = Slot::default();
    let (handle, _send) = Handle::detached();
    let run = slot.start(handle);
    slot.stop();
    assert!(
        slot.handle().is_some_and(|h| h.killed()),
        "the bound kills the held lsblk"
    );
    assert!(
        slot.ended(run),
        "and its exit is still the current job's, so the poller lists again"
    );
}
