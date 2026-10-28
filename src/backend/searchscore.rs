// Scoring visits candidate name ids and their parents; reusable storage belongs to the session.
use super::*;
use crate::backend::searchmatch::{fold, Matcher, State, ALIGNED, BASENAME_BONUS, BOUNDARY_BONUS, EXACT, HIT_SPAN, NAME, PENDING_ALIGNED, PENDING_NAME,
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
    scratch.names(matcher, rows, index);
    let path_edits = if matcher.needle.contains(&'/') { matcher.budget } else { 0 };
    let mut hits = Vec::new();
    let mut first = [0; 4];
    let first_bits = matcher.needle.first().map_or(0, |c| searchbits::mask(c.encode_utf8(&mut first).as_bytes()));
    for &row in rows {
        let entry = &index.rows[row];
        let mut score = scratch.name(entry.name);
        let path_limit = index.blocks.text_len(entry.parent) as usize + usize::from(entry.parent != EMPTY)
            + index.names.get(entry.name).len() + path_edits;
        let bits = index.blocks.bits(entry.parent);
        // A basename subsequence still takes the whole-path DP, which holds every basename alignment, so its parents can lift it.
        if !matcher.needle.is_empty() && !score.is_some_and(|score| score > NAME + HIT_SPAN || score == PENDING_NAME) && entry.parent != EMPTY
            && matcher.needle.len() <= path_limit
            && searchbits::possible(first_bits, bits, path_edits)
            && searchbits::possible(matcher.parent_bits, bits, path_edits)
            && searchbits::possible(matcher.bits, bits | index.names.bits(entry.name as usize), path_edits) {
            let reach = scratch.reach(matcher, index, entry.parent);
            if path_edits == 0 && (!searchbits::possible(matcher.suffix_bits[reach], index.names.bits(entry.name as usize), 0)
                || matcher.reach(index.names.get(entry.name), reach) != matcher.needle.len()) {
                if let Some(score) = score { hits.push((score, row)); }
                continue;
            }
            let exact_prefix = reach >= matcher.prefix;
            let typo_prefix = if exact_prefix || path_edits == 0 { false } else {
                if scratch.dirs[entry.parent as usize].parent_edits.is_none() {
                    scratch.parent_text(index, entry.parent);
                    let possible = matcher.parent_possible(&scratch.texts[scratch.dirs[entry.parent as usize].text.clone()], &mut scratch.edits);
                    scratch.dirs[entry.parent as usize].parent_edits = Some(possible);
                }
                scratch.dirs[entry.parent as usize].parent_edits.unwrap()
            };
            if !exact_prefix && !typo_prefix {
                if let Some(score) = score { hits.push((score, row)); }
                continue;
            }
            if exact_prefix && matcher.reach(index.names.get(entry.name), reach) == matcher.needle.len() {
                // A path DP hit scores at least NAME - HIT_SPAN, above every typo score, so neither typo arm could change this row.
                if matcher.defer {
                    hits.push((if matcher.needle.contains(&'/') { PENDING_ALIGNED } else { PENDING_PATH }, row));
                    #[cfg(test)]
                    index.paths_scored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    continue;
                }
                scratch.state(matcher, index, entry.parent);
                let parent = &scratch.states[scratch.dirs[entry.parent as usize].state];
                fold(index.names.get(entry.name), &mut scratch.letters);
                let state = scratch.row_state.get_or_insert_with(|| State::new(matcher.needle.len()));
                state.copy_from(parent);
                state.advance(&matcher.needle, &scratch.letters, true);
                #[cfg(test)]
                index.path_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                score = score.max(state.score(if matcher.needle.contains(&'/') { ALIGNED } else { NAME }));
            }
            if path_edits > 0 && !score.is_some_and(|score| score >= ALIGNED - HIT_SPAN) {
                scratch.parent_text(index, entry.parent);
                scratch.path_letters.clear();
                scratch.path_letters.extend_from_slice(&scratch.texts[scratch.dirs[entry.parent as usize].text.clone()]);
                fold(index.names.get(entry.name), &mut scratch.letters);
                scratch.path_letters.extend_from_slice(&scratch.letters);
                score = score.max(matcher.distance(&scratch.path_letters, &mut scratch.edits)
                    .map(|distance| crate::backend::searchmatch::TYPO - distance as i32 * crate::backend::searchmatch::TYPO_EDIT));
            }
            #[cfg(test)]
            index.paths_scored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(score) = score { hits.push((score, row)); }
    }
    scratch.end();
    hits
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
    let (paths, mut kept): (Vec<_>, Vec<_>) = std::mem::take(head).into_iter().partition(|hit| hit.0 == PENDING_PATH);
    resolve(matcher, index, scratch, &mut kept);
    let floor = if paths.is_empty() { None } else { window_floor(&kept, window) };
    let (mut admitted, mut low) = (Vec::new(), Vec::new());
    for hit in paths {
        match floor {
            Some(floor) if path_bound(matcher, index, scratch, hit.1, floor) < floor => low.push(hit),
            _ => admitted.push(hit),
        }
    }
    resolve(matcher, index, scratch, &mut admitted);
    kept.append(&mut admitted);
    if let Some(floor) = window_floor(&kept, window) {
        let (above, mut under): (Vec<_>, Vec<_>) = kept.into_iter().partition(|hit| hit.0 >= floor);
        kept = above;
        low.append(&mut under);
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

// The name bits give a first j for free; only a row they cannot rule out pays a fold for the exact one.
fn path_bound(matcher: &Matcher, index: &Index, scratch: &mut QueryScratch, row: usize, floor: i32) -> i32 {
    let entry = &index.rows[row];
    scratch.state(matcher, index, entry.parent);
    let state = scratch.dirs[entry.parent as usize].state;
    let len = matcher.needle.len();
    let bits = index.names.bits(entry.name as usize);
    let possible = (0..len).find(|&at| searchbits::possible(matcher.suffix_bits[at], bits, 0)).unwrap_or(len);
    let rough = bound(&scratch.states[state], len, possible);
    if rough < floor { return rough; }
    fold(index.names.get(entry.name), &mut scratch.letters);
    // Matching the needle backwards from its end finds the longest suffix the basename holds.
    let mut from = len;
    for letter in scratch.letters.iter().rev() {
        if from > 0 && letter.lower == matcher.needle[from - 1] { from -= 1; }
    }
    bound(&scratch.states[state], len, from)
}

// Exact scores for marked rows by score_cached's DP; on a subsequence match best[] only decays by one a letter, so it always hits.
pub(crate) fn resolve(matcher: &Matcher, index: &Index, scratch: &mut QueryScratch, hits: &mut [(i32, usize)]) {
    let mut pending = 0;
    for hit in hits.iter().filter(|hit| hit.0 < 0) {
        pending += 1;
        let parent = index.rows[hit.1].parent;
        if parent != EMPTY { scratch.state(matcher, index, parent); }
    }
    // Every parent state is ready and only read from here on, so the rows split across the name scorer's threads.
    let threads = scratch.threads(pending);
    let (states, dirs) = (&scratch.states, &scratch.dirs);
    #[cfg(test)]
    let mut largest = 0;
    if threads == 1 {
        resolve_rows(matcher, index, states, dirs, &mut scratch.workers[0], hits);
    } else {
        // Each thread takes an equal share of the pending rows, wherever they sit among the resolved ones.
        let share = pending.div_ceil(threads);
        std::thread::scope(|scope| {
            let mut rest = hits;
            for worker in &mut scratch.workers[..threads] {
                let cut = rest.iter().enumerate().filter(|(_, hit)| hit.0 < 0).nth(share - 1).map_or(rest.len(), |(at, _)| at + 1);
                let (chunk, tail) = std::mem::take(&mut rest).split_at_mut(cut);
                rest = tail;
                #[cfg(test)]
                { largest = largest.max(chunk.iter().filter(|hit| hit.0 < 0).count()); }
                scope.spawn(move || resolve_rows(matcher, index, states, dirs, worker, chunk));
            }
        });
    }
    #[cfg(test)]
    { scratch.counters.resolve_largest_share = scratch.counters.resolve_largest_share.max(largest); }
    scratch.end();
}

fn resolve_rows(matcher: &Matcher, index: &Index, states: &[State], dirs: &[scratch::Directory], worker: &mut scratch::Worker,
    hits: &mut [(i32, usize)]) {
    for hit in hits.iter_mut().filter(|hit| hit.0 < 0) {
        let entry = &index.rows[hit.1];
        let tier = if hit.0 == PENDING_ALIGNED { ALIGNED } else { NAME };
        // Only a row at the scope's top scores on its basename alone; every other match takes the whole-path DP.
        let path = entry.parent != EMPTY;
        fold(index.names.get(entry.name), &mut worker.letters);
        let state = &mut worker.state;
        // resize also resets every cell, which a fresh name DP needs and a path DP gets from copy_from.
        if !path { state.resize(matcher.needle.len()); }
        else { state.copy_from(&states[dirs[entry.parent as usize].state]); }
        state.advance(&matcher.needle, &worker.letters, true);
        let score = state.score(tier);
        debug_assert!(score.is_some(), "a subsequence match has a DP hit");
        hit.0 = score.unwrap_or(tier - HIT_SPAN);
        #[cfg(test)]
        if !path { index.name_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
        else { index.path_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
    }
}
