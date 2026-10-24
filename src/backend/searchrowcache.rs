// Requested search rows reuse facts only while every local path component remains watched.
use super::listing::Listing;
use super::meta::Meta;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::{c_char, c_int, c_void, CString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(crate) const ROW_CAP: usize = 350;
const WATCH_CAP: usize = 350;
const BYTE_CAP: usize = 2 * 1024 * 1024;
const MASK: u32 = 0x0000_2fce;
const SELF_OR_LOST: u32 = 0x0000_ec00;
extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

struct Entry {
    meta: Meta,
    directory: bool,
    rendered: Option<(String, String)>,
}

#[derive(Default)]
pub(crate) struct Cache {
    fd: Option<OwnedFd>,
    inodes: super::searchrowinodes::Inodes,
    watches: BTreeMap<PathBuf, i32>,
    entries: HashMap<PathBuf, Entry>,
    requested: HashSet<PathBuf>,
    bytes: usize,
    #[cfg(test)]
    pub(crate) misses: usize,
    #[cfg(test)]
    pub(crate) renders: usize,
}

impl Cache {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    // Overflow, self events and ancestor renames all fail closed, without opening any indexed file.
    fn drain(&mut self) {
        let Some(fd) = self.fd.as_ref().map(AsRawFd::as_raw_fd) else { return };
        let mut buffer = [0u8; 8192];
        for _ in 0..16 {
            let count = unsafe { read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count < 0 {
                match std::io::Error::last_os_error().kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    std::io::ErrorKind::WouldBlock => return,
                    _ => break,
                }
            }
            if count == 0 { break; }
            if !self.burst(&buffer[..count as usize]) { return; }
        }
        self.clear();
    }

    // The decoder is also driven with overflow and descriptor-loss events by deterministic tests.
    fn burst(&mut self, bytes: &[u8]) -> bool {
        let mut at = 0;
        while at + 16 <= bytes.len() {
            let wd = i32::from_ne_bytes(bytes[at..at + 4].try_into().unwrap());
            let mask = u32::from_ne_bytes(bytes[at + 4..at + 8].try_into().unwrap());
            let len = u32::from_ne_bytes(bytes[at + 12..at + 16].try_into().unwrap()) as usize;
            if at + 16 + len > bytes.len() || mask & SELF_OR_LOST != 0 {
                self.clear();
                return false;
            }
            use std::os::unix::ffi::OsStrExt;
            let name = &bytes[at + 16..at + 16 + len];
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            for (parent, &watch) in &self.watches {
                if watch == wd {
                    let changed = parent.join(std::ffi::OsStr::from_bytes(&name[..end]));
                    self.entries.retain(|path, entry| !path.starts_with(&changed) && !(entry.directory && path == parent));
                }
            }
            at += 16 + len;
        }
        if at != bytes.len() { self.clear(); return false; }
        self.recount();
        true
    }

    fn recount(&mut self) {
        let dependencies = self.requested.iter().chain(self.watches.keys())
            .map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>() + 64).sum::<usize>();
        self.bytes = dependencies + self.inodes.bytes() + self.entries.iter().map(|(path, entry)| {
            path.as_os_str().len() + entry.meta.target.len() + std::mem::size_of::<Entry>()
                + entry.rendered.as_ref().map_or(0, |(row, kind)| row.len() + kind.len())
        }).sum::<usize>();
    }

    // Ancestor and inode coverage are bounded independently to 350 paths each and armed before any miss is read.
    fn begin(&mut self, base: &Path, listing: &Listing, start: usize, count: usize, body: &str) {
        self.drain();
        let mounts = super::mountinfo::wedge_points_in(body);
        self.requested = (start..start.saturating_add(count).min(listing.len())).take(ROW_CAP)
            .map(|row| base.join(listing.name(row)))
            .filter(|path| path.is_absolute() && !path.to_string_lossy().contains('\u{fffd}')
                && super::extclass::gvfs_class(path).is_none()
                && !path.ancestors().find_map(|parent| mounts.get(parent)).copied().unwrap_or(false)).collect();
        let mut needed = BTreeSet::new();
        let directories: Vec<_> = (start..start.saturating_add(count.min(ROW_CAP)).min(listing.len()))
            .filter(|&row| listing.is_dir(row)).map(|row| base.join(listing.name(row)))
            .filter(|path| self.requested.contains(path)).collect();
        let mut parents: HashSet<_> = self.requested.iter().filter_map(|path| path.parent()).collect();
        parents.extend(directories.iter().map(PathBuf::as_path));
        for path in parents {
            for parent in path.ancestors() {
                needed.insert(parent.to_path_buf());
                if needed.len() > WATCH_CAP { self.clear(); return; }
            }
        }
        let path_bytes: usize = needed.iter().chain(self.requested.iter()).chain(self.requested.iter())
            .map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>() + 64).sum();
        if path_bytes > BYTE_CAP { self.clear(); return; }
        if self.fd.is_none() || self.watches.keys().ne(needed.iter()) {
            self.entries.clear();
            self.bytes = 0;
            self.watches.clear();
            let fd = unsafe { inotify_init1(0x0008_0800) };
            self.fd = (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) });
            if let Some(fd) = &self.fd {
                for path in needed {
                    let Ok(path_c) = CString::new(path.as_os_str().as_encoded_bytes()) else { continue };
                    let wd = unsafe { inotify_add_watch(fd.as_raw_fd(), path_c.as_ptr(), MASK) };
                    if wd < 0 { self.clear(); return; }
                    self.watches.insert(path, wd);
                }
            }
        }
        if !self.inodes.cover(&self.requested) { self.entries.clear(); }
        self.entries.retain(|path, _| self.requested.contains(path));
        self.recount();
    }

    pub(crate) fn window(&mut self, base: &Path, listing: &Listing, start: usize, count: usize, body: &str) -> (Vec<Meta>, f64) {
        let begun = Instant::now();
        let start = start.min(listing.len());
        if count == 0 || start == listing.len() { self.clear(); return (Vec::new(), 0.0); }
        self.begin(base, listing, start, count, body);
        let end = start.saturating_add(count).min(listing.len());
        let mut metas = Vec::with_capacity(end - start);
        let mut devices = HashMap::new();
        for row in start..end {
            let path = base.join(listing.name(row));
            let directory = listing.is_dir(row);
            if let Some(entry) = self.entries.get(&path).filter(|entry| entry.directory == directory) {
                metas.push(entry.meta.clone());
                continue;
            }
            #[cfg(test)]
            { self.misses += 1; }
            let (mut fresh, _) = super::meta::stat_range(base, listing, row, 1);
            let meta = fresh.remove(0);
            let device_remote = *devices.entry(meta.dev).or_insert_with(|| super::mountinfo::fstype_for_dev(meta.dev, body)
                .is_some_and(|kind| super::netfs::can_wedge(&kind)));
            let bytes = path.as_os_str().len() + meta.target.len() + std::mem::size_of::<Entry>();
            // Symlink targets and vanished rows have dependencies outside the directory watch, so each is a miss.
            if self.fd.is_some() && self.inodes.covered(&path) && self.requested.contains(&path) && meta.mode != 0 && meta.mode & 0o170000 != 0o120000
                && !device_remote && self.bytes + bytes <= BYTE_CAP {
                self.bytes += bytes;
                self.entries.insert(path, Entry { meta: meta.clone(), directory, rendered: None });
            }
            metas.push(meta);
        }
        (metas, begun.elapsed().as_secs_f64() * 1000.0)
    }

    pub(crate) fn rendered(&self, path: &Path) -> Option<&(String, String)> {
        self.entries.get(path)?.rendered.as_ref()
    }

    pub(crate) fn remember(&mut self, path: &Path, row: &str, kind: &str) {
        #[cfg(test)]
        { self.renders += 1; }
        if self.bytes + row.len() + kind.len() > BYTE_CAP { return; }
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
