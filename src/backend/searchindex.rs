// The session snapshot stores basename ids and parents; paths exist only in caller scratch.
use super::searchblocks::{display_len, Blocks};
use super::searchnames::{Names, Refusal, EMPTY};
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub(crate) const WALK_THREADS: usize = 4;
pub(crate) const PARALLEL_SCORE_AT: usize = 2048;

struct Row {
    name: u32,
    parent: u32,
    next: u32,
    previous: u32,
}

pub(crate) struct Index {
    names: Names,
    blocks: Blocks,
    rows: Vec<Row>,
    kinds: Vec<u8>,
    pub(crate) partial: Option<Refusal>,
    moves: Vec<(usize, usize)>,
    survivors: usize,
    pub(crate) epoch: u64,
    retired_bytes: usize,
    #[cfg(test)]
    pub(crate) compactions: usize,
    #[cfg(test)]
    pub(crate) scanned: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) row_limit: usize,
    #[cfg(test)]
    pub(crate) reconstructed: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) names_scored: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) dirs_scored: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) paths_scored: std::sync::atomic::AtomicUsize,
    // Name and path DP evaluations, the work the head-bounded ranking defers.
    #[cfg(test)]
    pub(crate) name_dp: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) path_dp: std::sync::atomic::AtomicUsize,
}

pub(crate) fn index_bytes(index: &Index) -> usize {
    index.bytes(false)
}

pub(crate) fn is_hidden(name: &OsStr) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

impl Index {
    pub(crate) fn new() -> Self {
        Self {
            names: Names::new(), blocks: Blocks::new(), rows: Vec::new(), kinds: Vec::new(),
            partial: None, moves: Vec::new(), survivors: 0, epoch: 0, retired_bytes: 0,
            #[cfg(test)]
            compactions: 0,
            #[cfg(test)]
            scanned: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            row_limit: u32::MAX as usize,
            #[cfg(test)]
            reconstructed: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            names_scored: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            dirs_scored: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            paths_scored: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            name_dp: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            path_dp: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn mark_partial(&mut self, reason: Refusal) {
        if matches!(reason, "capacity" | "allocation") {
            self.partial = Some(reason);
        } else {
            self.partial.get_or_insert(reason);
        }
    }

    fn storage_refused(&self) -> bool {
        matches!(self.partial, Some("allocation" | "capacity"))
    }

    pub(crate) fn finish(&mut self) {
        self.names.finish();
    }

    pub(crate) fn bytes(&self, allocated: bool) -> usize {
        let count = |len, capacity| if allocated { capacity } else { len };
        std::mem::size_of::<Self>() + self.names.bytes(allocated) + self.blocks.bytes(allocated)
            + count(self.rows.len(), self.rows.capacity()) * std::mem::size_of::<Row>()
            + count(self.kinds.len(), self.kinds.capacity())
            + count(self.moves.len(), self.moves.capacity()) * std::mem::size_of::<(usize, usize)>()
    }

    // Owned bridges make reconstruction visible to callers; no hidden full-path cache exists.
    pub(crate) fn path(&self, row: usize) -> String {
        String::from_utf8_lossy(&self.raw(row)).into_owned()
    }

    pub(crate) fn raw(&self, row: usize) -> Vec<u8> {
        let mut scratch = Scratch::default();
        self.reconstruct(row, &mut scratch);
        scratch.raw
    }

    pub(crate) fn raw_into<'a>(&self, row: usize, scratch: &'a mut Scratch) -> &'a [u8] {
        self.reconstruct(row, scratch);
        &scratch.raw
    }

    fn reconstruct(&self, row: usize, scratch: &mut Scratch) {
        #[cfg(test)]
        self.reconstructed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let entry = &self.rows[row];
        self.blocks.ancestors(entry.parent, &mut scratch.ancestors);
        scratch.raw.clear();
        for &name in scratch.ancestors.iter().rev() {
            scratch.raw.extend_from_slice(self.names.get(name));
            scratch.raw.push(b'/');
        }
        scratch.raw.extend_from_slice(self.names.get(entry.name));
    }

    pub(crate) fn path_len(&self, row: usize) -> usize {
        let row = &self.rows[row];
        // A name of known bytes only is ASCII, so its display length is its byte length and its bytes stay unread.
        let name = self.names.get(row.name);
        let name = if self.names.bits(row.name as usize) & super::searchbits::UNKNOWN == 0 { name.len() } else { display_len(name) };
        self.blocks.text_len(row.parent) as usize + usize::from(row.parent != EMPTY) + name
    }

    pub(crate) fn is_dir(&self, row: usize) -> bool {
        self.kinds[row / 8] & (1 << (row % 8)) != 0
    }

    // The row's folder as a slot of a per-folder table: its directory block, or the last slot for the scope's top.
    pub(crate) fn folder_slot(&self, row: usize) -> usize {
        let parent = self.rows[row].parent;
        if parent == EMPTY { self.blocks.len() } else { parent as usize }
    }

    pub(crate) fn folder_slots(&self) -> usize {
        self.blocks.len() + 1
    }

    pub(crate) fn push(&mut self, path: impl AsRef<OsStr>, is_dir: bool) -> bool {
        if self.storage_refused() {
            return false;
        }
        if let Err(reason) = self.try_push(path.as_ref().as_bytes(), is_dir) {
            self.mark_partial(reason);
            return false;
        }
        true
    }

    fn parent(&mut self, path: &[u8]) -> Result<u32, Refusal> {
        let mut parent = EMPTY;
        if !path.is_empty() {
            for part in path.split(|&byte| byte == b'/') {
                let name = self.names.intern(part)?;
                parent = self.blocks.directory(&self.names, parent, name)?;
            }
        }
        Ok(parent)
    }

    pub(crate) fn push_directory(&mut self, path: &OsStr, entries: &[(std::ffi::OsString, bool)]) -> bool {
        let result = (|| {
            let parent = self.parent(path.as_bytes())?;
            for (name, is_dir) in entries {
                self.push_name(parent, name.as_bytes(), *is_dir)?;
            }
            Ok::<(), Refusal>(())
        })();
        if let Err(reason) = result {
            self.mark_partial(reason);
            return false;
        }
        true
    }

    fn try_push(&mut self, path: &[u8], is_dir: bool) -> Result<(), Refusal> {
        let (directory, name) = match path.iter().rposition(|&byte| byte == b'/') {
            Some(at) => (&path[..at], &path[at + 1..]),
            None => (&b""[..], path),
        };
        let parent = self.parent(directory)?;
        self.push_name(parent, name, is_dir)
    }

    fn push_name(&mut self, parent: u32, name: &[u8], is_dir: bool) -> Result<(), Refusal> {
        #[cfg(test)]
        if self.rows.len() >= self.row_limit { return Err("capacity"); }
        let row = u32::try_from(self.rows.len()).ok().filter(|&row| row != EMPTY).ok_or("capacity")?;
        let name = self.names.intern(name)?;
        self.rows.try_reserve(1).map_err(|_| "allocation")?;
        if row as usize / 8 == self.kinds.len() {
            self.kinds.try_reserve(1).map_err(|_| "allocation")?;
        }
        if row as usize / 8 == self.kinds.len() { self.kinds.push(0); }
        if is_dir { self.kinds[row as usize / 8] |= 1 << (row % 8); }
        let next = self.blocks.head(parent);
        self.rows.push(Row { name, parent, next, previous: EMPTY });
        if next != EMPTY { self.rows[next as usize].previous = row; }
        self.blocks.set_head(parent, row);
        Ok(())
    }

}

#[path = "searchindexpatch.rs"]
mod patch;

#[derive(Default)]
pub(crate) struct Scratch {
    ancestors: Vec<u32>,
    raw: Vec<u8>,
}

#[path = "searchscore.rs"]
mod scoring;
pub(crate) use scoring::{boundary, class, class_counts, hide_typos, last_class, resolve, resolve_head, score_cached, score_shown, QueryScratch};
#[cfg(test)]
pub(crate) use scoring::score_all;
#[cfg(test)]
pub(crate) use scoring::CLASSES;

// A patch distinguishes a vanished directory from a refused or incomplete read.
fn read_bounded(dir: &Path, body: &str, follow: bool) -> Result<Option<super::searchread::Directory>, crate::error::FleaError> {
    match super::searchread::bounded(dir, body, follow)? {
        Ok(read) => Ok(Some(read)),
        Err(error) if matches!(error.raw_os_error(), Some(2 | 20 | 40)) => Ok(None),
        Err(error) => Err(crate::error::from_io("search", &dir.to_string_lossy(), &error)),
    }
}

#[path = "searchorder.rs"]
mod order;
pub(crate) use order::{select_head, RankOrder};
#[cfg(test)]
pub(crate) use order::{cut_elapsed, rank_cmp};

#[cfg(test)]
#[path = "searchindex_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "searchraw_tests.rs"]
mod raw_tests;

#[cfg(test)]
#[path = "searchcoverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
#[path = "searchfixture_tests.rs"]
pub(crate) mod fixture_tests;

#[cfg(test)]
#[path = "searchfamily_oracle_tests.rs"]
pub(crate) mod family_oracle;

#[cfg(test)]
#[path = "searchlatency_tests.rs"]
mod latency_tests;

#[cfg(test)]
#[path = "searchprune_tests.rs"]
mod prune_tests;

#[cfg(test)]
#[path = "searchreview_tests.rs"]
mod review_tests;
