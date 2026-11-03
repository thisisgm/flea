//! Inotify does not report mapped writes, so reuse also checks the requested inode's facts.
use std::{os::unix::fs::MetadataExt, path::Path};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
}

pub(super) fn read(path: &Path) -> Option<Stamp> {
    #[cfg(test)]
    {
        super::super::meta::SEARCH_STAT_CALLS.with(|calls| calls.set(calls.get() + 1));
        super::Cache::test_record_stat_path(path);
    }
    let meta = path.symlink_metadata().ok()?;
    Some(Stamp {
        device: meta.dev(),
        inode: meta.ino(),
        size: meta.size(),
        mode: meta.mode(),
        modified: (meta.mtime(), meta.mtime_nsec()),
        changed: (meta.ctime(), meta.ctime_nsec()),
    })
}
