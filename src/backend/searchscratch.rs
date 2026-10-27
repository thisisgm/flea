// Query generations invalidate logical slots without clearing the index-sized backing storage.
use super::super::{Index, EMPTY, PARALLEL_SCORE_AT, WALK_THREADS};
use crate::backend::searchmatch::{fold, separator, Letter, Matcher, State};
#[cfg(test)]
use crate::backend::searchmatch::{HIT_SPAN, NAME};
use crate::backend::searchtypo;

pub(super) const NO_SCORE: i32 = i32::MIN;
// Generation storage holding more than this multiple of its last use shrinks back to that use.
const SHRINK_ABOVE: usize = 2;

#[derive(Default, Clone, Copy)]
struct NameSlot {
    generation: u32,
    score: i32,
}

#[derive(Default)]
pub(super) struct Directory {
    generation: u32,
    pub reach: Option<usize>,
    pub state: usize,
    state_ready: bool,
    pub text: std::ops::Range<usize>,
    text_ready: bool,
    pub parent_edits: Option<bool>,
}

pub(super) struct Worker {
    pub(super) letters: Vec<Letter>,
    pub(super) state: State,
    edits: searchtypo::Scratch,
}

impl Worker {
    fn new() -> Self {
        Self { letters: Vec::new(), state: State::new(0), edits: searchtypo::Scratch::default() }
    }

    fn score(&mut self, matcher: &Matcher, index: &Index, jobs: &mut [(u32, i32)]) {
        self.state.resize(matcher.needle.len());
        for (id, score) in jobs {
            *score = matcher.name_bytes(index.names.get(*id), &mut self.letters, &mut self.state, &mut self.edits).unwrap_or(NO_SCORE);
            #[cfg(test)]
            if (NAME - HIT_SPAN..=NAME + HIT_SPAN).contains(&*score) {
                index.name_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

// Index-sized slots and the job list stay for the session, so a keystroke allocates only its output.
#[derive(Default)]
pub(crate) struct QueryScratch {
    generation: u32,
    names: Vec<NameSlot>,
    jobs: Vec<(u32, i32)>,
    pub(super) workers: Vec<Worker>,
    pub(super) dirs: Vec<Directory>,
    pub(super) texts: Vec<Letter>,
    pub(super) states: Vec<State>,
    states_used: usize,
    pending: Vec<u32>,
    pub(super) letters: Vec<Letter>,
    pub(super) path_letters: Vec<Letter>,
    pub(super) edits: searchtypo::Scratch,
    pub(super) row_state: Option<State>,
    #[cfg(test)]
    pub(crate) pinned_cores: Option<usize>,
    #[cfg(test)]
    pub(crate) counters: Counters,
}

#[cfg(test)]
#[derive(Default, Debug)]
pub(crate) struct Counters {
    pub names_scored: usize,
    pub directory_reach: usize,
    pub scratch_touched: usize,
    pub resolve_largest_share: usize,
}

impl QueryScratch {
    pub(crate) fn begin(&mut self, index: &Index) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.names.iter_mut().for_each(|slot| slot.generation = 0);
            self.dirs.iter_mut().for_each(|slot| slot.generation = 0);
            self.generation = 1;
        }
        self.names.resize(index.names.len(), NameSlot::default());
        self.dirs.resize_with(index.blocks.len(), Directory::default);
        self.jobs.clear();
        self.texts.clear();
        self.states_used = 0;
        #[cfg(test)]
        { self.counters = Counters::default(); }
    }

    pub(crate) fn end(&mut self) {
        if self.texts.capacity() > SHRINK_ABOVE * self.texts.len() { self.texts.shrink_to(self.texts.len()); }
        if self.states.len() > SHRINK_ABOVE * self.states_used {
            self.states.truncate(self.states_used);
            self.states.shrink_to(self.states_used);
        }
    }

    pub(super) fn names(&mut self, matcher: &Matcher, rows: &[usize], index: &Index) {
        for &row in rows {
            let id = index.rows[row].name;
            let slot = &mut self.names[id as usize];
            if slot.generation == self.generation { continue; }
            *slot = NameSlot { generation: self.generation, score: NO_SCORE };
            #[cfg(test)]
            { self.counters.scratch_touched += 1; }
            if super::searchbits::possible(matcher.bits, index.names.group_bits(id), matcher.budget)
                && super::searchbits::possible(matcher.bits, index.names.bits(id as usize), matcher.budget)
                && matcher.needle.len() <= index.names.get(id).len().saturating_add(matcher.budget) {
                self.jobs.push((id, NO_SCORE));
            }
        }
        if self.jobs.is_empty() { return; }
        let threads = self.threads(self.jobs.len());
        if threads == 1 {
            self.workers[0].score(matcher, index, &mut self.jobs);
        } else {
            let chunk = self.jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                for (jobs, worker) in self.jobs.chunks_mut(chunk).zip(&mut self.workers) {
                    scope.spawn(move || worker.score(matcher, index, jobs));
                }
            });
        }
        for &(id, score) in &self.jobs { self.names[id as usize].score = score; }
        #[cfg(test)]
        {
            self.counters.names_scored = self.jobs.len();
            index.names_scored.store(self.jobs.len(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    // One thread under PARALLEL_SCORE_AT jobs, else the machine's cores up to WALK_THREADS, each with its Worker ready.
    pub(super) fn threads(&mut self, jobs: usize) -> usize {
        let threads = if jobs < PARALLEL_SCORE_AT { 1 } else { self.cores().clamp(1, WALK_THREADS) };
        if self.workers.len() < threads { self.workers.resize_with(threads, Worker::new); }
        threads
    }

    // A test may pin the core count, so the split arm runs on a one-core builder too.
    fn cores(&self) -> usize {
        #[cfg(test)]
        if let Some(cores) = self.pinned_cores { return cores; }
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }

    pub(super) fn name(&self, id: u32) -> Option<i32> {
        let score = self.names[id as usize].score;
        (score != NO_SCORE).then_some(score)
    }

    fn directory(&mut self, id: u32) -> &mut Directory {
        let slot = &mut self.dirs[id as usize];
        if slot.generation != self.generation {
            slot.generation = self.generation;
            slot.reach = None;
            slot.state_ready = false;
            slot.text_ready = false;
            slot.parent_edits = None;
            #[cfg(test)]
            { self.counters.scratch_touched += 1; }
        }
        slot
    }

    pub(super) fn reach(&mut self, matcher: &Matcher, index: &Index, dir: u32) -> usize {
        self.pending.clear();
        let mut at = dir;
        while at != EMPTY && self.directory(at).reach.is_none() {
            self.pending.push(at);
            at = index.blocks.record(at).1;
        }
        while let Some(id) = self.pending.pop() {
            let (name, parent) = index.blocks.record(id);
            let before = if parent == EMPTY { 0 } else { self.dirs[parent as usize].reach.unwrap() };
            let reach = matcher.reach(index.names.get(name), before);
            self.dirs[id as usize].reach = Some(reach + usize::from(matcher.needle.get(reach) == Some(&'/')));
            #[cfg(test)]
            { self.counters.directory_reach += 1; }
        }
        self.dirs[dir as usize].reach.unwrap()
    }

    pub(super) fn state(&mut self, matcher: &Matcher, index: &Index, dir: u32) {
        self.pending.clear();
        let mut at = dir;
        while at != EMPTY && !self.directory(at).state_ready {
            self.pending.push(at);
            at = index.blocks.record(at).1;
        }
        while let Some(id) = self.pending.pop() {
            let (name, parent) = index.blocks.record(id);
            let slot = self.states_used;
            if slot == self.states.len() { self.states.push(State::new(matcher.needle.len())); }
            self.states_used += 1;
            // A ready parent took its slot earlier in this generation, so it sits below this one.
            let (ready, rest) = self.states.split_at_mut(slot);
            let state = &mut rest[0];
            if parent == EMPTY { state.resize(matcher.needle.len()); }
            else { state.copy_from(&ready[self.dirs[parent as usize].state]); }
            fold(index.names.get(name), &mut self.letters);
            state.advance(&matcher.needle, &self.letters, false);
            let boundary = self.letters.last().map_or(true, |letter| separator(letter.lower));
            state.advance(&matcher.needle, &[Letter { lower: '/', boundary }], false);
            self.dirs[id as usize].state = slot;
            self.dirs[id as usize].state_ready = true;
            #[cfg(test)]
            index.dirs_scored.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub(super) fn parent_text(&mut self, index: &Index, dir: u32) {
        if self.directory(dir).text_ready { return; }
        index.blocks.ancestors(dir, &mut self.pending);
        let start = self.texts.len();
        for &name in self.pending.iter().rev() {
            fold(index.names.get(name), &mut self.letters);
            let boundary = self.letters.last().map_or(true, |letter| separator(letter.lower));
            self.texts.extend_from_slice(&self.letters);
            self.texts.push(Letter { lower: '/', boundary });
        }
        self.dirs[dir as usize].text = start..self.texts.len();
        self.dirs[dir as usize].text_ready = true;
    }
}

#[cfg(test)]
#[path = "searchscratch_tests.rs"]
mod tests;
