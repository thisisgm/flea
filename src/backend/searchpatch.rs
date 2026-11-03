// A session's own-write and watch patches; one that lands while the walk runs waits for it, because the walk would push the same rows again.
use crate::backend::searchsession::Session;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl Session {
    // Patches the index for a backend write under the scope; false when outside it or its mount does not answer.
    #[cfg(test)]
    pub(crate) fn patch_after_write(&mut self, path: &Path) -> bool {
        self.patch_after_writes(&[path.to_path_buf()])
    }

    // A file two levels down patches its own directory, once however many items landed in it, so a big paste costs one scan per folder.
    pub(crate) fn patch_after_writes(&mut self, paths: &[PathBuf]) -> bool {
        let dirs: BTreeSet<&Path> = paths.iter().filter_map(|path| path.strip_prefix(&self.scope).ok())
            .map(|rel| rel.parent().unwrap_or(Path::new(""))).collect();
        dirs.into_iter().fold(false, |taken, dir| self.patch(dir) | taken)
    }

    // One bounded directory patch invalidates held row ids while preserving unrelated blocks.
    pub(crate) fn patch(&mut self, rel: &Path) -> bool {
        if self.unmounted { return false; }
        // A change under the scope can move every folder size above it, whether or not the index takes the patch.
        self.sizes.forget();
        if self.walking() {
            self.pending_patches.insert(rel.to_path_buf());
            return false;
        }
        self.patch_now(rel)
    }

    // A patch taken after a cancel locks the index, and a walker takes that lock to push, so the walker sees the cancel first and pushes nothing.
    fn patch_now(&mut self, rel: &Path) -> bool {
        if self.unmounted { return false; }
        let mut index = self.index.lock().unwrap();
        let watch = self.watch.as_ref().map(|watch| watch.core.as_ref());
        if !index.patch_watched(&self.scope, rel, self.hidden, &self.body, watch) {
            return false;
        }
        // A finished tag pass follows its rows to their new ids and queues the added ones; an unfinished one starts over.
        let added = self.tags.carry(&index);
        index.release_patch();
        drop(index);
        self.last_query.clear();
        self.last_matches.clear();
        self.pending_rest = None;
        self.tags.read_added(added, &self.index);
        true
    }

    // The patches that waited for the walk, applied once its index is whole or its cancel is seen by every walker; true when the index took one.
    pub(crate) fn flush_patches(&mut self) -> bool {
        let mut taken = false;
        for rel in std::mem::take(&mut self.pending_patches) {
            taken |= self.patch_now(&rel);
        }
        taken
    }
}

#[cfg(test)]
#[path = "searchpatch_tests.rs"]
mod tests;
