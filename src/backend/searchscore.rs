// Scoring visits candidate name ids and their parents; reusable storage belongs to the session.
use super::*;
use crate::backend::searchmatch::{fold, Matcher, State, ALIGNED, NAME, PATH};
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
        if !matcher.needle.is_empty() && !score.is_some_and(|score| score >= NAME) && entry.parent != EMPTY
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
                scratch.state(matcher, index, entry.parent);
                let parent = &scratch.states[scratch.dirs[entry.parent as usize].state];
                fold(index.names.get(entry.name), &mut scratch.letters);
                let state = scratch.row_state.get_or_insert_with(|| State::new(matcher.needle.len()));
                state.copy_from(parent);
                state.advance(&matcher.needle, &scratch.letters, true);
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
