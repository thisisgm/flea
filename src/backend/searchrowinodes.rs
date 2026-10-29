// File-inode coverage catches writes through any hard link while directory coverage guards path replacement.
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_int, c_void, CString};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use super::watch::{IN_CLOEXEC, IN_DONT_FOLLOW, IN_IGNORED, IN_NONBLOCK};
use super::searchrowcache::{word, DRAIN_READ_LIMIT, LEN_AT, MASK_AT, PATH_OVERHEAD_BYTES, READ_BUFFER_BYTES, WD_AT};

// The inode watch names the link itself, so it is the row mask plus IN_DONT_FOLLOW.
const MASK: u32 = IN_DONT_FOLLOW | super::searchrowcache::MASK;
extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

// A failed spawn drops its closure on the caller. Transfer a raw fd so that only the selected arm closes it.
pub(super) fn close_later(fd: Option<OwnedFd>, #[cfg(test)] counts: &std::sync::Arc<super::searchrowcache::test_support::Counters>) {
    let Some(fd) = fd else { return };
    let raw = fd.into_raw_fd();
    #[cfg(test)]
    let observed = std::sync::Arc::clone(counts);
    let close = move || {
        // SAFETY: this arm owns raw only after spawn succeeded; the failure arm below then never runs.
        let owned = unsafe { OwnedFd::from_raw_fd(raw) };
        #[cfg(test)]
        observed.close(owned, false);
        #[cfg(not(test))]
        drop(owned);
    };
    if std::thread::Builder::new().name("search-row-close".into()).spawn(close).is_err() {
        // SAFETY: spawn failed, so its closure never reconstructed or closed raw.
        let owned = unsafe { OwnedFd::from_raw_fd(raw) };
        #[cfg(test)]
        counts.close(owned, true);
        #[cfg(not(test))]
        drop(owned);
    }
}

// One reference per path, including paths that resolve to the same watched inode.
pub(super) fn remove_watch(fd: c_int, wd: i32, refs: &mut HashMap<i32, usize>, retired: &mut HashSet<i32>,
    #[cfg(test)] counts: &super::searchrowcache::test_support::Counters) {
    let references = refs.get_mut(&wd).expect("every watched path owns a reference");
    *references -= 1;
    if *references != 0 { return; }
    refs.remove(&wd);
    #[cfg(test)]
    counts.removes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if unsafe { inotify_rm_watch(fd, wd) } == 0 { retired.insert(wd); }
}

#[derive(Default)]
pub(crate) struct Inodes {
    fd: Option<OwnedFd>,
    paths: HashMap<PathBuf, i32>,
    refs: HashMap<i32, usize>,
    retired: HashSet<i32>,
    #[cfg(test)]
    pub(crate) counts: std::sync::Arc<super::searchrowcache::test_support::Counters>,
}

impl Inodes {
    pub(crate) fn clear(&mut self) {
        if let Some(fd) = self.fd.as_ref() {
            for (_, wd) in std::mem::take(&mut self.paths) {
                remove_watch(fd.as_raw_fd(), wd, &mut self.refs, &mut self.retired, #[cfg(test)] &self.counts);
            }
        }
    }

    // Sample input: IN_IGNORED on wd 5 after this set removed wd 5, then IN_MODIFY on wd 3.
    // Own removals are bookkeeping; every other event invalidates all inode facts and removes their coverage.
    fn burst(&mut self, bytes: &[u8]) -> bool {
        let mut at = 0;
        let mut clean = true;
        while at + super::inotifyburst::EVENT_HEADER <= bytes.len() {
            let wd = i32::from_ne_bytes(word(bytes, at, WD_AT));
            let mask = u32::from_ne_bytes(word(bytes, at, MASK_AT));
            let len = u32::from_ne_bytes(word(bytes, at, LEN_AT)) as usize;
            if len > bytes.len() - at - super::inotifyburst::EVENT_HEADER { self.clear(); return false; }
            if !(mask == IN_IGNORED && self.retired.remove(&wd)) {
                self.clear();
                clean = false;
            }
            at += super::inotifyburst::EVENT_HEADER + len;
        }
        if at != bytes.len() { self.clear(); return false; }
        clean
    }

    fn unchanged(&mut self) -> bool {
        let Some(fd) = self.fd.as_ref().map(AsRawFd::as_raw_fd) else { return false };
        let mut buffer = [0u8; READ_BUFFER_BYTES];
        let mut clean = true;
        for _ in 0..DRAIN_READ_LIMIT {
            let count = unsafe { read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count < 0 {
                match std::io::Error::last_os_error().kind() {
                    std::io::ErrorKind::WouldBlock => return clean,
                    std::io::ErrorKind::Interrupted => continue,
                    _ => {
                        self.clear();
                        close_later(self.fd.take(), #[cfg(test)] &self.counts);
                        self.retired.clear();
                        return false;
                    }
                }
            }
            if count == 0 { break; }
            clean &= self.burst(&buffer[..count as usize]);
        }
        self.clear();
        false
    }

    // Add before removing, then drain the handoff before the caller reads any row.
    pub(crate) fn cover(&mut self, paths: &HashSet<PathBuf>) -> bool {
        let clean = self.unchanged();
        if self.paths.len() == paths.len() && self.paths.keys().all(|path| paths.contains(path)) { return clean; }
        if paths.len() > super::searchrowcache::ROW_CAP { self.clear(); return false; }
        if paths.is_empty() { self.clear(); return clean; }
        if self.fd.is_none() {
            #[cfg(test)]
            self.counts.opens.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let raw = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
            if raw < 0 { return false; }
            self.fd = Some(unsafe { OwnedFd::from_raw_fd(raw) });
        }
        let fd = self.fd.as_ref().unwrap().as_raw_fd();
        for path in paths {
            if self.paths.contains_key(path) { continue; }
            let Ok(path_c) = CString::new(path.as_os_str().as_encoded_bytes()) else { self.clear(); return false };
            #[cfg(test)]
            self.counts.adds.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let wd = unsafe { inotify_add_watch(fd, path_c.as_ptr(), MASK) };
            if wd < 0 { self.clear(); return false; }
            *self.refs.entry(wd).or_default() += 1;
            self.paths.insert(path.clone(), wd);
        }
        self.paths.retain(|path, &mut wd| {
            if paths.contains(path) { return true; }
            remove_watch(fd, wd, &mut self.refs, &mut self.retired, #[cfg(test)] &self.counts);
            false
        });
        let bridged = self.unchanged();
        clean && bridged
    }

    pub(crate) fn covered(&self, path: &Path) -> bool {
        self.fd.is_some() && self.paths.contains_key(path)
    }

    pub(crate) fn bytes(&self) -> usize {
        self.paths.keys().map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>() + PATH_OVERHEAD_BYTES).sum()
    }

    #[cfg(test)]
    pub(crate) fn watches_inode(&self, path: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        let Some(fd) = self.fd.as_ref() else { return false };
        let inode = format!("ino:{:x}", path.metadata().unwrap().ino());
        // Sample input: inotify wd:1 ino:a001 sdev:800013 mask:fce ignored_mask:0 fhandle-bytes:8 fhandle-type:1 f_handle:01a0
        std::fs::read_to_string(format!("/proc/self/fdinfo/{}", fd.as_raw_fd())).unwrap()
            .lines().any(|line| line.starts_with("inotify ") && line.split_whitespace().any(|field| field == inode))
    }

    #[cfg(test)]
    pub(crate) fn assert_cleared(&self) {
        assert!(self.paths.is_empty() && self.refs.is_empty());
        if let Some(fd) = &self.fd {
            super::searchrowcache::test_support::assert_unwatched(fd);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize { self.paths.len() }
}

impl Drop for Inodes {
    fn drop(&mut self) { close_later(self.fd.take(), #[cfg(test)] &self.counts); }
}

#[cfg(test)]
#[path = "searchrowinodes_tests.rs"]
mod tests;
