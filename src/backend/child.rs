use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

// poll(2) POLLIN, which for a pidfd is raised exactly once, when the child exits.
const POLLIN: i16 = 1;
// poll(2) fails with EINTR when a signal lands mid-wait, which is a retry and neither an exit nor a timeout.
const EINTR: i32 = 4;
// One descriptor is all this ever waits on, which is poll's nfds argument.
const ONE_FD: usize = 1;
// A whole millisecond is the finest timeout poll takes, and the remaining time is rounded up to one.
const NS_PER_MS: u128 = 1_000_000;

// poll(2) struct pollfd, whose three fields are int, short, short in that order.
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

// std already links the system libc, so the two symbols are declared here rather than taking a crate.
extern "C" {
    fn pidfd_open(pid: i32, flags: u32) -> i32;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGKILL: i32 = 9;

// A fork that failed under memory pressure says nothing about the file, so a child that never started is kept apart from one that ran and failed.
pub enum Ran {
    Succeeded,
    Failed,
    NotStarted,
    // A viewport that moved on, never a verdict on the file, so fail/ is not written.
    Cancelled,
}

// SIGKILL the pid and its process group: bwrap --new-session is a new leader, and --die-with-parent
// only fires if that leader actually dies. pid 0 is "not spawned yet" and is not a kill target.
pub fn kill_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    let p = pid as i32;
    unsafe {
        kill(p, SIGKILL);
        kill(-p, SIGKILL);
    }
}

// The deadline and both syscall failures have to end the child, and only the caller knows what its ending means.
fn kill_and_reap(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

// thumbargv builds the inner argv and sandbox wraps it; this runs the result and reports which of the three things happened.
#[cfg(test)]
pub fn run_with_timeout(full: &[String], limit: Duration) -> Ran {
    let cancel = AtomicBool::new(false);
    let pid = AtomicU32::new(0);
    run_cancellable(full, limit, &cancel, &pid)
}

// The pool's cancel path: the flag is what distinguishes a killed child from a decoder failure, and
// the pid is what lets another thread wake this poll by killing the child.
pub fn run_cancellable(full: &[String], limit: Duration, cancel: &AtomicBool, pid_slot: &AtomicU32) -> Ran {
    if cancel.load(Ordering::Acquire) {
        return Ran::Cancelled;
    }
    let mut cmd = std::process::Command::new(&full[0]);
    cmd.args(&full[1..]);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return Ran::NotStarted,
    };
    pid_slot.store(child.id(), Ordering::Release);
    if cancel.load(Ordering::Acquire) {
        kill_and_reap(&mut child);
        pid_slot.store(0, Ordering::Release);
        return Ran::Cancelled;
    }
    // A child that exited before this line is a zombie std has not waited on, so the pid is still ours and cannot have been reused.
    let raw = unsafe { pidfd_open(child.id() as i32, 0) };
    if raw < 0 {
        // corner: a descriptor this process could not open is the machine's fault and never the file's, so no marker is recorded, see AGENTS.md "Thumbnail pool".
        kill_and_reap(&mut child);
        pid_slot.store(0, Ordering::Release);
        return Ran::NotStarted;
    }
    // OwnedFd closes on Drop, so every path out of this function releases the descriptor without anyone remembering to.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    let deadline = Instant::now() + limit;
    loop {
        if cancel.load(Ordering::Acquire) {
            kill_and_reap(&mut child);
            pid_slot.store(0, Ordering::Release);
            return Ran::Cancelled;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        // Rounded up, not truncated, so a sub-millisecond remainder is waited out instead of truncating to a zero-timeout poll; left is recomputed against the fixed deadline every round, so nothing accumulates.
        let ms = left.as_nanos().div_ceil(NS_PER_MS).min(i32::MAX as u128) as i32;
        let mut fds = PollFd { fd: pidfd.as_raw_fd(), events: POLLIN, revents: 0 };
        let ready = unsafe { poll(&mut fds, ONE_FD, ms) };
        if ready > 0 && fds.revents & POLLIN != 0 {
            break;
        }
        if ready == 0 {
            // A decoder still running at the deadline is one of the two paths that record a marker, the other being a non-zero exit.
            kill_and_reap(&mut child);
            pid_slot.store(0, Ordering::Release);
            return Ran::Failed;
        }
        // corner: a ready descriptor with no POLLIN cannot happen for a pidfd and a poll error is the machine's, so neither judges the file, see AGENTS.md "Thumbnail pool".
        if ready > 0 || std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
            kill_and_reap(&mut child);
            pid_slot.store(0, Ordering::Release);
            return Ran::NotStarted;
        }
    }
    pid_slot.store(0, Ordering::Release);
    if cancel.load(Ordering::Acquire) {
        let _ = child.wait();
        return Ran::Cancelled;
    }
    // poll says only that the child exited, so the status itself still comes from wait.
    verdict(&mut child)
}

// Reading the status is its own function so a test can reap the child first and drive the wait that fails.
fn verdict(child: &mut std::process::Child) -> Ran {
    match child.wait() {
        Ok(status) if status.success() => Ran::Succeeded,
        // A child that exited non-zero ran on the file, which is the decoder's own verdict on it.
        Ok(_) => Ran::Failed,
        // corner: a wait this process could not complete is the machine's fault and never the file's, so no marker is recorded, see AGENTS.md "Thumbnail pool".
        Err(_) => Ran::NotStarted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The pool's shipped deadline, repeated here so a test that must not hit one says which one it means.
    const A_LONG_LIMIT: Duration = Duration::from_secs(20);

    #[test]
    fn a_hung_child_is_killed_at_the_deadline() {
        let full = vec!["/usr/bin/sleep".to_string(), "600".to_string()];
        let started = Instant::now();
        // thumbs.rs ships JOB_TIMEOUT at 20 s, so this pins the kill to whatever deadline it was given.
        assert!(matches!(run_with_timeout(&full, Duration::from_millis(300)), Ran::Failed));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_child_that_never_started_is_told_apart_from_one_that_ran_and_failed() {
        // A missing program never touched the file; /usr/bin/false ran and exited non-zero, which is what a marker is for.
        let one = |p: &str| run_with_timeout(&[p.to_string()], A_LONG_LIMIT);
        assert!(matches!(one("/definitely/not/here"), Ran::NotStarted));
        assert!(matches!(one("/usr/bin/false"), Ran::Failed));
        assert!(matches!(one("/usr/bin/true"), Ran::Succeeded));
    }

    // Declared here and not beside pidfd_open, because only this test reaps a child the production code owns.
    extern "C" {
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    }

    #[test]
    fn a_failed_wait_is_told_apart_from_a_child_that_ran_and_failed() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id() as i32;
        let mut status: i32 = 0;
        // Reaping the child here is what leaves the verdict's own wait nothing to reap, which is the ECHILD it cannot otherwise be shown.
        assert_eq!(unsafe { waitpid(pid, &mut status, 0) }, pid);
        assert!(matches!(verdict(&mut child), Ran::NotStarted));
    }

    #[test]
    fn a_child_is_noticed_when_it_exits_rather_than_at_the_next_poll_boundary() {
        // The child sleeps 30 ms, which a 25 ms poll step could only notice at its 50 ms boundary.
        const CHILD_SLEEP: Duration = Duration::from_millis(30);
        // Nine runs, so a single scheduling stall cannot move the fifth value.
        const RUNS: usize = 9;
        // Above the exact wait's low single digits and well under the old step's 20 ms, so it is neither flaky nor vacuous.
        const BOUND: Duration = Duration::from_millis(10);
        // The argv is derived from the constant, so raising one cannot silently leave the other behind.
        let full = vec!["/usr/bin/sleep".to_string(), format!("{:.3}", CHILD_SLEEP.as_secs_f64())];
        let mut overshoot: Vec<Duration> = Vec::new();
        for _ in 0..RUNS {
            let started = Instant::now();
            assert!(matches!(run_with_timeout(&full, A_LONG_LIMIT), Ran::Succeeded));
            let took = started.elapsed();
            // The lower bound is what stops a shortened child from making every overshoot zero and the test vacuous.
            assert!(took >= CHILD_SLEEP, "the child returned before its own sleep, at {:?}", took);
            overshoot.push(took - CHILD_SLEEP);
        }
        overshoot.sort();
        let median = overshoot[RUNS / 2];
        assert!(median < BOUND, "median overshoot was {:?} over {} runs", median, RUNS);
    }

    #[test]
    fn a_descriptor_still_opens_on_a_child_that_exited_before_the_wait_began() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        // Nothing here reaps it, so this is exactly the zombie state run_with_timeout can find its child in.
        std::thread::sleep(Duration::from_millis(200));
        let raw = unsafe { pidfd_open(child.id() as i32, 0) };
        assert!(raw >= 0, "pidfd_open on an unreaped exited child failed");
        let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut fds = PollFd { fd: pidfd.as_raw_fd(), events: POLLIN, revents: 0 };
        assert_eq!(unsafe { poll(&mut fds, ONE_FD, 0) }, 1);
        assert!(fds.revents & POLLIN != 0);
        assert!(child.wait().unwrap().success());
    }

    // A no-op handler, because a signal with no handler is not delivered and so never interrupts poll.
    extern "C" fn on_alarm(_sig: i32) {}

    // Declared here and not beside pidfd_open, because only this test aims a signal at a thread.
    extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
        fn pthread_self() -> u64;
        fn pthread_kill(thread: u64, sig: i32) -> i32;
    }

    // Fires SIGALRM at the calling thread five times, so several land inside whatever wait that thread then enters.
    fn signal_this_thread_five_times() {
        // SIGALRM, whose default action is to terminate, so a no-op handler is the safe thing to install.
        const SIGALRM: i32 = 14;
        const SIGNALS: usize = 5;
        const APART: Duration = Duration::from_millis(20);
        let handler = on_alarm as extern "C" fn(i32);
        assert_ne!(unsafe { signal(SIGALRM, handler as usize) }, usize::MAX);
        // The signal has to land on the thread sitting in poll, so it is aimed at this thread and not at the process.
        let waiting = unsafe { pthread_self() };
        std::thread::spawn(move || {
            for _ in 0..SIGNALS {
                std::thread::sleep(APART);
                unsafe { pthread_kill(waiting, SIGALRM) };
            }
        });
    }

    #[test]
    fn a_signal_during_the_wait_is_a_retry_and_not_a_kill() {
        // Long enough that four of the five signals land while the child is still running.
        const CHILD_SLEEP: Duration = Duration::from_millis(200);
        signal_this_thread_five_times();
        let full = vec!["/usr/bin/sleep".to_string(), format!("{:.3}", CHILD_SLEEP.as_secs_f64())];
        let started = Instant::now();
        let ran = run_with_timeout(&full, A_LONG_LIMIT);
        let took = started.elapsed();
        assert!(matches!(ran, Ran::Succeeded), "the child was killed rather than waited for again");
        assert!(took >= CHILD_SLEEP, "the child was cut short at {:?}", took);
    }

    #[test]
    fn a_signal_does_not_hand_the_deadline_over_to_a_blocking_wait() {
        // A deadline far shorter than the child, so a retry that dropped the deadline would run the whole 2 s.
        const LIMIT: Duration = Duration::from_millis(150);
        const CHILD_SLEEP: Duration = Duration::from_secs(2);
        // Halfway between the deadline and the child, so neither a slow box nor a fast one can decide the verdict.
        const STILL_DEADLINED: Duration = Duration::from_millis(1000);
        signal_this_thread_five_times();
        let full = vec!["/usr/bin/sleep".to_string(), format!("{:.3}", CHILD_SLEEP.as_secs_f64())];
        let started = Instant::now();
        let ran = run_with_timeout(&full, LIMIT);
        let took = started.elapsed();
        assert!(matches!(ran, Ran::Failed), "the deadline was lost to the EINTR retries");
        assert!(took >= LIMIT, "the deadline fired early at {:?}", took);
        assert!(took < STILL_DEADLINED, "the deadline was lost to the EINTR retries, at {:?}", took);
    }
}
