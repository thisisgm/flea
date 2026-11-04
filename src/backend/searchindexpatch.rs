// Directory reconciliation visits linked members and moves only the dense tail when a row goes.
use super::*;

impl Index {
    // Sparse swap-removals let tag state follow row ids without walking unrelated entries.
    pub(crate) fn last_patch(&self) -> (&[(usize, usize)], usize) {
        (&self.moves, self.survivors)
    }

    // The tag pass has consumed the remap; a large deletion must not retain its temporary move log.
    pub(crate) fn release_patch(&mut self) {
        self.moves.clear();
        if self.moves.capacity() > 4096 { self.moves.shrink_to(32); }
    }

    fn begin_patch(&mut self) {
        self.moves.clear();
        self.survivors = self.rows.len();
        self.epoch = self.epoch.wrapping_add(1);
    }

    fn remove_row(&mut self, at: usize) {
        let last = self.rows.len() - 1;
        let row = &self.rows[at];
        self.retired_bytes = self.retired_bytes.saturating_add(self.names.get(row.name).len() + std::mem::size_of::<Row>());
        let (parent, previous, next) = (row.parent, row.previous, row.next);
        if previous == EMPTY { self.blocks.set_head(parent, next); }
        else { self.rows[previous as usize].next = next; }
        if next != EMPTY { self.rows[next as usize].previous = previous; }
        let kind = self.is_dir(last);
        self.rows.swap_remove(at);
        if at != last {
            let row = &self.rows[at];
            let (parent, previous, next) = (row.parent, row.previous, row.next);
            if previous == EMPTY { self.blocks.set_head(parent, at as u32); }
            else { self.rows[previous as usize].next = at as u32; }
            if next != EMPTY { self.rows[next as usize].previous = at as u32; }
            if kind { self.kinds[at / 8] |= 1 << (at % 8); }
            else { self.kinds[at / 8] &= !(1 << (at % 8)); }
        }
        self.kinds[last / 8] &= !(1 << (last % 8));
        self.kinds.truncate(self.rows.len().div_ceil(8));
        self.moves.push((at, last));
    }

    fn drop_block(&mut self, parent: u32) {
        let mut pending = vec![parent];
        while let Some(dir) = pending.pop() {
            pending.extend(self.blocks.children(dir));
            while self.blocks.head(dir) != EMPTY {
                #[cfg(test)]
                self.scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.remove_row(self.blocks.head(dir) as usize);
            }
        }
    }

    // Dense rows leave no tombstones; reclaim retained names only beyond both residency thresholds.
    fn reclaim(&mut self) {
        if self.retired_bytes < (1024 * 1024).max(self.bytes(false) / 100) { return; }
        let mut next = Index::new();
        for row in 0..self.rows.len() {
            if !next.push(OsStr::from_bytes(&self.raw(row)), self.is_dir(row)) {
                self.mark_partial(next.partial.unwrap_or("allocation"));
                return;
            }
        }
        next.finish();
        next.partial = self.partial;
        next.epoch = self.epoch.wrapping_add(1);
        next.survivors = self.survivors;
        next.moves = std::mem::take(&mut self.moves);
        #[cfg(test)]
        {
            next.compactions = self.compactions + 1;
            next.scanned.store(self.scanned.load(std::sync::atomic::Ordering::Relaxed) + self.len(), std::sync::atomic::Ordering::Relaxed);
        }
        *self = next;
    }

    // A moved or replaced directory must discard its old descendants even when its name and kind agree.
    pub(crate) fn reset_tree(&mut self, root: &Path, rel: &Path, hidden: bool, body: &str, watch: Option<&super::super::searchwatch::Core>) -> bool {
        self.begin_patch();
        let parent = match self.parent(rel.as_os_str().as_bytes()) {
            Ok(parent) => parent,
            Err(reason) => { self.mark_partial(reason); return false; }
        };
        self.drop_block(parent);
        self.survivors = self.rows.len();
        let path = root.join(rel);
        let probe = path.clone();
        let directory = super::super::iomount::call(&path, body, "search", move || match probe.symlink_metadata() {
            Ok(meta) => Ok(meta.is_dir()),
            Err(error) if matches!(error.raw_os_error(), Some(2 | 20 | 40)) => Ok(false),
            Err(error) => Err(error),
        });
        match directory {
            Ok(Ok(true)) => self.fill_subtrees(root, &[rel.as_os_str().as_bytes().to_vec()], hidden, body, watch),
            Ok(Ok(false)) => {},
            _ => self.mark_partial("unreadable"),
        }
        self.reclaim();
        true
    }

    #[cfg(test)]
    pub(crate) fn patch_dir(&mut self, root: &Path, rel: &Path, hidden: bool, body: &str) -> bool {
        self.patch_watched(root, rel, hidden, body, None)
    }

    pub(crate) fn patch_watched(&mut self, root: &Path, rel: &Path, hidden: bool, body: &str, watch: Option<&super::super::searchwatch::Core>) -> bool {
        let dir = root.join(rel);
        if let Some(watch) = watch {
            if !watch.arm_local(rel, body) { self.mark_partial("watch_failed"); }
        }
        let Ok(listing) = read_bounded(&dir, body, rel.as_os_str().is_empty()) else {
            self.mark_partial("unreadable");
            return false;
        };
        if let Some(reason) = listing.as_ref().and_then(|listing| listing.partial) {
            self.mark_partial(reason);
            return false;
        }
        self.begin_patch();
        let parent = match self.parent(rel.as_os_str().as_bytes()) {
            Ok(parent) => parent,
            Err(reason) => { self.mark_partial(reason); return false; }
        };
        let Some(listing) = listing else {
            self.drop_block(parent);
            if let Some((container, name)) = self.blocks.identity(parent) {
                let mut row = self.blocks.head(container);
                while row != EMPTY {
                    #[cfg(test)]
                    self.scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let entry = &self.rows[row as usize];
                    if entry.name == name {
                        self.remove_row(row as usize);
                        break;
                    }
                    row = entry.next;
                }
            }
            self.survivors = self.rows.len();
            self.reclaim();
            return true;
        };
        let mut wanted = HashMap::new();
        for (name, kind) in listing.entries.into_iter().filter(|(name, _)| hidden || !is_hidden(name)) {
            match self.names.intern(name.as_bytes()) {
                Ok(name) => { wanted.insert(name, kind); }
                Err(reason) => { self.mark_partial(reason); return false; }
            }
        }
        let mut row = self.blocks.head(parent);
        let mut removed_trees = Vec::new();
        while row != EMPTY {
            let entry = &self.rows[row as usize];
            if self.is_dir(row as usize) && wanted.get(&entry.name) != Some(&true) {
                removed_trees.push(entry.name);
            }
            row = entry.next;
        }
        for name in removed_trees {
            if let Ok(dir) = self.blocks.directory(&self.names, parent, name) { self.drop_block(dir); }
        }
        let mut row = self.blocks.head(parent);
        while row != EMPTY {
            #[cfg(test)]
            self.scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let entry = &self.rows[row as usize];
            let (name, next) = (entry.name, entry.next);
            if wanted.get(&name) == Some(&self.is_dir(row as usize)) {
                wanted.remove(&name);
                row = next;
            } else {
                let last = self.rows.len() - 1;
                self.remove_row(row as usize);
                if next as usize != last { row = next; }
            }
        }
        self.survivors = self.rows.len();
        let mut added_dirs = Vec::new();
        for (name, is_dir) in wanted {
            let bytes = self.names.get(name).to_vec();
            if let Err(reason) = self.push_name(parent, &bytes, is_dir) { self.mark_partial(reason); break; }
            if is_dir { added_dirs.push(rel.join(OsStr::from_bytes(&bytes)).as_os_str().as_bytes().to_vec()); }
        }
        self.fill_subtrees(root, &added_dirs, hidden, body, watch);
        self.reclaim();
        true
    }

    fn fill_subtrees(&mut self, root: &Path, added: &[Vec<u8>], hidden: bool, body: &str, watch: Option<&super::super::searchwatch::Core>) {
        let mut pending = added.to_vec();
        while let Some(raw) = pending.pop() {
            if self.storage_refused() { return; }
            let rel = Path::new(OsStr::from_bytes(&raw));
            if let Some(watch) = watch {
                if !watch.arm_local(rel, body) { self.mark_partial("watch_failed"); }
            }
            let entries = match read_bounded(&root.join(rel), body, rel.as_os_str().is_empty()) {
                Ok(Some(entries)) => entries,
                // The walk's rule: a local directory gone since its parent was read is not unreadable, and the parent's watch reports it.
                Ok(None) if !rel.as_os_str().is_empty() && !super::super::iomount::is_remote(&root.join(rel), body) => continue,
                _ => {
                    self.mark_partial("unreadable");
                    continue;
                }
            };
            if let Some(reason) = entries.partial { self.mark_partial(reason); }
            let entries: Vec<_> = entries.entries.into_iter().filter(|(name, _)| hidden || !is_hidden(name)).collect();
            if !self.push_directory(rel.as_os_str(), &entries) { return; }
            for (name, is_dir) in entries {
                if is_dir { pending.push(rel.join(name).as_os_str().as_bytes().to_vec()); }
            }
        }
    }
}

#[cfg(test)]
#[path = "searchindexpatch_tests.rs"]
mod tests;
