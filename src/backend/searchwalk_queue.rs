// The walk's shared directory queue: a thread waits while another may still queue more, and a cancel ends the wait.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

// How long an idle walk thread waits before it rechecks the cancel flag; every queue change notifies sooner.
const CANCEL_RECHECK: Duration = Duration::from_millis(50);

// Pops the next directory, waiting while a thread that holds one may still queue more.
pub(super) fn next_dir<T>(
    work: &Mutex<(VecDeque<T>, usize)>,
    ready: &Condvar,
    cancel: &AtomicBool,
) -> Option<T> {
    let mut state = work.lock().unwrap();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(dir) = state.0.pop_front() {
            state.1 += 1;
            return Some(dir);
        }
        if state.1 == 0 {
            return None;
        }
        state = ready.wait_timeout(state, CANCEL_RECHECK).unwrap().0;
    }
}
