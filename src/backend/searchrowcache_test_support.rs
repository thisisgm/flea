// Test-only syscall accounting and isolated fixtures for descriptor lifetime regressions.
use super::*;
use crate::backend::testdir::TestDir;
use std::sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc, Mutex};
use std::time::Duration;

// Positions of each counter in a snapshot.
pub(crate) const OPENS: usize = 0;
pub(crate) const CLOSES_INLINE: usize = 1;
pub(crate) const ADDS: usize = 2;
pub(crate) const REMOVES: usize = 3;
pub(crate) const MISSES: usize = 4;
pub(crate) const FIELDS: usize = 5;
pub(crate) const US_PER_SECOND: f64 = 1e6;
// No watch is ever armed as wd -1, so an event on it is foreign to every cache.
pub(crate) const FOREIGN_WD: i32 = -1;

#[derive(Default)]
pub(crate) struct Counters {
    pub opens: AtomicUsize,
    pub adds: AtomicUsize,
    pub removes: AtomicUsize,
    pub closes_inline: AtomicUsize,
    pub closes_background: AtomicUsize,
    pub misses: AtomicUsize,
    pub closed: Mutex<Option<mpsc::Sender<(std::thread::ThreadId, Duration)>>>,
}

impl Counters {
    pub(crate) fn snapshot(&self) -> [usize; FIELDS] {
        [&self.opens, &self.closes_inline, &self.adds, &self.removes, &self.misses]
            .map(|count| count.load(Ordering::Relaxed))
    }

    pub(crate) fn close(&self, fd: OwnedFd, inline: bool) {
        let start = Instant::now();
        drop(fd);
        let elapsed = start.elapsed();
        let counter = if inline { &self.closes_inline } else { &self.closes_background };
        counter.fetch_add(1, Ordering::Relaxed);
        if let Some(closed) = self.closed.lock().unwrap().take() {
            let _ = closed.send((std::thread::current().id(), elapsed));
        }
    }
}

// Each case owns a mktemp root and the standard guarded TestDir within it.
pub(crate) struct Fixture {
    root: PathBuf,
    dir: Option<TestDir>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let output = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-test-fs3ra.XXXXXXXX"]).output().unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(std::str::from_utf8(&output.stdout).unwrap().trim());
        assert!(root.starts_with("/tmp") && root.file_name().unwrap().as_encoded_bytes().starts_with(b"flea-test-fs3ra."));
        std::fs::write(root.join(".flea-test-sandbox"), "FS3-RA\n").unwrap();
        let dir = TestDir::new_in("fs3ra", &root);
        Self { root, dir: Some(dir) }
    }
}

impl std::ops::Deref for Fixture {
    type Target = TestDir;
    fn deref(&self) -> &TestDir { self.dir.as_ref().unwrap() }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.dir.take());
        assert_eq!(std::fs::read_to_string(self.root.join(".flea-test-sandbox")).unwrap(), "FS3-RA\n");
        std::fs::remove_file(self.root.join(".flea-test-sandbox")).unwrap();
        std::fs::remove_dir(&self.root).unwrap();
    }
}

pub(super) fn listing(names: &[&str]) -> Listing {
    let mut listing = Listing::new();
    for name in names { listing.push(name, false); }
    listing
}

pub(super) fn counts(cache: &Cache) -> [Arc<Counters>; 2] {
    [Arc::clone(&cache.counts), Arc::clone(&cache.inodes.counts)]
}

pub(super) fn snapshot(counts: &[Arc<Counters>; 2]) -> [usize; FIELDS] {
    let a = counts[0].snapshot();
    let b = counts[1].snapshot();
    std::array::from_fn(|i| a[i] + b[i])
}

pub(crate) fn assert_unwatched(fd: &OwnedFd) {
    let info = std::fs::read_to_string(format!("/proc/self/fdinfo/{}", fd.as_raw_fd())).unwrap();
    // Sample input: inotify wd:1 ino:a001 sdev:800013 mask:fce ignored_mask:0 fhandle-bytes:8 fhandle-type:1 f_handle:01a0
    let armed: HashSet<_> = info.lines().filter(|line| line.starts_with("inotify ")).map(|line| {
        let wd = line.split_whitespace().find_map(|field| field.strip_prefix("wd:")).unwrap();
        i32::from_str_radix(wd, 16).unwrap()
    }).collect();
    assert!(armed.is_empty(), "a cleared descriptor must have no kernel watches: {armed:?}");
}

pub(super) fn assert_cleared(cache: &Cache) {
    assert!(cache.fd.is_some(), "these cases have no read error that could retire the descriptor");
    assert!(cache.watches.is_empty() && cache.refs.is_empty());
    assert!(cache.entries.is_empty() && cache.requested.is_empty());
    assert_eq!(cache.bytes, 0);
    assert_unwatched(cache.fd.as_ref().unwrap());
    cache.inodes.assert_cleared();
}
