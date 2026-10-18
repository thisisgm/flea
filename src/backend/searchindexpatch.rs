// Existing directory reconciliation, using explicit raw-path reconstruction.
use super::*;

impl Index {
    // Rebuilds only surviving identities; publication is atomic if storage is refused.
    fn compact(&mut self, kept: &[usize]) -> bool {
        let mut next = Index::new();
        for &row in kept {
            if !next.push(OsStr::from_bytes(&self.raw(row)), self.is_dir(row)) {
                self.mark_partial(next.partial.unwrap_or("allocation"));
                return false;
            }
        }
        next.finish();
        next.partial = self.partial;
        #[cfg(test)]
        {
            next.compactions = self.compactions + 1;
            next.scanned.store(self.scanned.load(std::sync::atomic::Ordering::Relaxed), std::sync::atomic::Ordering::Relaxed);
        }
        *self = next;
        true
    }

    // The last patch's row moves: the old ids kept in order (None: all kept) and the count of surviving old rows.
    pub(crate) fn last_patch(&self) -> (Option<&[usize]>, usize) {
        (self.kept.as_deref(), self.survivors)
    }

    // Reconciles one scope-relative directory in one pass and one repack; false when its mount does not answer.
    pub(crate) fn patch_dir(&mut self, root: &Path, rel: &Path, hidden: bool, body: &str) -> bool {
        let rel = rel.as_os_str();
        let dir = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
        let Ok(listing) = read_bounded(&dir, body) else {
            self.mark_partial("unreadable");
            return false;
        };
        (self.kept, self.survivors) = (None, self.rows.len());
        let prefix: Vec<u8> = if rel.is_empty() { Vec::new() } else { [rel.as_bytes(), b"/"].concat() };
        // A directory that is gone drops itself and its subtree; the scope itself drops every row.
        let Some(listing) = listing else {
            let kept: Vec<usize> = (0..self.rows.len())
                .filter(|&row| !rel.is_empty() && self.raw(row).as_slice() != rel.as_bytes() && !self.raw(row).starts_with(&prefix))
                .collect();
            if !self.compact(&kept) { return false; }
            (self.survivors, self.kept) = (kept.len(), Some(kept));
            return true;
        };
        if let Some(reason) = listing.partial {
            self.mark_partial(reason);
            return false;
        }
        let seen: Vec<(Vec<u8>, bool)> = listing.entries
            .into_iter()
            .filter(|(name, _)| hidden || !is_hidden(name))
            .map(|(name, is_dir)| ([prefix.as_slice(), name.as_bytes()].concat(), is_dir))
            .collect();
        let kinds: HashMap<&[u8], bool> = seen.iter().map(|(child, is_dir)| (child.as_slice(), *is_dir)).collect();
        let mut kept = Vec::with_capacity(self.rows.len());
        let mut present: HashSet<Vec<u8>> = HashSet::new();
        for row in 0..self.rows.len() {
            #[cfg(test)]
            self.scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let raw = self.raw(row);
            let path = raw.as_slice();
            let Some(below) = path.strip_prefix(prefix.as_slice()) else {
                kept.push(row);
                continue;
            };
            let child = &path[..prefix.len() + below.iter().position(|&byte| byte == b'/').unwrap_or(below.len())];
            // A name that changed kind is a new entry: its old row and the rows below it go, and the push below adds it again.
            let holds = match kinds.get(child) {
                Some(&is_dir) if child.len() == path.len() => is_dir == self.is_dir(row),
                Some(&is_dir) => is_dir,
                None => false,
            };
            if holds {
                kept.push(row);
                if child.len() == path.len() {
                    present.insert(path.to_vec());
                }
            }
        }
        self.survivors = kept.len();
        if kept.len() < self.rows.len() {
            if !self.compact(&kept) { return false; }
            self.kept = Some(kept);
        }
        let mut added_dirs = Vec::new();
        for (child, is_dir) in seen {
            if !present.contains(&child) {
                if is_dir {
                    added_dirs.push(child.clone());
                }
                self.push(OsStr::from_bytes(&child), is_dir);
            }
        }
        if !added_dirs.is_empty() {
            self.fill_subtrees(root, &added_dirs, hidden, body);
        }
        true
    }

    // Walks newly added directories in, so a moved tree lands whole; rows already under them are read once.
    fn fill_subtrees(&mut self, root: &Path, added: &[Vec<u8>], hidden: bool, body: &str) {
        let prefixes: Vec<Vec<u8>> = added.iter().map(|dir| [dir.as_slice(), b"/"].concat()).collect();
        let mut present: HashSet<Vec<u8>> = HashSet::new();
        for row in 0..self.rows.len() {
            #[cfg(test)]
            self.scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let raw = self.raw(row);
            let path = raw.as_slice();
            if prefixes.iter().any(|prefix| path.starts_with(prefix)) {
                present.insert(path.to_vec());
            }
        }
        let mut pending = added.to_vec();
        while let Some(dir_rel) = pending.pop() {
            if self.storage_refused() {
                return;
            }
            let Ok(Some(entries)) = read_bounded(&root.join(OsStr::from_bytes(&dir_rel)), body) else {
                self.mark_partial("unreadable");
                continue;
            };
            if let Some(reason) = entries.partial { self.mark_partial(reason); }
            for (name, is_dir) in entries.entries {
                if !hidden && is_hidden(&name) {
                    continue;
                }
                let child = [dir_rel.as_slice(), b"/", name.as_bytes()].concat();
                if present.insert(child.clone()) {
                    self.push(OsStr::from_bytes(&child), is_dir);
                    if is_dir {
                        pending.push(child);
                    }
                }
            }
        }
    }
}
