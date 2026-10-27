// Scoring visits candidate name ids and their parents; reusable storage belongs to the session.
use super::*;
use crate::backend::searchmatch::{fold, Matcher, State, ALIGNED, EXACT, HIT_SPAN, NAME, PATH, PENDING_ALIGNED, PENDING_NAME, PENDING_PATH, PREFIX, WORD};
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
        if !matcher.needle.is_empty() && !score.is_some_and(|score| score >= NAME || score == PENDING_NAME) && entry.parent != EMPTY
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
                // A path DP hit scores at least PATH - HIT_SPAN, above every typo score, so neither typo arm could change this row.
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
                score = score.max(state.score(if matcher.needle.contains(&'/') { ALIGNED } else { PATH }));
            }
            if path_edits > 0 && !score.is_some_and(|score| score >= PATH) {
                scratch.parent_text(index, entry.parent);
                scratch.path_letters.clear();
                scratch.path_letters.extend_from_slice(&scratch.texts[scratch.dirs[entry.parent as usize].text.clone()]);
                fold(index.names.get(entry.name), &mut scratch.letters);
                scratch.path_letters.extend_from_slice(&scratch.letters);
                score = score.max(matcher.distance(&scratch.path_letters, &mut scratch.edits)
                    .map(|distance| crate::backend::searchmatch::TYPO - distance as i32 * 1000));
            }
            #[cfg(test)]
            index.paths_scored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(score) = score { hits.push((score, row)); }
    }
    scratch.end();
    hits
}

pub(crate) const CLASSES: usize = 7;

// The class of a pass-1 score, best first; no prior bonus lets a row outrank a row of an earlier class.
pub(crate) fn class(score: i32) -> usize {
    match score {
        PENDING_ALIGNED => class(ALIGNED),
        PENDING_NAME => class(NAME),
        PENDING_PATH => class(PATH),
        _ => [EXACT, PREFIX, WORD, ALIGNED - HIT_SPAN, NAME - HIT_SPAN, PATH - HIT_SPAN].iter()
            .position(|&floor| score >= floor).unwrap_or(CLASSES - 1),
    }
}

// The last class the best `window` hits reach; None when the window is empty.
pub(crate) fn boundary(hits: &[(i32, usize)], window: usize) -> Option<usize> {
    let window = window.min(hits.len());
    if window == 0 { return None; }
    let mut counts = [0usize; CLASSES];
    for hit in hits { counts[class(hit.0)] += 1; }
    let mut reached = 0;
    counts.iter().position(|&count| { reached += count; reached >= window })
}

// Exact scores for marked rows by score_cached's DP; on a subsequence match best[] only decays by one a letter, so it always hits.
pub(crate) fn resolve(matcher: &Matcher, index: &Index, scratch: &mut QueryScratch, hits: &mut [(i32, usize)]) {
    for hit in hits.iter_mut().filter(|hit| hit.0 < 0) {
        let entry = &index.rows[hit.1];
        let tier = match hit.0 { PENDING_NAME => NAME, PENDING_ALIGNED => ALIGNED, _ => PATH };
        if tier != NAME { scratch.state(matcher, index, entry.parent); }
        fold(index.names.get(entry.name), &mut scratch.letters);
        let state = scratch.row_state.get_or_insert_with(|| State::new(matcher.needle.len()));
        // resize also resets every cell, which a fresh name DP needs and a path DP gets from copy_from.
        if tier == NAME { state.resize(matcher.needle.len()); }
        else { state.copy_from(&scratch.states[scratch.dirs[entry.parent as usize].state]); }
        state.advance(&matcher.needle, &scratch.letters, true);
        let score = state.score(tier);
        debug_assert!(score.is_some(), "a subsequence match has a DP hit");
        hit.0 = score.unwrap_or(tier - HIT_SPAN);
        #[cfg(test)]
        if tier == NAME { index.name_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
        else { index.path_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
    }
    scratch.end();
}
