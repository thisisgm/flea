// Requested search rows reuse facts only while every local path component remains watched.
use super::inotifyburst::EVENT_HEADER;
use super::listing::Listing;
use super::meta::{Meta, S_IFLNK, S_IFMT};
use super::watch::{
    IN_ATTRIB, IN_CLOEXEC, IN_CLOSE_WRITE, IN_CREATE, IN_DELETE, IN_DELETE_SELF, IN_IGNORED, IN_MODIFY,
    IN_DONT_FOLLOW, IN_MOVED_FROM, IN_MOVED_TO, IN_MOVE_SELF, IN_NONBLOCK, IN_ONLYDIR, IN_Q_OVERFLOW, IN_UNMOUNT,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::{c_char, c_int, c_void, CString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[path = "searchrowstamp.rs"]
mod stamp;

pub(crate) const ROW_CAP: usize = 350;
const WATCH_CAP: usize = 350;
// A window moving more rows than this reads cold, since arming costs a watch call per path; a steady window arms and reuses its rows.
const ARM_LIMIT: usize = 32;
const BYTE_CAP: usize = 2 * 1024 * 1024;
pub(crate) const READ_BUFFER_BYTES: usize = 8192;
pub(crate) const DRAIN_READ_LIMIT: usize = 16;
pub(crate) const PATH_OVERHEAD_BYTES: usize = 64;
// The events that drop a cached row: any write, create, delete or move under the watch, and an unmount.
pub(crate) const MASK: u32 = IN_MODIFY | IN_ATTRIB | IN_CLOSE_WRITE | IN_MOVED_FROM | IN_MOVED_TO | IN_CREATE | IN_DELETE | IN_DELETE_SELF | IN_MOVE_SELF | IN_UNMOUNT;
// The watch lost its own inode or its queue, so the whole cache clears.
const SELF_OR_LOST: u32 = IN_IGNORED | IN_Q_OVERFLOW | IN_UNMOUNT | IN_MOVE_SELF | IN_DELETE_SELF;
// inotify_add_watch's answer for a symlink it was told neither to follow nor to accept as a directory.
const ENOTDIR: i32 = 20;
// Byte offsets of the wd, mask and len words inside one inotify_event header.
pub(super) const WD_AT: usize = 0;
pub(super) const MASK_AT: usize = 4;
pub(super) const LEN_AT: usize = 12;
extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

// The device and inode a path resolves to through every symlink on the way.
fn resolved(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|meta| (meta.dev(), meta.ino()))
}

// One native-endian header word, read at its offset inside the event that starts at at.
pub(super) fn word(bytes: &[u8], at: usize, field: usize) -> [u8; 4] {
    bytes[at + field..at + field + 4].try_into().unwrap()
}

struct Entry {
    meta: Meta,
    stamp: stamp::Stamp,
    directory: bool,
    rendered: Option<(String, String)>,
}

impl Entry {
    fn bytes(&self, path: &Path) -> usize {
        path.as_os_str().len() + self.meta.target.len() + std::mem::size_of::<Self>()
            + self.rendered.as_ref().map_or(0, |(row, kind)| row.len() + kind.len())
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    fd: Option<OwnedFd>,
    inodes: super::searchrowinodes::Inodes,
    watches: BTreeMap<PathBuf, i32>,
    // Watched paths that are symlinks, with the device and inode each resolved to when its watch was armed.
    linked: HashMap<PathBuf, (u64, u64)>,
    refs: HashMap<i32, usize>,
    retired: HashSet<i32>,
    entries: HashMap<PathBuf, Entry>,
    requested: HashSet<PathBuf>,
    // A hash of each name the last window asked for, armed or cold.
    last: HashSet<u64>,
    // A cold response reads current facts without changing the armed entries underneath it.
    reuse_rendered: bool,
    kind_revision: u64,
    bytes: usize,
    #[cfg(test)]
    pub(crate) misses: usize,
    #[cfg(test)]
    pub(crate) renders: usize,
    #[cfg(test)]
    pub(crate) counts: std::sync::Arc<test_support::Counters>,
}

impl Cache {
    pub(crate) fn clear(&mut self) {
        self.reuse_rendered = false;
        if let Some(fd) = self.fd.as_ref() {
            for (_, wd) in std::mem::take(&mut self.watches) {
                super::searchrowinodes::remove_watch(fd.as_raw_fd(), wd, &mut self.refs, &mut self.retired, #[cfg(test)] &self.counts);
            }
        }
        self.linked.clear();
        self.inodes.clear();
        self.entries.clear();
        self.requested.clear();
        self.bytes = 0;
    }

    // Overflow, self events and ancestor renames all fail closed, without opening any indexed file.
    fn drain(&mut self) {
        let Some(fd) = self.fd.as_ref().map(AsRawFd::as_raw_fd) else { return };
        let mut buffer = [0u8; READ_BUFFER_BYTES];
        for _ in 0..DRAIN_READ_LIMIT {
            let count = unsafe { read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count < 0 {
                match std::io::Error::last_os_error().kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    std::io::ErrorKind::WouldBlock => return,
                    _ => {
                        self.clear();
                        super::searchrowinodes::close_later(self.fd.take(), #[cfg(test)] &self.counts);
                        self.retired.clear();
                        return;
                    }
                }
            }
            if count == 0 { break; }
            self.burst(&buffer[..count as usize]);
        }
        self.clear();
    }

    // Sample input: wd 1, mask 0x00000100, cookie 0, len 16, then "NEWFILE.txt\0\0\0\0\0".
    // Constraint: a header or name past the buffer clears the cache and answers false.
    fn burst(&mut self, bytes: &[u8]) -> bool {
        let mut at = 0;
        let mut clean = true;
        while at + EVENT_HEADER <= bytes.len() {
            let wd = i32::from_ne_bytes(word(bytes, at, WD_AT));
            let mask = u32::from_ne_bytes(word(bytes, at, MASK_AT));
            let len = u32::from_ne_bytes(word(bytes, at, LEN_AT)) as usize;
            if len > bytes.len() - at - EVENT_HEADER { self.clear(); return false; }
            at += EVENT_HEADER + len;
            if mask == IN_IGNORED && self.retired.remove(&wd) { continue; }
            if mask & SELF_OR_LOST != 0 {
                self.clear();
                clean = false;
                continue;
            }
            use std::os::unix::ffi::OsStrExt;
            let name = &bytes[at - len..at];
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            for (parent, &watch) in &self.watches {
                if watch == wd {
                    let changed = parent.join(std::ffi::OsStr::from_bytes(&name[..end]));
                    self.entries.retain(|path, entry| !path.starts_with(&changed) && !(entry.directory && path == parent));
                }
            }
        }
        if at != bytes.len() { self.clear(); return false; }
        self.recount();
        clean
    }

    fn recount(&mut self) {
        let dependencies = self.requested.iter().chain(self.watches.keys())
            .map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>() + PATH_OVERHEAD_BYTES).sum::<usize>();
        self.bytes = dependencies + self.inodes.bytes() + self.entries.iter()
            .map(|(path, entry)| entry.bytes(path)).sum::<usize>();
    }

    // Ancestor and inode coverage are bounded independently to 350 paths each and armed before any miss is read.
    fn begin(&mut self, base: &Path, listing: &Listing, start: usize, count: usize, body: &str) {
        self.drain();
        // A symlink can retarget from an unwatched directory; checked before the rows are chosen, since clear() empties them.
        if self
            .linked
            .iter()
            .any(|(path, &armed)| resolved(path) != Some(armed))
        {
            self.clear();
        }
        let mounts = super::mountinfo::wedge_points_in(body);
        self.requested = (start..start.saturating_add(count).min(listing.len()))
            .take(ROW_CAP)
            .map(|row| listing.path(base, row))
            .filter(|path| {
                path.is_absolute()
                    && super::extclass::gvfs_class(path).is_none()
                    && !path
                        .ancestors()
                        .find_map(|parent| mounts.get(parent))
                        .copied()
                        .unwrap_or(false)
            })
            .collect();
        let mut needed = BTreeSet::new();
        let directories: Vec<_> = (start
            ..start.saturating_add(count.min(ROW_CAP)).min(listing.len()))
            .filter(|&row| listing.is_dir(row))
            .map(|row| listing.path(base, row))
            .filter(|path| self.requested.contains(path))
            .collect();
        let mut parents: HashSet<_> = self
            .requested
            .iter()
            .filter_map(|path| path.parent())
            .collect();
        parents.extend(directories.iter().map(PathBuf::as_path));
        for path in parents {
            for parent in path.ancestors() {
                needed.insert(parent.to_path_buf());
                if needed.len() > WATCH_CAP { self.clear(); return; }
            }
        }
        let path_bytes: usize = needed.iter().chain(self.requested.iter()).chain(self.requested.iter())
            .map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>() + PATH_OVERHEAD_BYTES).sum();
        if path_bytes > BYTE_CAP { self.clear(); return; }
        if needed.is_empty() { self.clear(); return; }
        if self.fd.is_none() {
            #[cfg(test)]
            self.counts.opens.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let raw = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
            if raw < 0 { self.clear(); return; }
            self.fd = Some(unsafe { OwnedFd::from_raw_fd(raw) });
        }
        if self.watches.keys().ne(needed.iter()) {
            let fd = self.fd.as_ref().unwrap().as_raw_fd();
            for path in &needed {
                if self.watches.contains_key(path) { continue; }
                let Ok(path_c) = CString::new(path.as_os_str().as_encoded_bytes()) else { self.clear(); return };
                #[cfg(test)]
                self.counts.adds.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let mut wd = unsafe { inotify_add_watch(fd, path_c.as_ptr(), MASK | IN_DONT_FOLLOW | IN_ONLYDIR) };
                if wd < 0 && std::io::Error::last_os_error().raw_os_error() == Some(ENOTDIR) {
                    // Resolved before the watch is armed, so a retarget between the two reads as a change.
                    let Some(armed) = resolved(path) else { self.clear(); return };
                    wd = unsafe { inotify_add_watch(fd, path_c.as_ptr(), MASK) };
                    self.linked.insert(path.clone(), armed);
                }
                if wd < 0 { self.clear(); return; }
                *self.refs.entry(wd).or_default() += 1;
                self.watches.insert(path.clone(), wd);
            }
            self.watches.retain(|path, &mut wd| {
                if needed.contains(path) { return true; }
                self.linked.remove(path);
                super::searchrowinodes::remove_watch(fd, wd, &mut self.refs, &mut self.retired, #[cfg(test)] &self.counts);
                false
            });
            // Catch changes during the handoff before reusing facts or reading a miss.
            self.drain();
        }
        if !self.inodes.cover(&self.requested) { self.entries.clear(); }
        self.entries.retain(|path, _| self.requested.contains(path));
        self.recount();
    }

    // Cold past ARM_LIMIT moved rows, or when a small window would unarm more; a cold read leaves the armed state to the next warm window.
    fn cold(&mut self, listing: &Listing, start: usize, end: usize) -> bool {
        use std::hash::{BuildHasher, BuildHasherDefault};
        let hasher = BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default();
        let rows: HashSet<u64> = (start..end.min(start + ROW_CAP)).map(|row| hasher.hash_one(listing.name(row))).collect();
        let moved = rows.difference(&self.last).count() + self.last.difference(&rows).count();
        let small = rows.len() <= ARM_LIMIT && self.requested.len() > rows.len() + ARM_LIMIT;
        self.last = rows;
        moved > ARM_LIMIT || small
    }

    pub(crate) fn window(&mut self, base: &Path, listing: &Listing, start: usize, count: usize, body: &str) -> (Vec<Meta>, f64) {
        let begun = Instant::now();
        self.reuse_rendered = false;
        let start = start.min(listing.len());
        if count == 0 || start == listing.len() { self.clear(); return (Vec::new(), 0.0); }
        let end = start.saturating_add(count).min(listing.len());
        if self.cold(listing, start, end) {
            #[cfg(test)]
            { self.misses += end - start; self.counts.misses.fetch_add(end - start, std::sync::atomic::Ordering::Relaxed); }
            return (super::meta::stat_range(base, listing, start, end - start).0, begun.elapsed().as_secs_f64() * 1000.0);
        }
        self.begin(base, listing, start, count, body);
        self.reuse_rendered = true;
        let mut metas = Vec::with_capacity(end - start);
        let mut devices = HashMap::new();
        for row in start..end {
            let path = listing.path(base, row);
            let directory = listing.is_dir(row);
            let stamp = self.requested.contains(&path).then(|| stamp::read(&path)).flatten();
            if let Some(entry) = self.entries.get(&path).filter(|entry| entry.directory == directory && Some(entry.stamp) == stamp) {
                metas.push(entry.meta.clone());
                continue;
            }
            if let Some(old) = self.entries.remove(&path) {
                self.bytes = self.bytes.saturating_sub(old.bytes(&path));
            }
            #[cfg(test)]
            { self.misses += 1; self.counts.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
            let (mut fresh, _) = super::meta::stat_range(base, listing, row, 1);
            let meta = fresh.remove(0);
            let device_remote = *devices.entry(meta.dev).or_insert_with(|| super::mountinfo::fstype_for_dev(meta.dev, body)
                .is_some_and(|kind| super::netfs::can_wedge(&kind)));
            let bytes = path.as_os_str().len() + meta.target.len() + std::mem::size_of::<Entry>();
            // Symlink targets and vanished rows have dependencies outside the directory watch, so each is a miss.
            if let Some(stamp) = stamp.filter(|_| self.fd.is_some() && self.inodes.covered(&path) && self.requested.contains(&path) && meta.mode != 0 && meta.mode & S_IFMT != S_IFLNK
                && !device_remote && self.bytes + bytes <= BYTE_CAP) {
                self.bytes += bytes;
                self.entries.insert(path, Entry { meta: meta.clone(), stamp, directory, rendered: None });
            }
            metas.push(meta);
        }
        self.recount();
        (metas, begun.elapsed().as_secs_f64() * 1000.0)
    }

    pub(crate) fn rendered(&self, path: &Path) -> Option<&(String, String)> {
        if !self.reuse_rendered { return None; }
        self.entries.get(path)?.rendered.as_ref()
    }

    pub(crate) fn kind_revision(&mut self, revision: u64) {
        if self.kind_revision == revision { return; }
        self.kind_revision = revision;
        for entry in self.entries.values_mut() { entry.rendered = None; }
        self.recount();
    }

    pub(crate) fn remember(&mut self, path: &Path, row: &str, kind: &str) {
        #[cfg(test)]
        { self.renders += 1; }
        if !self.reuse_rendered || self.bytes + row.len() + kind.len() > BYTE_CAP { return; }
        if let Some(entry) = self.entries.get_mut(path) {
            if entry.rendered.is_none() {
                entry.rendered = Some((row.to_string(), kind.to_string()));
                self.bytes += row.len() + kind.len();
            }
        }
    }
}

#[cfg(test)]
#[path = "searchrowcache_tests.rs"]
mod tests;

impl Drop for Cache {
    fn drop(&mut self) {
        super::searchrowinodes::close_later(self.fd.take(), #[cfg(test)] &self.counts);
    }
}

#[cfg(test)]
#[path = "searchrowcache_test_support.rs"]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "searchrowcache_lifetime_tests.rs"]
mod lifetime_tests;
#[cfg(test)]
#[path = "searchrowcache_freshness_tests.rs"]
mod freshness_tests;
#[cfg(test)]
#[path = "searchrowcache_probe_tests.rs"]
mod probe_tests;

#[cfg(test)]
#[path = "searchrowcache_events_tests.rs"]
mod events_tests;

#[cfg(test)]
#[path = "searchrowcache_gap64_tests.rs"]
mod gap64_tests;
