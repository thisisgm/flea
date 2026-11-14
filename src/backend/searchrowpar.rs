// Parallel row passes: each thread keeps a one-directory parent cache, and the session's shared caches stay with the calling thread.
use super::*;

// Rows a partition keeps, then the rest.
type Split = (Vec<(i32, usize)>, Vec<(i32, usize)>);

impl QueryScratch {
    // Each thread takes one contiguous run of rows, so its one-parent cache holds through every row of a directory.
    pub(in super::super) fn rows_parallel<F>(&mut self, index: &Index, rows: &[usize], threads: usize, body: F) -> Vec<(i32, usize)>
    where F: Fn(&mut ParentCache, &mut RowTemps, usize, Option<i32>) -> Option<i32> + Sync {
        let chunk = rows.len().div_ceil(threads);
        if self.row_workers.len() < threads { self.row_workers.resize_with(threads, Default::default); }
        let (names, body) = (&self.names, &body);
        let mut parts: Vec<Vec<(i32, usize)>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = rows.chunks(chunk).zip(&mut self.row_workers).map(|(rows, (cache, temps))| scope.spawn(move || {
                cache.dir = None;
                let mut hits = Vec::with_capacity(rows.len());
                for &row in rows {
                    let score = names[index.rows[row].name as usize].score;
                    if let Some(score) = body(cache, temps, row, (score > TYPO_LATER).then_some(score)) { hits.push((score, row)); }
                }
                temps.flush(index);
                hits
            })).collect();
            jobs.into_iter().map(|job| job.join().unwrap()).collect()
        });
        // The first run's buffer grows in place to hold the rest, so only the later runs are copied.
        let total: usize = parts.iter().map(Vec::len).sum();
        let mut hits = std::mem::take(&mut parts[0]);
        hits.reserve_exact(total - hits.len());
        for part in &parts[1..] { hits.extend_from_slice(part); }
        hits
    }

    fn row_workers(&mut self, threads: usize) -> &mut [(ParentCache, RowTemps)] {
        if self.row_workers.len() < threads { self.row_workers.resize_with(threads, Default::default); }
        &mut self.row_workers[..threads]
    }

    // Each thread takes an equal share of the pending rows, wherever they sit among the resolved ones.
    pub(in super::super) fn resolve_parallel<F>(&mut self, index: &Index, hits: &mut [(i32, usize)], threads: usize, pending: usize, body: F)
    where F: Fn(&mut ParentCache, &mut RowTemps, &mut (i32, usize)) + Sync {
        let share = pending.div_ceil(threads);
        let body = &body;
        #[cfg(test)]
        let mut largest = 0;
        let workers = self.row_workers(threads);
        std::thread::scope(|scope| {
            let mut rest = hits;
            for (cache, temps) in workers {
                let cut = rest.iter().enumerate().filter(|(_, hit)| hit.0 < 0).nth(share - 1).map_or(rest.len(), |(at, _)| at + 1);
                let (chunk, tail) = std::mem::take(&mut rest).split_at_mut(cut);
                rest = tail;
                #[cfg(test)]
                { largest = largest.max(chunk.iter().filter(|hit| hit.0 < 0).count()); }
                scope.spawn(move || {
                    cache.dir = None;
                    for hit in chunk.iter_mut().filter(|hit| hit.0 < 0) { body(cache, temps, hit); }
                    temps.flush(index);
                });
            }
        });
        #[cfg(test)]
        { self.counters.resolve_largest_share = self.counters.resolve_largest_share.max(largest); }
    }

    // Splits hits into those the test keeps and the rest, each side in its original order, one contiguous run per thread.
    pub(in super::super) fn partition_parallel<F>(&mut self, hits: Vec<(i32, usize)>, threads: usize, keep: F) -> Split
    where F: Fn(&mut ParentCache, &mut RowTemps, usize) -> bool + Sync {
        let chunk = hits.len().div_ceil(threads);
        let (keep, workers) = (&keep, self.row_workers(threads));
        let parts: Vec<Split> = std::thread::scope(|scope| {
            let jobs: Vec<_> = hits.chunks(chunk).zip(workers).map(|(hits, (cache, temps))| scope.spawn(move || {
                cache.dir = None;
                hits.iter().copied().partition::<Vec<_>, _>(|hit| keep(cache, temps, hit.1))
            })).collect();
            jobs.into_iter().map(|job| job.join().unwrap()).collect()
        });
        let (mut kept, mut dropped) = (Vec::with_capacity(parts.iter().map(|part| part.0.len()).sum()), Vec::with_capacity(parts.iter().map(|part| part.1.len()).sum()));
        for (yes, no) in parts {
            kept.extend(yes);
            dropped.extend(no);
        }
        (kept, dropped)
    }
}

impl Parents for QueryScratch {
    fn parent_reach(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> usize { self.reach(matcher, index, dir) }

    fn parent_typo(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> bool {
        if let Some(possible) = self.directory(dir).parent_edits { return possible; }
        self.parent_text(index, dir);
        let possible = matcher.parent_possible(&self.texts[self.dirs[dir as usize].text.clone()], &mut self.edits);
        self.dirs[dir as usize].parent_edits = Some(possible);
        possible
    }

    fn parent_state(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> &State {
        self.state(matcher, index, dir);
        &self.states[self.dirs[dir as usize].state]
    }

    fn parent_letters(&mut self, index: &Index, dir: u32) -> &[Letter] {
        self.parent_text(index, dir);
        &self.texts[self.dirs[dir as usize].text.clone()]
    }
}

// One thread's view of the directory its rows are in: the session's shared caches stay with the calling thread.
#[derive(Default)]
pub(in super::super) struct ParentCache {
    dir: Option<u32>,
    names: Vec<u32>,
    reach: Option<usize>,
    typo: Option<bool>,
    text: Vec<Letter>,
    text_ready: bool,
    state: Option<State>,
    state_ready: bool,
    letters: Vec<Letter>,
    edits: searchtypo::Scratch,
}

impl ParentCache {
    // The directory's names, root last, read once per directory and only when a check needs them.
    fn at(&mut self, index: &Index, dir: u32) {
        if self.dir == Some(dir) { return; }
        self.dir = Some(dir);
        index.blocks.ancestors(dir, &mut self.names);
        self.reach = None;
        self.typo = None;
        self.text_ready = false;
        self.state_ready = false;
    }
}

impl Parents for ParentCache {
    fn parent_reach(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> usize {
        self.at(index, dir);
        if let Some(reach) = self.reach { return reach; }
        let mut reach = 0;
        for &name in self.names.iter().rev() {
            reach = matcher.reach(index.names.get(name), reach);
            reach += usize::from(matcher.needle.get(reach) == Some(&'/'));
        }
        self.reach = Some(reach);
        reach
    }

    fn parent_typo(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> bool {
        self.at(index, dir);
        if let Some(possible) = self.typo { return possible; }
        self.parent_letters(index, dir);
        let possible = matcher.parent_possible(&self.text, &mut self.edits);
        self.typo = Some(possible);
        possible
    }

    fn parent_state(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> &State {
        self.at(index, dir);
        let state = self.state.get_or_insert_with(|| State::new(matcher.needle.len()));
        if !self.state_ready {
            state.resize(matcher.needle.len());
            for &name in self.names.iter().rev() {
                let bytes = index.names.get(name);
                matcher.advance(state, bytes, &mut self.letters, false);
                // The same boundary rule as QueryScratch::state, so both arms build the same column.
                let boundary = if bytes.is_ascii() { bytes.last().is_none_or(|&byte| separator(byte as char)) }
                    else { self.letters.last().is_none_or(|letter| separator(letter.lower)) };
                state.advance(&matcher.needle, &[Letter { lower: '/', boundary }], false);
            }
            self.state_ready = true;
        }
        state
    }

    fn parent_letters(&mut self, index: &Index, dir: u32) -> &[Letter] {
        self.at(index, dir);
        if !self.text_ready {
            self.text.clear();
            for &name in self.names.iter().rev() {
                fold(index.names.get(name), &mut self.letters);
                let boundary = self.letters.last().is_none_or(|letter| separator(letter.lower));
                self.text.extend_from_slice(&self.letters);
                self.text.push(Letter { lower: '/', boundary });
            }
            self.text_ready = true;
        }
        &self.text
    }
}
