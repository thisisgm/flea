// The directories the columns view peeks at, watched so a neighbour column follows an outside change; see docs/protocol.md "changed".
use crate::backend::events::Event;
use crate::backend::peekpump::pump;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};
use crate::backend::events::Sender;
use std::thread::{self, JoinHandle};

// Parent, grandparent, great-grandparent and the child column, plus room for one navigation's leftovers.
const PEEK_WATCH_CAP: usize = 8;

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn eventfd(initval: u32, flags: c_int) -> c_int;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
}
// IN_CLOEXEC is O_CLOEXEC, so no thumbnailer child inherits this descriptor.
const IN_CLOEXEC: c_int = 0x0008_0000;
// EFD_CLOEXEC is the same bit, and an eventfd read by poll alone never needs the counter drained.
const EFD_CLOEXEC: c_int = 0x0008_0000;
// An eventfd is written an 8 byte counter, and any nonzero one makes it readable.
const STOP_TICKET: u64 = 1;

// Its own inotify descriptor, so a descriptor removed here can never be the listed directory's own watch.
pub struct PeekWatch {
    pump: super::lazy::Lazy<Pump>,
    held: VecDeque<(c_int, PathBuf)>,
    // The column directories the client draws now, once it has said; None until its first keep.
    shown: Option<Vec<PathBuf>>,
}

struct Pump {
    fd: c_int,
    // Written once on drop, so the pump thread wakes from its poll and ends before the descriptor closes.
    stop: c_int,
    pumper: Option<JoinHandle<()>>,
}

impl PeekWatch {
    // A box with no inotify keeps its neighbour columns as live as before.
    pub fn start(tx: Sender<Event>) -> PeekWatch {
        PeekWatch { pump: super::lazy::Lazy::new(move || Pump::start(tx)), held: VecDeque::new(), shown: None }
    }

    // The peek worker arms with this descriptor, so a dead mount never blocks the loop.
    pub fn raw_fd(&self) -> c_int {
        self.pump.fd
    }

    // Only a directory the client still draws stays watched, so a column on screen never loses its watch to one scrolled past.
    pub fn keep(&mut self, shown: Vec<PathBuf>) {
        let fd = self.pump.get().map_or(-1, |pump| pump.fd);
        self.held.retain(|(wd, path)| {
            let drawn = shown.contains(path);
            if !drawn {
                unsafe { inotify_rm_watch(fd, *wd) };
            }
            drawn
        });
        self.shown = Some(shown);
    }

    // A path armed twice answers the descriptor it already had, so it is held once; the oldest goes past the cap, a guard behind keep.
    pub fn register(&mut self, wd: c_int, path: PathBuf) {
        if wd < 0 || self.held.iter().any(|(held, _)| *held == wd) {
            return;
        }
        // A peek answered after the client scrolled on arms a column nobody draws any more.
        if self.shown.as_ref().is_some_and(|shown| !shown.contains(&path)) {
            unsafe { inotify_rm_watch(self.pump.fd, wd) };
            return;
        }
        self.held.push_back((wd, path));
        while self.held.len() > PEEK_WATCH_CAP {
            if let Some((old, _)) = self.held.pop_front() {
                unsafe { inotify_rm_watch(self.pump.fd, old) };
            }
        }
    }

    // The kernel dropped this watch itself, a deleted or unmounted directory, so it stops counting toward the cap.
    pub fn forget(&mut self, wd: c_int) {
        self.held.retain(|(held, _)| *held != wd);
    }

    // A removed watch's own late IN_IGNORED names a descriptor nobody holds, so it answers nothing.
    pub fn path_of(&self, wd: c_int) -> Option<&Path> {
        self.held.iter().find(|(held, _)| *held == wd).map(|(_, path)| path.as_path())
    }
}

impl Pump {
    fn start(tx: Sender<Event>) -> Self {
        let fd = unsafe { inotify_init1(IN_CLOEXEC) };
        let stop = if fd >= 0 { unsafe { eventfd(0, EFD_CLOEXEC) } } else { -1 };
        if fd >= 0 && stop < 0 {
            unsafe { close(fd) };
        }
        if stop < 0 {
            return Pump { fd: -1, stop: -1, pumper: None };
        }
        let pumper = Some(thread::spawn(move || pump(fd, tx, stop)));
        Pump { fd, stop, pumper }
    }

}

// The pump is joined before either descriptor closes, so a closed number is never read by a thread still running.
impl Drop for Pump {
    fn drop(&mut self) {
        if let Some(pumper) = self.pumper.take() {
            unsafe { write(self.stop, (&STOP_TICKET as *const u64).cast(), std::mem::size_of::<u64>()) };
            let _ = pumper.join();
        }
        for fd in [self.fd, self.stop] {
            if fd >= 0 {
                unsafe { close(fd) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::watch::Watch;
    use crate::backend::events::channel;
    use std::time::Duration;

    // Bounds the wait for a kernel event; the test asserts what arrived, never how long it took.
    const EVENT_WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn an_outside_create_in_a_peeked_directory_names_that_directory() {
        let d = TestDir::new("peekwatchcreate");
        let peeked = d.dir("peeked");
        let (tx, rx) = channel();
        let mut watch = PeekWatch::start(tx);
        let wd = Watch::add_raw(watch.raw_fd(), &peeked);
        assert!(wd >= 0, "a local directory can be watched");
        watch.register(wd, peeked.clone());
        std::fs::write(peeked.join("new.txt"), "x").unwrap();
        let Event::PeekChanged(seen) = rx.recv_timeout(EVENT_WAIT).expect("a create answers an event") else { panic!("not a peek event") };
        assert_eq!(watch.path_of(seen), Some(peeked.as_path()));
    }

    #[test]
    fn the_same_directory_armed_twice_is_held_once() {
        let d = TestDir::new("peekwatchtwice");
        let peeked = d.dir("peeked");
        let (tx, _rx) = channel();
        let mut watch = PeekWatch::start(tx);
        let (first, second) = (Watch::add_raw(watch.raw_fd(), &peeked), Watch::add_raw(watch.raw_fd(), &peeked));
        assert_eq!(first, second, "inotify answers the descriptor it already holds");
        watch.register(first, peeked.clone());
        watch.register(second, peeked.clone());
        assert_eq!(watch.held.len(), 1);
    }

    #[test]
    fn the_oldest_directory_is_unwatched_past_the_cap() {
        let d = TestDir::new("peekwatchcap");
        let (tx, _rx) = channel();
        let mut watch = PeekWatch::start(tx);
        let mut armed = Vec::new();
        for n in 0..=PEEK_WATCH_CAP {
            let dir = d.dir(&format!("d{n}"));
            let wd = Watch::add_raw(watch.raw_fd(), &dir);
            watch.register(wd, dir.clone());
            armed.push((wd, dir));
        }
        assert_eq!(watch.held.len(), PEEK_WATCH_CAP);
        assert_eq!(watch.path_of(armed[0].0), None, "the oldest watch was dropped");
        assert_eq!(watch.path_of(armed[PEEK_WATCH_CAP].0), Some(armed[PEEK_WATCH_CAP].1.as_path()));
    }

    #[test]
    fn no_descriptor_is_held() {
        let (tx, _rx) = channel();
        let mut watch = PeekWatch::start(tx);
        watch.register(-1, PathBuf::from("/nowhere"));
        assert_eq!(watch.held.len(), 0);
    }

    // Sample input: three columns slide over eleven directories; a column already answered is never re-peeked, so never re-armed.
    #[test]
    fn a_column_still_shown_keeps_its_watch_while_the_others_scroll_past() {
        let d = TestDir::new("peekwatchscroll");
        let (tx, rx) = channel();
        let mut watch = PeekWatch::start(tx);
        let dirs: Vec<PathBuf> = (0..=PEEK_WATCH_CAP + 2).map(|n| d.dir(&format!("d{n}"))).collect();
        let anchor = dirs[0].clone();
        let (mut anchor_wd, mut asked) = (-1, Vec::new());
        for slide in 1..dirs.len() {
            let shown = vec![anchor.clone(), dirs[slide - 1].clone(), dirs[slide].clone()];
            watch.keep(shown.clone());
            for dir in shown.iter() {
                if asked.contains(dir) {
                    continue;
                }
                let wd = Watch::add_raw(watch.raw_fd(), dir);
                watch.register(wd, dir.clone());
                asked.push(dir.clone());
                if *dir == anchor {
                    anchor_wd = wd;
                }
            }
        }
        assert_eq!(watch.path_of(anchor_wd), Some(anchor.as_path()), "a column on screen keeps its watch");
        assert_eq!(watch.held.len(), 3, "only the columns drawn are held");
        std::fs::write(anchor.join("new.txt"), "x").unwrap();
        // The unwatched columns' own IN_IGNORED arrive first and name descriptors nobody holds.
        let named = loop {
            let Event::PeekChanged(seen) = rx.recv_timeout(EVENT_WAIT).expect("a create in the still shown column answers an event") else { continue };
            if let Some(path) = watch.path_of(seen) {
                break path.to_path_buf();
            }
        };
        assert_eq!(named, anchor);
    }

    // A peek whose reply lands after the client scrolled on arms a directory nobody draws any more.
    #[test]
    fn a_late_arm_of_a_column_no_longer_shown_is_unwatched_at_once() {
        let d = TestDir::new("peekwatchlate");
        let (tx, _rx) = channel();
        let mut watch = PeekWatch::start(tx);
        let (shown, gone) = (d.dir("shown"), d.dir("gone"));
        watch.keep(vec![shown.clone()]);
        let wd = Watch::add_raw(watch.raw_fd(), &gone);
        watch.register(wd, gone);
        assert_eq!(watch.held.len(), 0);
        assert_eq!(watch.path_of(wd), None);
    }

    #[test]
    fn a_deleted_peeked_directory_leaves_the_held_set() {
        let d = TestDir::new("peekwatchdeleted");
        let peeked = d.dir("peeked");
        let (tx, rx) = channel();
        let mut watch = PeekWatch::start(tx);
        let wd = Watch::add_raw(watch.raw_fd(), &peeked);
        watch.register(wd, peeked.clone());
        std::fs::remove_dir(&peeked).unwrap();
        let mut changed_first = false;
        loop {
            match rx.recv_timeout(EVENT_WAIT).expect("the kernel dropping a watch answers an event") {
                Event::PeekChanged(seen) if seen == wd => changed_first = true,
                Event::PeekGone(gone) => {
                    assert_eq!(gone, wd);
                    watch.forget(gone);
                    break;
                }
                _ => {}
            }
        }
        assert!(changed_first, "the column is told before its watch is forgotten");
        assert_eq!(watch.held.len(), 0, "a watch the kernel dropped is not held");
    }

    // The pump thread owns the only other sender, so a disconnected channel is the thread having ended.
    #[test]
    fn dropping_the_watch_ends_its_pump_thread() {
        let (tx, rx) = channel();
        drop(PeekWatch::start(tx));
        assert!(matches!(rx.recv_timeout(EVENT_WAIT), Err(std::sync::mpsc::RecvTimeoutError::Disconnected)), "the thread still holds its sender");
    }
}
