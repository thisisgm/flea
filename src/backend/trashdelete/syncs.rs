// A restore batch holds each claim's folder syncs, and the folders its items went back to, to one sync per folder at its close.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

thread_local! {
    // The folders the open batch syncs at its close; None while no batch is open.
    static FOLDERS: RefCell<Option<BTreeSet<PathBuf>>> = const { RefCell::new(None) };
    #[cfg(test)]
    static SYNCED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

pub struct FolderSyncs;

impl FolderSyncs {
    pub fn open() -> Self {
        FOLDERS.with(|open| {
            open.borrow_mut().get_or_insert_with(BTreeSet::new);
        });
        Self
    }

    // The folder a restored item went back to: synced at the batch's close, or now with no batch open.
    pub fn restored(dir: &Path) {
        if !held(dir) {
            synced(dir);
        }
    }
}

impl Drop for FolderSyncs {
    // The items are in place already, so a folder that will not sync is logged, never a failure.
    fn drop(&mut self) {
        for dir in FOLDERS
            .with(|open| open.borrow_mut().take())
            .unwrap_or_default()
        {
            synced(&dir);
        }
    }
}

fn synced(dir: &Path) {
    #[cfg(test)]
    SYNCED.with(|seen| seen.borrow_mut().push(dir.to_path_buf()));
    if let Err(e) = File::open(dir).and_then(|folder| folder.sync_all()) {
        crate::backend::trashmanifest::log(&format!("Could not sync {}: {}", dir.display(), e));
    }
}

// True when an open batch took the folder for its close.
pub(super) fn held(dir: &Path) -> bool {
    FOLDERS.with(|open| {
        open.borrow_mut()
            .as_mut()
            .map(|folders| folders.insert(dir.to_path_buf()))
            .is_some()
    })
}

// The same for a folder held open, named by the path the kernel keeps for its descriptor.
pub(super) fn held_open(dir: &File) -> bool {
    FOLDERS.with(|open| open.borrow().is_some())
        && std::fs::read_link(format!("/proc/self/fd/{}", dir.as_raw_fd()))
            .is_ok_and(|path| held(&path))
}

#[cfg(test)]
impl FolderSyncs {
    // Test seam: the folders synced through a batch or a lone restore since the last read.
    pub(crate) fn take_synced() -> Vec<PathBuf> {
        SYNCED.with(|seen| std::mem::take(&mut *seen.borrow_mut()))
    }
}
