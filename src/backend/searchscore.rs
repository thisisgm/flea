// Scoring visits candidate name ids and their parents; reusable storage belongs to the session.
use super::*;
use crate::backend::searchmatch::{fold, Letter, Matcher, State, ALIGNED, BASENAME_BONUS, BOUNDARY_BONUS, EXACT, HIT_SPAN, NAME, PENDING_ALIGNED, PENDING_NAME,
    PENDING_PATH, PREFIX, RUN_BONUS, SHADOW_TYPO, WORD};
use crate::backend::searchbits;

#[path = "searchscratch.rs"]
mod scratch;
pub(crate) use scratch::QueryScratch;

#[cfg(test)]
pub(crate) fn score_all(matcher: &Matcher, rows: &[usize], index: &Index) -> Vec<(i32, usize)> {
    score_cached(matcher, rows, index, &mut QueryScratch::default())
}

pub(crate) fn score_cached(matcher: &Matcher, rows: &[usize], index: &Index, scratch: &mut QueryScratch) -> Vec<(i32, usize)> {
    #[cfg(test)]
    for counter in [&index.names_scored, &index.dirs_scored, &index.paths_scored] {
        counter.store(0, std::sync::atomic::Ordering::Relaxed);
    }
    scratch.begin(index);
    scratch.names(matcher, None, rows, index);
    let hits = score_rows(matcher, rows, index, scratch);
    scratch.end();
    hits
}

// Pass 1, with typo work only when no basename holds the query exactly, as prefix or at a word start; true when hits hold every typo match.
pub(crate) fn score_shown(matcher: &Matcher, rows: &[usize], index: &Index, scratch: &mut QueryScratch) -> (Vec<(i32, usize)>, bool) {
    if matcher.budget == 0 { return (score_cached(matcher, rows, index, scratch), true); }
    #[cfg(test)]
    for counter in [&index.names_scored, &index.dirs_scored, &index.paths_scored] {
        counter.store(0, std::sync::atomic::Ordering::Relaxed);
    }
    scratch.begin(index);
    let strict = matcher.strict();
    let real = scratch.names(matcher, Some(&strict), rows, index);
    // A name table can hold names no row uses any more, so the rows confirm a real match.
    if real > 0 {
        let hits = score_rows(&strict, rows, index, scratch);
        if hits.iter().any(|hit| class(hit.0) <= class(WORD)) {
            scratch.end();
            return (hits, false);
        }
    }
    scratch.fill_typos(matcher, index);
    let hits = score_rows(matcher, rows, index, scratch);
    scratch.end();
    (hits, true)
}

// The row half of pass 1: each row's name score, lifted by its parents where a path check can.
fn score_rows(matcher: &Matcher, rows: &[usize], index: &Index, scratch: &mut QueryScratch) -> Vec<(i32, usize)> {
    let path_edits = if matcher.needle.contains(&'/') { matcher.budget } else { 0 };
    let mut first = [0; 4];
    let first_bits = matcher.needle.first().map_or(0, |c| searchbits::mask(c.encode_utf8(&mut first).as_bytes()));
    let threads = if rows.len() < PARALLEL_ROWS_AT { 1 } else { scratch.threads(rows.len()) };
    if threads == 1 {
        let mut hits = Vec::new();
        let mut temps = std::mem::take(&mut scratch.temps);
        for &row in rows {
            let name = scratch.name(index.rows[row].name);
            if let Some(score) = score_row(matcher, index, &mut *scratch, &mut temps, row, name, path_edits, first_bits) { hits.push((score, row)); }
        }
        temps.flush(index);
        scratch.temps = temps;
        hits
    } else {
        scratch.rows_parallel(index, rows, threads, |cache, temps, row, name| score_row(matcher, index, cache, temps, row, name, path_edits, first_bits))
    }
}

// Rows past this many split into contiguous runs, one per core, each with its own cache of the parent it is on.
const PARALLEL_ROWS_AT: usize = 32_768;

// What a row's path checks read from its parent directory, each computed at most once per directory and query.
trait Parents {
    fn parent_reach(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> usize;
    fn parent_typo(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> bool;
    fn parent_state(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> &State;
    fn parent_letters(&mut self, index: &Index, dir: u32) -> &[Letter];
}

// One row's buffers, reused from row to row.
#[derive(Default)]
struct RowTemps {
    letters: Vec<Letter>,
    path_letters: Vec<Letter>,
    edits: crate::backend::searchtypo::Scratch,
    row_state: Option<State>,
    // Test counts gather per thread and reach the index once per run, so threads never contend on one counter.
    #[cfg(test)]
    tally: [usize; 3],
}

#[cfg(test)]
const PATHS_SCORED: usize = 0;
#[cfg(test)]
const PATH_DP: usize = 1;
#[cfg(test)]
const NAME_DP: usize = 2;

impl RowTemps {
    fn flush(&mut self, _index: &Index) {
        #[cfg(test)]
        for (counter, count) in [&_index.paths_scored, &_index.path_dp, &_index.name_dp].into_iter().zip(std::mem::take(&mut self.tally)) {
            counter.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

// One row's pass-1 score from its name's score and its parent's checks; None when the row does not match.
#[allow(clippy::too_many_arguments)]
fn score_row<P: Parents>(matcher: &Matcher, index: &Index, parents: &mut P, temps: &mut RowTemps, row: usize, name: Option<i32>,
    path_edits: usize, first_bits: u64) -> Option<i32> {
    let entry = &index.rows[row];
    let mut score = name;
    let path_limit = index.blocks.text_len(entry.parent) as usize + usize::from(entry.parent != EMPTY)
        + index.names.get(entry.name).len() + path_edits;
    let bits = index.blocks.bits(entry.parent);
    // A basename subsequence still takes the whole-path DP, which holds every basename alignment, so its parents can lift it.
    if matcher.needle.is_empty() || score.is_some_and(|score| score > NAME + HIT_SPAN || score == PENDING_NAME) || entry.parent == EMPTY
        || matcher.needle.len() > path_limit
        || !searchbits::possible(first_bits, bits, path_edits)
        || !searchbits::possible(matcher.parent_bits, bits, path_edits)
        || !searchbits::possible(matcher.bits, bits | index.names.bits(entry.name as usize), path_edits) {
        return score;
    }
    let reach = parents.parent_reach(matcher, index, entry.parent);
    if path_edits == 0 && (!searchbits::possible(matcher.suffix_bits[reach], index.names.bits(entry.name as usize), 0)
        || matcher.reach(index.names.get(entry.name), reach) != matcher.needle.len()) {
        return score;
    }
    let exact_prefix = reach >= matcher.prefix;
    let typo_prefix = !exact_prefix && path_edits > 0 && parents.parent_typo(matcher, index, entry.parent);
    if !exact_prefix && !typo_prefix { return score; }
    if exact_prefix && matcher.reach(index.names.get(entry.name), reach) == matcher.needle.len() {
        // A path DP hit scores at least NAME - HIT_SPAN, above every typo score, so neither typo arm could change this row.
        if matcher.defer {
            #[cfg(test)]
            { temps.tally[PATHS_SCORED] += 1; }
            return Some(if matcher.needle.contains(&'/') { PENDING_ALIGNED } else { PENDING_PATH });
        }
        let parent = parents.parent_state(matcher, index, entry.parent);
        let state = temps.row_state.get_or_insert_with(|| State::new(matcher.needle.len()));
        state.copy_from(parent);
        matcher.advance(state, index.names.get(entry.name), &mut temps.letters, true);
        #[cfg(test)]
        { temps.tally[PATH_DP] += 1; }
        score = score.max(state.score(if matcher.needle.contains(&'/') { ALIGNED } else { NAME }));
    }
    if path_edits > 0 && !score.is_some_and(|score| score >= ALIGNED - HIT_SPAN) {
        temps.path_letters.clear();
        temps.path_letters.extend_from_slice(parents.parent_letters(index, entry.parent));
        fold(index.names.get(entry.name), &mut temps.letters);
        temps.path_letters.extend_from_slice(&temps.letters);
        score = score.max(matcher.distance(&temps.path_letters, &mut temps.edits)
            .map(|distance| crate::backend::searchmatch::TYPO - distance as i32 * crate::backend::searchmatch::TYPO_EDIT));
    }
    #[cfg(test)]
    { temps.tally[PATHS_SCORED] += 1; }
    score
}

pub(crate) const CLASSES: usize = 6;

// The class of a pass-1 score, best first; no prior bonus lets a row outrank a row of an earlier class.
pub(crate) fn class(score: i32) -> usize {
    match score {
        PENDING_ALIGNED => class(ALIGNED),
        PENDING_NAME | PENDING_PATH => class(NAME),
        _ => [EXACT, PREFIX, WORD, ALIGNED - HIT_SPAN, NAME - HIT_SPAN].iter()
            .position(|&floor| score >= floor).unwrap_or(CLASSES - 1),
    }
}

// The pass-1 hits of each class, best class first.
pub(crate) fn class_counts(hits: &[(i32, usize)]) -> [usize; CLASSES] {
    let mut counts = [0usize; CLASSES];
    for hit in hits { counts[class(hit.0)] += 1; }
    counts
}

// The last class the best `window` hits reach; None when the window is empty.
pub(crate) fn boundary(hits: &[(i32, usize)], window: usize) -> Option<usize> {
    last_class(&class_counts(hits), window)
}

pub(crate) fn last_class(counts: &[usize; CLASSES], window: usize) -> Option<usize> {
    let window = window.min(counts.iter().sum());
    if window == 0 { return None; }
    let mut reached = 0;
    counts.iter().position(|&count| { reached += count; reached >= window })
}

// Shadow rows are never listed, and typo rows only when no basename holds the query exactly, as its prefix or at a word start.
pub(crate) fn hide_typos(hits: &mut Vec<(i32, usize)>, counts: &mut [usize; CLASSES], budget: usize) -> usize {
    if budget == 0 || counts[CLASSES - 1] == 0 { return 0; }
    let real = counts[..=class(WORD)].iter().any(|&count| count > 0);
    let before = hits.len();
    hits.retain(|hit| class(hit.0) < CLASSES - 1 || (!real && hit.0 > SHADOW_TYPO));
    counts[CLASSES - 1] -= before - hits.len();
    before - hits.len()
}

// The head resolved, minus every row scored or bounded under the window's floor; path-only rows bounded under it skip their DP.
pub(crate) fn resolve_head(matcher: &Matcher, index: &Index, scratch: &mut QueryScratch, head: &mut Vec<(i32, usize)>, window: usize)
    -> Vec<(i32, usize)> {
    // The head keeps its own buffer and order; only the path rows move out, into one exact allocation.
    let mut kept = std::mem::take(head);
    let mut paths = Vec::with_capacity(kept.iter().filter(|hit| hit.0 == PENDING_PATH).count());
    kept.retain(|hit| if hit.0 == PENDING_PATH { paths.push(*hit); false } else { true });
    resolve(matcher, index, scratch, &mut kept);
    let floor = if paths.is_empty() { None } else { window_floor(&kept, window) };
    let (mut admitted, mut low) = match floor {
        None => (paths, Vec::new()),
        Some(floor) if paths.len() < PARALLEL_ROWS_AT => {
            let mut letters = std::mem::take(&mut scratch.letters);
            let split = paths.into_iter().partition(|hit| path_bound(matcher, index, &mut *scratch, &mut letters, hit.1, floor) >= floor);
            scratch.letters = letters;
            split
        }
        Some(floor) => {
            let threads = scratch.threads(paths.len());
            scratch.partition_parallel(paths, threads, |cache, temps, row| path_bound(matcher, index, cache, &mut temps.letters, row, floor) >= floor)
        }
    };
    resolve(matcher, index, scratch, &mut admitted);
    kept.append(&mut admitted);
    if let Some(floor) = window_floor(&kept, window) {
        low.reserve(kept.iter().filter(|hit| hit.0 < floor).count());
        kept.retain(|hit| if hit.0 >= floor { true } else { low.push(*hit); false });
    }
    *head = kept;
    low
}

// The window-th best raw score: a row under it never reaches the window, because a prior adds under one raw point.
fn window_floor(hits: &[(i32, usize)], window: usize) -> Option<i32> {
    if window == 0 || hits.len() < window { return None; }
    let mut raws: Vec<i32> = hits.iter().map(|hit| hit.0).collect();
    Some(*raws.select_nth_unstable_by(window - 1, |a, b| b.cmp(a)).1)
}

// The most a path-only row can score: no run crosses its parent's closing `/`, so the first basename letter adds a word start and each later one a run word start.
fn bound(parent: &State, len: usize, from: usize) -> i32 {
    let first = BOUNDARY_BONUS + BASENAME_BONUS;
    let later = RUN_BONUS + first;
    let hit = (from..len).map(|at| parent.best()[at] + first + later * (len - 1 - at) as i32).fold(parent.hit, i32::max);
    NAME + hit.clamp(-HIT_SPAN, HIT_SPAN)
}

// bound for a basename holding the whole needle: each split's parent column plus its lift, capped by bound's run of word starts.
fn lift_bound(parent: &State, len: usize, lift: i32) -> i32 {
    let first = BOUNDARY_BONUS + BASENAME_BONUS;
    let later = RUN_BONUS + first;
    let hit = (1..len).map(|at| parent.best()[at] + lift.min(first + later * (len - 1 - at) as i32)).fold(parent.hit, i32::max);
    NAME + hit.clamp(-HIT_SPAN, HIT_SPAN)
}

// The name bits give a first j for free; only a row they cannot rule out pays a fold for the exact one.
fn path_bound<P: Parents>(matcher: &Matcher, index: &Index, parents: &mut P, letters: &mut Vec<Letter>, row: usize, floor: i32) -> i32 {
    let entry = &index.rows[row];
    let state = parents.parent_state(matcher, index, entry.parent);
    let len = matcher.needle.len();
    let bits = index.names.bits(entry.name as usize);
    let possible = (0..len).find(|&at| searchbits::possible(matcher.suffix_bits[at], bits, 0)).unwrap_or(len);
    let rough = bound(state, len, possible);
    if rough < floor { return rough; }
    bound(state, len, held_suffix(matcher, index.names.get(entry.name), letters))
}

// Where the longest needle suffix a basename holds starts, matched backwards; only a non-ASCII name is folded first.
fn held_suffix(matcher: &Matcher, name: &[u8], letters: &mut Vec<Letter>) -> usize {
    let mut from = matcher.needle.len();
    if name.is_ascii() {
        for &byte in name.iter().rev() {
            if from > 0 && byte.to_ascii_lowercase() as char == matcher.needle[from - 1] { from -= 1; }
        }
        return from;
    }
    fold(name, letters);
    for letter in letters.iter().rev() {
        if from > 0 && letter.lower == matcher.needle[from - 1] { from -= 1; }
    }
    from
}

// Exact scores for marked rows by score_cached's DP; on a subsequence match best[] only decays by one a letter, so it always hits.
pub(crate) fn resolve(matcher: &Matcher, index: &Index, scratch: &mut QueryScratch, hits: &mut [(i32, usize)]) {
    let pending = hits.iter().filter(|hit| hit.0 < 0).count();
    // Each thread builds the parent states it reads, so the rows split across the name scorer's threads.
    let threads = scratch.threads(pending);
    let owned = scratch.lend_names();
    if threads == 1 {
        let mut temps = std::mem::take(&mut scratch.temps);
        for hit in hits.iter_mut().filter(|hit| hit.0 < 0) { resolve_row(matcher, index, &mut *scratch, &mut temps, &owned, hit); }
        temps.flush(index);
        scratch.temps = temps;
    } else {
        let owned = &owned;
        scratch.resolve_parallel(index, hits, threads, pending, |cache, temps, hit| resolve_row(matcher, index, cache, temps, owned, hit));
    }
    scratch.return_names(owned);
    scratch.end();
}

// One pending row's exact score: its parent's column carried through its basename.
fn resolve_row<P: Parents>(matcher: &Matcher, index: &Index, parents: &mut P, temps: &mut RowTemps, owned: &scratch::Owned, hit: &mut (i32, usize)) {
    let entry = &index.rows[hit.1];
    let tier = if hit.0 == PENDING_ALIGNED { ALIGNED } else { NAME };
    // Only a row at the scope's top scores on its basename alone; every other match takes the whole-path DP.
    let path = entry.parent != EMPTY;
    // A basename contains no slash, so a query ending in one can only have finished in its parent.
    #[cfg(test)]
    let parent_score = matcher.parent_score;
    #[cfg(not(test))]
    let parent_score = true;
    if parent_score && path && matcher.needle.last() == Some(&'/') {
        if let Some(score) = parents.parent_state(matcher, index, entry.parent).score(tier) {
            hit.0 = score;
            return;
        }
    }
    // The path DP's basename-only alignments score the name's own score, and bound caps every one that takes a parent letter.
    if let Some(own) = owned.get(entry.name).filter(|_| hit.0 == PENDING_NAME) {
        if !path || lift_bound(parents.parent_state(matcher, index, entry.parent), matcher.needle.len(), own.lift()) <= own.score() {
            hit.0 = own.score();
            // The row takes its name's DP score, read once by the name pass.
            #[cfg(test)]
            { temps.tally[NAME_DP] += 1; }
            return;
        }
    }
    let state = temps.row_state.get_or_insert_with(|| State::new(matcher.needle.len()));
    // resize also resets every cell, which a fresh name DP needs and a path DP gets from copy_from.
    if !path { state.resize(matcher.needle.len()); }
    else { state.copy_from(parents.parent_state(matcher, index, entry.parent)); }
    matcher.advance(state, index.names.get(entry.name), &mut temps.letters, true);
    let score = state.score(tier);
    debug_assert!(score.is_some(), "a subsequence match has a DP hit");
    hit.0 = score.unwrap_or(tier - HIT_SPAN);
    #[cfg(test)]
    { temps.tally[if path { PATH_DP } else { NAME_DP }] += 1; }
}
