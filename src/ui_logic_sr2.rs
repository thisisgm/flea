//! Filter040, the name filter: its matcher and the listing-row and view-position spaces it joins (only `at` and `view_of` convert), shared by the window, the picker and the TUI.
pub mod matcher;
pub mod narrow;
mod counted;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cursor;
#[cfg(test)]
mod tests_matcher;
#[cfg(test)]
mod tests_cost;
#[cfg(test)]
mod tests_bound;
#[cfg(test)]
mod tests_fixture;
#[cfg(test)]
mod tests_same;

pub use matcher::{Close, Matcher, Run};
pub use narrow::{fuzzy_score, longest_run, narrow, runs_of, Narrowed};
#[cfg(test)]
use counted::{PASSES, RECORDED, STEPS};
use counted::{pass, recorded, steps, Name};

/// The rows of one match list as a bitset, built once per answer so each test of a row is one step.
struct Members(Vec<u64>);

impl Members {
    fn of(rows: &[usize]) -> Self {
        let mut members = Self(Vec::new());
        rows.iter().for_each(|&row| members.insert(row));
        members
    }

    fn insert(&mut self, row: usize) {
        steps(1);
        let word = row / 64;
        if word >= self.0.len() {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1 << (row % 64);
    }

    fn has(&self, row: usize) -> bool {
        steps(1);
        self.0.get(row / 64).is_some_and(|word| word & (1 << (row % 64)) != 0)
    }
}

/// The listing row drawn at a view position; None past the end, where a shrinking list can leave a stale position.
pub fn at(list: Option<&[usize]>, view: usize) -> Option<usize> {
    match list {
        None => Some(view),
        Some(list) => list.get(view).copied(),
    }
}

/// The view position a listing row is drawn at; None for a row the filter hides.
pub fn view_of(list: Option<&[usize]>, row: usize) -> Option<usize> {
    match list {
        None => Some(row),
        Some(list) => list.iter().position(|&shown| shown == row),
    }
}

/// How many positions a view draws.
pub fn shown_total(list: Option<&[usize]>, total: usize) -> usize {
    list.map_or(total, <[usize]>::len)
}

/// The rows drawn between two view positions, inclusive either way round and in drawn order, never a row the filter hid.
pub fn between(list: &[usize], a: usize, b: usize) -> Vec<usize> {
    let (lo, hi) = (a.min(b), a.max(b));
    list.get(lo..=hi.min(list.len().saturating_sub(1))).map_or_else(Vec::new, <[usize]>::to_vec)
}

/// A request cut back to the rows still drawn, since a filtered viewport covers a set and not a run.
pub fn keep(asked: &[usize], list: Option<&[usize]>) -> Vec<usize> {
    match list {
        None => asked.to_vec(),
        Some(list) => {
            let drawn = Members::of(list);
            asked.iter().copied().filter(|&row| drawn.has(row)).collect()
        }
    }
}

/// A thumbnail plan cut to drawn rows, dropping every pending row the filter hides that is not already being dropped.
pub fn cut(ask: &[usize], drop: &[usize], list: Option<&[usize]>, pending: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mut dropped = drop.to_vec();
    if let Some(list) = list {
        let (drawn, mut dropping) = (Members::of(list), Members::of(drop));
        for &row in pending {
            if !drawn.has(row) && !dropping.has(row) {
                dropping.insert(row);
                dropped.push(row);
            }
        }
    }
    (keep(ask, list), dropped)
}

/// The listing rows a view range covers, for planners that take a first and a last; None when the view draws nothing.
pub fn span(list: Option<&[usize]>, first: usize, last: usize) -> Option<(usize, usize)> {
    let Some(list) = list else { return Some((first, last)) };
    if list.is_empty() {
        return None;
    }
    let lo = first.min(list.len() - 1);
    let hi = last.min(list.len() - 1).max(lo);
    let covered = &list[lo..=hi];
    Some((*covered.iter().min()?, *covered.iter().max()?))
}

/// Whether the cursor is a row the filter draws, issue 92's rule for the fallback target.
pub fn cursor_shown(list: Option<&[usize]>, cursor: Option<usize>) -> bool {
    cursor.is_some_and(|row| view_of(list, row).is_some())
}

/// Why an action on the cursor went nowhere, checked in ui/js/Ops.js's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NothingToActOn,
    NotLoaded,
    HiddenByFilter,
}

/// The rows an action takes: the selection, else the cursor while the filter draws it and its row is loaded.
pub fn targets(selected: &[usize], list: Option<&[usize]>, cursor: Option<usize>, loaded: impl Fn(usize) -> bool) -> Result<Vec<usize>, Refusal> {
    if !selected.is_empty() {
        return Ok(selected.to_vec());
    }
    let row = cursor.ok_or(Refusal::NothingToActOn)?;
    if !loaded(row) {
        return Err(Refusal::NotLoaded);
    }
    if !cursor_shown(list, Some(row)) {
        return Err(Refusal::HiddenByFilter);
    }
    Ok(vec![row])
}

/// The marked rows a new match list no longer draws, which the selection must lose before anything acts on them.
pub fn pruned(selected: &[usize], list: Option<&[usize]>) -> Vec<usize> {
    list.map_or_else(Vec::new, |list| {
        let drawn = Members::of(list);
        selected.iter().copied().filter(|&row| !drawn.has(row)).collect()
    })
}

/// Where a cursor goes when the list changes: kept while drawn, else onto the first row left standing.
pub fn cursor_after(list: Option<&[usize]>, cursor: usize) -> Option<usize> {
    match list {
        Some(list) if !list.is_empty() && !list.contains(&cursor) => Some(list[0]),
        _ => None,
    }
}

/// A cursor set to a view position, clamped to what is drawn: the listing row and the position to scroll to.
pub fn cursor_at_view(list: Option<&[usize]>, total: usize, view: isize) -> Option<(usize, usize)> {
    let drawn = shown_total(list, total);
    if drawn == 0 {
        return None;
    }
    let to = view.clamp(0, drawn as isize - 1) as usize;
    Some((at(list, to)?, to))
}

/// One step through the drawn rows, which is not one step through the listing.
pub fn step(list: Option<&[usize]>, total: usize, cursor: usize, delta: isize) -> Option<(usize, usize)> {
    let from = view_of(list, cursor).map_or(-1, |view| view as isize);
    cursor_at_view(list, total, from + delta)
}

/// The cursor pulled onto a viewport the wheel moved, both ends view positions; None when it is already inside.
pub fn clamp(list: Option<&[usize]>, cursor: usize, first: usize, last: usize) -> Option<usize> {
    let was = view_of(list, cursor)?;
    let to = was.clamp(first, last.max(first));
    (to != was).then(|| at(list, to)).flatten()
}

/// ui/js/Match.js base(): a walked row's own name, the last segment of its relative path.
pub fn base(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// ui/js/Match.js location(): the folder a walked row names after its own name, empty in the root.
pub fn location(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Where the filter tests names: this folder's listing, or its tree walked as search walks it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Folder,
    Subfolders,
}

impl Scope {
    /// Tab flips the scope, and Tab again returns to this folder.
    pub fn toggled(self) -> Self {
        match self {
            Self::Folder => Self::Subfolders,
            Self::Subfolders => Self::Folder,
        }
    }
}

/// The strip's counts, the numbers its sentence carries; the caller translates them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub shown: usize,
    pub tested: usize,
    pub close: usize,
    pub scope: Scope,
}

impl Counts {
    pub fn of(narrowed: &Narrowed, tested: usize, scope: Scope) -> Self {
        Self { shown: narrowed.exact.len(), tested, close: narrowed.close.len(), scope }
    }

    /// Nothing exact and nothing close: the pane shows the no-match state with the query as typed.
    pub fn nothing(&self) -> bool {
        self.shown == 0 && self.close == 0
    }
}
