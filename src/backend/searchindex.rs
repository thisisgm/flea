// The session snapshot stores basename ids and parents; paths exist only in caller scratch.
use crate::backend::fuzzy::{fold_packed, rank_order, Hay, Packed};
use super::searchblocks::{display_len, Blocks};
use super::searchnames::{Names, Refusal, EMPTY};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub(crate) const WALK_THREADS: usize = 4;
pub(crate) const PARALLEL_SCORE_AT: usize = 2048;

struct Row {
    name: u32,
    parent: u32,
}

pub(crate) struct Index {
    names: Names,
    blocks: Blocks,
    rows: Vec<Row>,
    kinds: Vec<u8>,
    pub(crate) partial: Option<Refusal>,
    kept: Option<Vec<usize>>,
    survivors: usize,
    #[cfg(test)]
    pub(crate) compactions: usize,
    #[cfg(test)]
    pub(crate) scanned: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) row_limit: usize,
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
            partial: None, kept: None, survivors: 0,
            #[cfg(test)]
            compactions: 0,
            #[cfg(test)]
            scanned: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            row_limit: u32::MAX as usize,
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
        size_of::<Self>() + self.names.bytes(allocated) + self.blocks.bytes(allocated)
            + count(self.rows.len(), self.rows.capacity()) * size_of::<Row>()
            + count(self.kinds.len(), self.kinds.capacity())
            + self.kept.as_ref().map_or(0, |kept| count(kept.len(), kept.capacity()) * size_of::<usize>())
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

    fn reconstruct(&self, row: usize, scratch: &mut Scratch) {
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
        self.blocks.text_len(row.parent) as usize + usize::from(row.parent != EMPTY)
            + display_len(self.names.get(row.name))
    }

    pub(crate) fn is_dir(&self, row: usize) -> bool {
        self.kinds[row / 8] & (1 << (row % 8)) != 0
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
        self.blocks.append(parent, row)?;
        if row as usize / 8 == self.kinds.len() { self.kinds.push(0); }
        if is_dir { self.kinds[row as usize / 8] |= 1 << (row % 8); }
        self.rows.push(Row { name, parent });
        Ok(())
    }

}

#[path = "searchindexpatch.rs"]
mod patch;

// The old scorer receives an identical packed full path assembled in reusable worker scratch.
#[derive(Default)]
struct Scratch {
    ancestors: Vec<u32>,
    raw: Vec<u8>,
    ascii: Vec<u8>,
}

fn score_part(fuzzy: &crate::backend::fuzzy::Fuzzy, rows: &[usize], index: &Index) -> Vec<(i32, usize)> {
    let mut scratch = Scratch::default();
    rows.iter().filter_map(|&row| {
        index.reconstruct(row, &mut scratch);
        let text = String::from_utf8_lossy(&scratch.raw);
        let score = if text.is_ascii() {
            scratch.ascii.clear();
            scratch.ascii.extend(text.bytes().map(|byte| byte.to_ascii_lowercase() | (u8::from(byte.is_ascii_uppercase()) << 7)));
            fuzzy.score_pre_folded(Hay::Ascii(&scratch.ascii))
        } else {
            match fold_packed(&text) {
                Packed::Ascii(bytes) => fuzzy.score_pre_folded(Hay::Ascii(&bytes)),
                Packed::Wide(words) => fuzzy.score_pre_folded(Hay::Wide(&words)),
            }
        };
        score.map(|score| (score, row))
    }).collect()
}

// A patch distinguishes a vanished directory from a refused or incomplete read.
fn read_bounded(dir: &Path, body: &str) -> Result<Option<super::searchread::Directory>, crate::error::FleaError> {
    let owned = dir.to_path_buf();
    match super::iomount::call(dir, body, "search", move || super::searchread::read(&owned))? {
        Ok(read) => Ok(Some(read)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(crate::error::from_io("search", &dir.to_string_lossy(), &error)),
    }
}

// Scores candidates with the session's scorer; wide indexes split over the same walker bound.
pub(crate) fn score_all(fuzzy: &crate::backend::fuzzy::Fuzzy, rows: &[usize], index: &Index) -> Vec<(i32, usize)> {
    if rows.len() < PARALLEL_SCORE_AT {
        return score_part(fuzzy, rows, index);
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, WALK_THREADS);
    let chunk = rows.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for part in rows.chunks(chunk) {
            handles.push(scope.spawn(move || {
                score_part(fuzzy, part, index)
            }));
        }
        let mut out = Vec::with_capacity(rows.len());
        for handle in handles {
            out.extend(handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)));
        }
        out
    })
}

// The one rank comparator for answers and rest chunks, so a scrolled window only extends the head's order.
pub(crate) fn rank_cmp(index: &Index, a: &(i32, usize), b: &(i32, usize)) -> CmpOrdering {
    b.0.cmp(&a.0).then_with(|| index.path_len(a.1).cmp(&index.path_len(b.1)))
        .then_with(|| rank_order(a.0, &index.path(a.1), b.0, &index.path(b.1)))
}

// The best `window` hits in rank order, the rest unranked; the path comparator runs only on the window and its edge ties.
pub(crate) fn select_head<K, F, C>(hits: &mut Vec<(i32, usize)>, window: usize, key: F, cmp: C) -> Vec<(i32, usize)>
where
    K: Ord,
    F: Fn(&(i32, usize)) -> K,
    C: Fn(&(i32, usize), &(i32, usize)) -> CmpOrdering,
{
    if window > 0 && window < hits.len() {
        hits.select_nth_unstable_by_key(window - 1, &key);
        let edge = key(&hits[window - 1]);
        // Hits tied with the edge on the integer key can sit on either side of the cut, so the comparator picks among them.
        if hits[window..].iter().any(|hit| key(hit) == edge) {
            let (mut sure, mut tied, mut past) = (Vec::with_capacity(window), Vec::new(), Vec::new());
            for &hit in hits.iter() {
                match key(&hit).cmp(&edge) {
                    CmpOrdering::Less => sure.push(hit),
                    CmpOrdering::Equal => tied.push(hit),
                    CmpOrdering::Greater => past.push(hit),
                }
            }
            // The edge hit is tied and inside the window, so at least one tied hit joins the head.
            tied.select_nth_unstable_by(window - sure.len() - 1, &cmp);
            *hits = [sure, tied, past].concat();
        }
    }
    let rest = hits.split_off(window.min(hits.len()));
    hits.sort_by(&cmp);
    rest
}

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
mod fixture_tests;
