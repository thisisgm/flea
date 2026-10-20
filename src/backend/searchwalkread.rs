// One parsed mount table and one serial remote lane per walk; a refused mount is not retried by its siblings.
use super::{searchread::Directory, searchwalk::WalkProbes};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{atomic::Ordering, Arc, Mutex};

pub(super) struct Reader<'a> {
    body: &'a str,
    mounts: Vec<(PathBuf, String)>,
    remote: Mutex<HashSet<PathBuf>>,
}

impl<'a> Reader<'a> {
    pub(super) fn new(body: &'a str) -> Self {
        Self { body, mounts: super::mountinfo::mounts_in(body), remote: Mutex::new(HashSet::new()) }
    }

    fn remote_mount(&self, dir: &Path) -> Option<PathBuf> {
        if super::extclass::gvfs_class(dir).is_some() { return Some(super::iomount::mount_key(dir, self.body)); }
        super::mountinfo::enclosing(dir, &self.mounts)
            .filter(|(_, kind)| super::netfs::can_wedge(kind)).map(|(mount, _)| mount.clone())
    }

    pub(super) fn is_remote(&self, dir: &Path) -> bool {
        self.remote_mount(dir).is_some()
    }

    pub(super) fn read(&self, dir: &Path, follow: bool, force_bound: bool, probes: &WalkProbes) -> std::io::Result<Directory> {
        let mount = self.remote_mount(dir);
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
