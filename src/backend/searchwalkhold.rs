// Test seam: parks one walk thread at a chosen directory until released, so a cancel or a close lands mid-walk.
use crate::backend::searchwalk::WalkProbes;
use std::path::Path;

impl WalkProbes {
    // Arms the hold on this walk's own probes; the walk sends `parked` once blocked.
    pub(crate) fn test_hold_walk_at(
        &self,
        at: usize,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (parked_tx, parked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *self.hold.lock().unwrap() = Some(WalkHold { at, dir: None, late: false, parked: parked_tx, release: release_rx });
        (parked_rx, release_tx)
    }

    // Arms the hold on the directory named `dir` instead of a count, so which directory parks never races the threads.
    pub(crate) fn test_hold_walk_in(&self, dir: &str) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (parked_rx, release_tx) = self.test_hold_walk_at(usize::MAX);
        if let Some(hold) = self.hold.lock().unwrap().as_mut() {
            hold.dir = Some(dir.to_string());
        }
        (parked_rx, release_tx)
    }

    // Arms the hold after the directory named `dir` is read and before its rows land, where a cancel can still come between.
    pub(crate) fn test_hold_walk_late_in(&self, dir: &str) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let held = self.test_hold_walk_in(dir);
        if let Some(hold) = self.hold.lock().unwrap().as_mut() {
            hold.late = true;
        }
        held
    }

    // Parks this walk once it has opened `at` directories, or as it opens the directory the hold names; other walks never see the hold.
    pub(crate) fn hold_if_due(&self, opened: usize, dir: &Path, late: bool) {
        let named = |hold: &WalkHold| hold.dir.as_deref().is_some_and(|name| dir.file_name() == Some(std::ffi::OsStr::new(name)));
        let due = self.hold.lock().unwrap().as_ref().is_some_and(|hold| hold.late == late && (opened >= hold.at || named(hold)));
        if due {
            let hold = self.hold.lock().unwrap().take();
            if let Some(hold) = hold {
                let _ = hold.parked.send(());
                let _ = hold.release.recv();
            }
        }
    }
}

// Test seam: holds the walk once it has opened `at` directories until released, so close lands mid-walk.
pub(crate) struct WalkHold {
    at: usize,
    dir: Option<String>,
    late: bool,
    parked: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

// Tests waiting for a walk or tag landing keep recursive notifications distinct from that completion.
pub(crate) fn wait_landed(rx: &std::sync::mpsc::Receiver<crate::backend::events::Event>, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(crate::backend::events::Event::SearchWalked) => return,
            Ok(crate::backend::events::Event::SearchDelta(_)) => {},
            _ => panic!("the expected search walk or tag landing did not arrive"),
        }
    }
}

pub(crate) fn assert_no_landing(rx: &std::sync::mpsc::Receiver<crate::backend::events::Event>, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(crate::backend::events::Event::SearchDelta(_)) => {},
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return,
            _ => panic!("an unexpected search walk or tag landing arrived"),
        }
    }
}

pub(crate) struct ReadHold {
    dir: String,
    after: usize,
    parked: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

impl WalkProbes {
    pub(crate) fn test_hold_scan_in(&self, dir: &str, after: usize) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (parked, wait) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel();
        *self.read_hold.lock().unwrap() = Some(ReadHold { dir: dir.to_string(), after, parked, release: resume });
        (wait, release)
    }

    pub(crate) fn hold_read_if_due(&self, dir: &Path, count: usize) {
        let due = self.read_hold.lock().unwrap().as_ref().is_some_and(|hold|
            dir.file_name() == Some(std::ffi::OsStr::new(&hold.dir)) && count >= hold.after);
        if due {
            if let Some(hold) = self.read_hold.lock().unwrap().take() {
                let _ = hold.parked.send(());
                let _ = hold.release.recv();
            }
        }
    }
}
