// One parsed mount table and one serial remote lane per walk; a refused mount is not retried by its siblings.
use super::{searchread::Directory, searchwalk::WalkProbes};
use std::collections::HashSet;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{atomic::Ordering, Arc, Mutex};

// A directory's remote mount key, None when it is local.
pub(super) type Class = Option<PathBuf>;

pub(super) struct Reader<'a> {
    body: &'a str,
    mounts: Vec<(PathBuf, String)>,
    // Every mount point, sorted by component, so a walk finds a boundary without scanning the table.
    points: Vec<PathBuf>,
    remote: Mutex<HashSet<PathBuf>>,
    // Full classifications, each a gvfs check and a scan of the mount table.
    #[cfg(test)]
    pub(super) classified: std::sync::atomic::AtomicUsize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(body: &'a str) -> Self {
        let mounts = super::mountinfo::mounts_in(body);
        let mut points: Vec<PathBuf> = mounts.iter().map(|(mount, _)| mount.clone()).collect();
        points.sort();
        points.dedup();
        Self {
            body, mounts, points, remote: Mutex::new(HashSet::new()),
            #[cfg(test)]
            classified: Default::default(),
        }
    }

    // A child read from its parent's listing has its parent's class unless a mount or a gvfs share starts at it.
    pub(super) fn class(&self, dir: &Path, parent: Option<&Class>) -> Class {
        match parent {
            Some(parent) if !self.boundary(dir) => parent.clone(),
            _ => self.remote_mount(dir),
        }
    }

    // Only a mount point can change the deepest enclosing mount of a one-name child, and only a text holding "gvfs" can name a share.
    fn boundary(&self, dir: &Path) -> bool {
        self.points.binary_search_by(|point| point.as_path().cmp(dir)).is_ok()
            || dir.as_os_str().as_bytes().windows(4).any(|part| part == b"gvfs")
    }

    fn remote_mount(&self, dir: &Path) -> Option<PathBuf> {
        #[cfg(test)]
        self.classified.fetch_add(1, Ordering::Relaxed);
        if super::extclass::gvfs_class(dir).is_some() { return Some(super::iomount::mount_key(dir, self.body)); }
        super::mountinfo::enclosing(dir, &self.mounts)
            .filter(|(_, kind)| super::netfs::can_wedge(kind)).map(|(mount, _)| mount.clone())
    }

    #[cfg(test)]
    pub(super) fn is_remote(&self, dir: &Path) -> bool {
        self.remote_mount(dir).is_some()
    }

    #[cfg(test)]
    pub(super) fn read(&self, dir: &Path, follow: bool, force_bound: bool, probes: &WalkProbes) -> std::io::Result<Directory> {
        self.read_classed(dir, self.remote_mount(dir), follow, force_bound, probes)
    }

    pub(super) fn read_classed(&self, dir: &Path, mount: Class, follow: bool, force_bound: bool, probes: &WalkProbes) -> std::io::Result<Directory> {
        let cancel = Arc::clone(&probes.cancel);
        #[cfg(test)]
        let hold = Arc::clone(&probes.read_hold);
        #[cfg(test)]
        let held_dir = dir.to_path_buf();
        let progress = move |_count| {
            #[cfg(test)]
            super::searchwalkhold::hold_read_if_due(&hold, &held_dir, _count);
        };
        if mount.is_none() && !force_bound {
            return super::searchread::read_interruptible(dir, follow, || cancel.load(Ordering::Relaxed), progress);
        }
        let mut refused = self.remote.lock().unwrap();
        if cancel.load(Ordering::Relaxed) {
            return super::searchread::read_interruptible(dir, follow, || true, |_| {});
        }
        if mount.as_ref().is_some_and(|mount| refused.contains(mount)) {
            return Err(std::io::Error::other("the mount did not answer this walk"));
        }
        #[cfg(test)]
        probes.remote_reads.fetch_add(1, Ordering::Relaxed);
        let result = super::searchread::bounded_observed(dir, self.body, follow, move || cancel.load(Ordering::Relaxed), progress);
        match result {
            Ok(read) => read,
            Err(error) => {
                if let Some(mount) = mount { refused.insert(mount); }
                Err(std::io::Error::other(error.msg))
            }
        }
    }
}
