// Issue 144: the stdin-reading thread sets the cancel flag here, so a slow listing cannot hold cancellation behind the loop.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// The operation on the thread and the flag it polls, or None while nothing is running.
pub(crate) struct Live(
    Mutex<Option<(usize, Arc<AtomicBool>)>>,
    pub Arc<DetachedJobs>,
);

impl Live {
    pub fn new() -> Live {
        Live(Mutex::new(None), Arc::new(DetachedJobs::new()))
    }

    pub fn claim(&self, id: usize, cancel: &Arc<AtomicBool>) {
        if let Ok(mut held) = self.0.lock() {
            *held = Some((id, Arc::clone(cancel)));
        }
    }

    pub fn finished(&self) {
        if let Ok(mut held) = self.0.lock() {
            *held = None;
        }
    }

    // A late Done releases only its own claim, so it never frees a newer operation's slot.
    pub fn finished_if(&self, id: usize) {
        if let Ok(mut held) = self.0.lock() {
            if held.as_ref().is_some_and(|(running, _)| *running == id) {
                *held = None;
            }
        }
    }

    pub fn running(&self) -> Option<usize> {
        self.0.lock().ok().and_then(|held| held.as_ref().map(|(id, _)| *id))
    }

    // The id is what keeps a cancel off the operation after the one it was aimed at, and the lock is
    // what keeps it off one claimed between the read and the store.
    pub fn cancel(&self, id: usize) {
        self.1.cancel(id);
        if let Ok(held) = self.0.lock() {
            if let Some((running, flag)) = held.as_ref() {
                if *running == id {
                    flag.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}

// Detached jobs, keyed by id, so a quit sets each flag; extracts use Live.
pub(crate) struct DetachedJobs(std::sync::Mutex<std::collections::HashMap<usize, Arc<AtomicBool>>>);

impl DetachedJobs {
    pub fn new() -> DetachedJobs {
        DetachedJobs(std::sync::Mutex::new(std::collections::HashMap::new()))
    }
    pub fn insert(&self, id: usize, cancel: &Arc<AtomicBool>) {
        if let Ok(mut held) = self.0.lock() {
            held.insert(id, Arc::clone(cancel));
        }
    }
    pub fn cancel(&self, id: usize) {
        if let Ok(held) = self.0.lock() {
            if let Some(flag) = held.get(&id) {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
    pub fn remove(&self, id: usize) {
        if let Ok(mut held) = self.0.lock() {
            held.remove(&id);
        }
    }
    // A quit sets every detached flag, so each job's Work cleanup runs.
    pub fn cancel_all(&self) {
        if let Ok(held) = self.0.lock() {
            for flag in held.values() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.lock().map(|held| held.is_empty()).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_reaches_the_running_operation_and_nothing_else() {
        let live = Live::new();
        let flag = Arc::new(AtomicBool::new(false));
        live.claim(7, &flag);
        // The stdin helper's own call, made while the loop is inside a read_dir on a FUSE mount.
        live.cancel(7);
        assert!(flag.load(Ordering::Relaxed), "the operation on the thread is the one a cancel reaches");
        assert_eq!(live.running(), Some(7));
    }

    #[test]
    fn a_cancel_for_an_operation_that_has_ended_never_reaches_the_next_one() {
        let live = Live::new();
        let first = Arc::new(AtomicBool::new(false));
        live.claim(7, &first);
        live.finished();
        let second = Arc::new(AtomicBool::new(false));
        live.claim(8, &second);
        live.cancel(7);
        assert!(!second.load(Ordering::Relaxed), "an id that is not running cancels nothing");
        assert!(!first.load(Ordering::Relaxed), "and the flag it was aimed at is not set either");
        live.cancel(8);
        assert!(second.load(Ordering::Relaxed), "the one that is running still takes its own cancel");
    }

    #[test]
    fn nothing_running_takes_no_cancel_at_all() {
        let live = Live::new();
        let flag = Arc::new(AtomicBool::new(false));
        live.claim(1, &flag);
        live.finished();
        live.cancel(1);
        assert_eq!(live.running(), None);
        assert!(!flag.load(Ordering::Relaxed), "a cancel with nothing running reaches the flag Live held");
        live.claim(1, &flag);
        assert!(!flag.load(Ordering::Relaxed), "and it does not carry over to the next claim of that id");
    }

    #[test]
    fn a_quit_cancel_reaches_every_detached_job_and_none_finished() {
        let jobs = DetachedJobs::new();
        assert!(jobs.is_empty());
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        jobs.insert(11, &first);
        jobs.insert(12, &second);
        assert!(!jobs.is_empty());
        jobs.cancel_all();
        assert!(first.load(Ordering::Relaxed) && second.load(Ordering::Relaxed));
        jobs.remove(11);
        assert!(!jobs.is_empty());
        jobs.remove(12);
        assert!(jobs.is_empty());
    }

    #[test]
    fn a_late_done_releases_only_its_own_claim() {
        let live = Live::new();
        let first = Arc::new(AtomicBool::new(false));
        live.claim(7, &first);
        live.finished_if(8);
        assert_eq!(live.running(), Some(7));
        live.finished_if(7);
        assert_eq!(live.running(), None);
    }
}
