// Query scratch is keyed by interned names and directory ids, never by reconstructed file paths.
use super::*;
use crate::backend::searchmatch::{fold, separator, Letter, Matcher, State, ALIGNED, NAME, PATH};
use crate::backend::{searchbits, searchtypo};

fn names(matcher: &Matcher, index: &Index, used: &[bool], scores: &mut [Option<i32>], offset: usize) {
    let mut letters = Vec::new();
    let mut state = State::new(matcher.needle.len());
    let mut edits = searchtypo::Scratch::default();
    #[cfg(test)]
    let mut scored = 0;
    for (group, scores) in scores.chunks_mut(64).enumerate() {
        let first = offset + group * 64;
        if !searchbits::possible(matcher.bits, index.names.group_bits(first / 64), matcher.budget) { continue; }
        for (at, score) in scores.iter_mut().enumerate() {
            let id = first + at;
            if !used[group * 64 + at] || !searchbits::possible(matcher.bits, index.names.bits(id), matcher.budget) { continue; }
            let name = index.names.get(id as u32);
            if matcher.needle.len() > name.len().saturating_add(matcher.budget) { continue; }
            #[cfg(test)]
            { scored += 1; }
            *score = matcher.name_bytes(name, &mut letters, &mut state, &mut edits);
        }
    }
    #[cfg(test)]
    index.names_scored.fetch_add(scored, std::sync::atomic::Ordering::Relaxed);
}

fn directory(matcher: &Matcher, index: &Index, dir: u32, states: &mut [Option<State>], pending: &mut Vec<u32>, letters: &mut Vec<Letter>) {
    pending.clear();
    let mut at = dir;
    while at != EMPTY && states[at as usize].is_none() {
        pending.push(at);
        at = index.blocks.record(at).1;
    }
    while let Some(dir) = pending.pop() {
        let (name, parent) = index.blocks.record(dir);
        let mut state = if parent == EMPTY { State::new(matcher.needle.len()) } else { states[parent as usize].as_ref().unwrap().clone() };
        fold(index.names.get(name), letters);
        state.advance(&matcher.needle, letters, false);
        let boundary = letters.last().is_none_or(|letter| separator(letter.lower));
        state.advance(&matcher.needle, &[Letter { lower: '/', boundary }], false);
        states[dir as usize] = Some(state);
        #[cfg(test)]
        index.dirs_scored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn parent_text<'a>(index: &Index, dir: u32, cache: &'a mut HashMap<u32, Vec<Letter>>, pending: &mut Vec<u32>, letters: &mut Vec<Letter>) -> &'a Vec<Letter> {
    cache.entry(dir).or_insert_with(|| {
        index.blocks.ancestors(dir, pending);
        let mut text = Vec::new();
        for &name in pending.iter().rev() {
            fold(index.names.get(name), letters);
            let boundary = letters.last().is_none_or(|letter| separator(letter.lower));
            text.extend_from_slice(letters);
            text.push(Letter { lower: '/', boundary });
        }
        text
    })
}

pub(crate) fn score_all(matcher: &Matcher, rows: &[usize], index: &Index) -> Vec<(i32, usize)> {
    #[cfg(test)]
    for counter in [&index.names_scored, &index.dirs_scored, &index.paths_scored] {
        counter.store(0, std::sync::atomic::Ordering::Relaxed);
    }
    let mut used = vec![false; index.names.len()];
    for &row in rows { used[index.rows[row].name as usize] = true; }
    let mut scores = vec![None; used.len()];
    if used.len() < PARALLEL_SCORE_AT {
        names(matcher, index, &used, &mut scores, 0);
    } else {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, WALK_THREADS);
        let chunk = used.len().div_ceil(64 * threads) * 64;
        std::thread::scope(|scope| {
            for (part, (scores, used)) in scores.chunks_mut(chunk).zip(used.chunks(chunk)).enumerate() {
                scope.spawn(move || names(matcher, index, used, scores, part * chunk));
            }
        });
    }
    let mut dir_bits = Vec::with_capacity(index.blocks.len());
    let mut dir_reach = Vec::with_capacity(index.blocks.len());
    for id in 0..index.blocks.len() {
        let (name, parent) = index.blocks.record(id as u32);
        let bits = index.names.bits(name as usize) | searchbits::mask(b"/")
            | if parent == EMPTY { 0 } else { dir_bits[parent as usize] };
        dir_bits.push(bits);
        let before = if parent == EMPTY { 0 } else { dir_reach[parent as usize] };
        let reach = matcher.reach(index.names.get(name), before);
        dir_reach.push(reach + usize::from(matcher.needle.get(reach) == Some(&'/')));
    }
    let mut states = vec![None; index.blocks.len()];
    let mut state = State::new(matcher.needle.len());
    let mut pending = Vec::new();
    let mut letters = Vec::new();
    let mut text_cache = HashMap::<u32, Vec<Letter>>::new();
    let mut parent_edits = vec![None; index.blocks.len()];
    let mut path_letters = Vec::new();
    let mut edits = searchtypo::Scratch::default();
    let path_edits = if matcher.needle.contains(&'/') { matcher.budget } else { 0 };
    let mut hits = Vec::new();
    let mut first = [0; 4];
    let first_bits = matcher.needle.first().map_or(0, |c| searchbits::mask(c.encode_utf8(&mut first).as_bytes()));
    for &row in rows {
        let entry = &index.rows[row];
        let mut score = scores[entry.name as usize];
        let path_limit = index.blocks.text_len(entry.parent) as usize + usize::from(entry.parent != EMPTY)
            + index.names.get(entry.name).len() + path_edits;
        if !matcher.needle.is_empty() && !score.is_some_and(|score| score >= NAME) && entry.parent != EMPTY
            && matcher.needle.len() <= path_limit
            && searchbits::possible(first_bits, dir_bits[entry.parent as usize], path_edits)
            && searchbits::possible(matcher.parent_bits, dir_bits[entry.parent as usize], path_edits)
            && searchbits::possible(matcher.bits, dir_bits[entry.parent as usize] | index.names.bits(entry.name as usize), path_edits) {
            let reach = dir_reach[entry.parent as usize];
            if path_edits == 0 && (!searchbits::possible(matcher.suffix_bits[reach], index.names.bits(entry.name as usize), 0)
                || matcher.reach(index.names.get(entry.name), reach) != matcher.needle.len()) {
                if let Some(score) = score { hits.push((score, row)); }
                continue;
            }
            let exact_prefix = reach >= matcher.prefix;
            let typo_prefix = if exact_prefix || path_edits == 0 { false } else {
                *parent_edits[entry.parent as usize].get_or_insert_with(|| {
                    let parent = parent_text(index, entry.parent, &mut text_cache, &mut pending, &mut letters);
                    matcher.parent_possible(parent, &mut edits)
                })
            };
            if !exact_prefix && !typo_prefix {
                if let Some(score) = score { hits.push((score, row)); }
                continue;
            }
            if exact_prefix && matcher.reach(index.names.get(entry.name), reach) == matcher.needle.len() {
                directory(matcher, index, entry.parent, &mut states, &mut pending, &mut letters);
                let parent = states[entry.parent as usize].as_ref().unwrap();
                fold(index.names.get(entry.name), &mut letters);
                state.clone_from(parent);
                state.advance(&matcher.needle, &letters, true);
                score = score.max(state.score(if matcher.needle.contains(&'/') { ALIGNED } else { PATH }));
            }
            if path_edits > 0 && !score.is_some_and(|score| score >= PATH) {
                let parent = parent_text(index, entry.parent, &mut text_cache, &mut pending, &mut letters);
                path_letters.clear();
                path_letters.extend_from_slice(parent);
                fold(index.names.get(entry.name), &mut letters);
                path_letters.extend_from_slice(&letters);
                score = score.max(matcher.distance(&path_letters, &mut edits)
                    .map(|distance| crate::backend::searchmatch::TYPO - distance as i32 * 1000));
            }
            #[cfg(test)]
            index.paths_scored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(score) = score { hits.push((score, row)); }
    }
    hits
}
