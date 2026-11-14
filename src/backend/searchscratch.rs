// Query generations invalidate logical slots without clearing the index-sized backing storage.
use super::super::{Index, EMPTY, PARALLEL_SCORE_AT};
use super::{Parents, RowTemps};
use crate::backend::searchmatch::ascii::{Bits, Own};
use crate::backend::searchmatch::{fold, separator, Letter, Matcher, State, PENDING_NAME};
#[cfg(test)]
use crate::backend::searchmatch::{HIT_SPAN, NAME};
use crate::backend::searchmatch::WORD;
use crate::backend::searchtypo;

pub(super) const NO_SCORE: i32 = i32::MIN;
// A name only an edit could match, scored only once a query shows its typo rows.
const TYPO_LATER: i32 = i32::MIN + 1;
// A query's CPU work splits across up to this many cores; the walk keeps its own, smaller, I/O-bound count.
const QUERY_THREADS: usize = 16;
// Generation storage holding more than this multiple of its last use shrinks back to that use.
const SHRINK_ABOVE: usize = 2;

// A PENDING_NAME slot's own is its basename-only score and lift where the name pass read them, else Own::NONE.
#[derive(Clone, Copy)]
struct NameSlot {
    generation: u32,
    score: i32,
    own: Own,
}

impl Default for NameSlot {
    fn default() -> Self { Self { generation: 0, score: NO_SCORE, own: Own::NONE } }
}

// The name slots resolve reads a PENDING_NAME row's own score from, lent while the scratch serves as its parent cache.
pub(super) struct Owned {
    names: Vec<NameSlot>,
    generation: u32,
}

impl Owned {
    pub(super) fn get(&self, id: u32) -> Option<Own> {
        let slot = self.names.get(id as usize)?;
        (slot.generation == self.generation && slot.score == PENDING_NAME && !slot.own.is_none()).then_some(slot.own)
    }
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
    bits: Box<Bits>,
}

impl Worker {
    fn new() -> Self {
        Self { letters: Vec::new(), state: State::new(0), edits: searchtypo::Scratch::default(), bits: Box::new(Bits::new()) }
    }

    fn score(&mut self, matcher: &Matcher, strict: Option<&Matcher>, index: &Index, jobs: &mut [(u32, i32, Own)]) {
        self.state.resize(matcher.needle.len());
        for (id, score, own) in jobs { (*score, *own) = self.score_split(matcher, strict, index, *id); }
    }

    // With a strict matcher, a name it cannot score waits as TYPO_LATER; the caller checked it against the full one.
    fn score_split(&mut self, matcher: &Matcher, strict: Option<&Matcher>, index: &Index, id: u32) -> (i32, Own) {
        let Some(strict) = strict else { return self.score_one(matcher, index, id) };
        let (score, own) = if eligible(strict, index, id) { self.score_one(strict, index, id) } else { (NO_SCORE, Own::NONE) };
        if score == NO_SCORE { (TYPO_LATER, own) } else { (score, own) }
    }

    fn score_one(&mut self, matcher: &Matcher, index: &Index, id: u32) -> (i32, Own) {
        let (score, own) = matcher.name_own(index.names.get(id), &mut self.letters, &mut self.state, &mut self.edits, &mut self.bits);
        let (score, own) = (score.unwrap_or(NO_SCORE), own.unwrap_or(Own::NONE));
        // The DP budget counts a scored NAME row; a deferred name's own score comes with its class read.
        #[cfg(test)]
        if (NAME - HIT_SPAN..=NAME + HIT_SPAN).contains(&score) {
            index.name_dp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        (score, own)
    }

    // A whole-index pass scores its share of the name table by id, so no row is visited to find its names.
    fn score_slots(&mut self, matcher: &Matcher, strict: Option<&Matcher>, index: &Index, first: usize, slots: &mut [NameSlot], generation: u32)
        -> (usize, usize) {
        self.state.resize(matcher.needle.len());
        let (mut scored, mut real) = (0, 0);
        for (id, slot) in (first as u32..).zip(slots) {
            *slot = NameSlot { generation, ..NameSlot::default() };
            if eligible(matcher, index, id) {
                (slot.score, slot.own) = self.score_split(matcher, strict, index, id);
                scored += 1;
                real += usize::from(slot.score >= WORD);
            }
        }
        (scored, real)
    }

    // The typo scores a strict pass left for later: only the typo arm is left to run on them.
    fn fill_slots(&mut self, matcher: &Matcher, index: &Index, first: usize, slots: &mut [NameSlot]) {
        for (id, slot) in (first as u32..).zip(slots) {
            if slot.score == TYPO_LATER { slot.score = self.score_typo(matcher, index, id); }
        }
    }

    fn fill(&mut self, matcher: &Matcher, index: &Index, jobs: &mut [(u32, i32, Own)]) {
        for (id, score, _) in jobs { *score = self.score_typo(matcher, index, *id); }
    }

    fn score_typo(&mut self, matcher: &Matcher, index: &Index, id: u32) -> i32 {
        matcher.typo_bytes(index.names.get(id), &mut self.letters, &mut self.edits).unwrap_or(NO_SCORE)
    }
}

// The bit and length gates a name passes before its scorer runs.
fn eligible(matcher: &Matcher, index: &Index, id: u32) -> bool {
    super::searchbits::possible(matcher.bits, index.names.group_bits(id), matcher.budget)
        && super::searchbits::possible(matcher.bits, index.names.bits(id as usize), matcher.budget)
        && matcher.needle.len() <= index.names.get(id).len().saturating_add(matcher.budget)
}

// Index-sized slots and the job list stay for the session, so a keystroke allocates only its output.
#[derive(Default)]
pub(crate) struct QueryScratch {
    generation: u32,
    names: Vec<NameSlot>,
    jobs: Vec<(u32, i32, Own)>,
    // The last name pass scored the whole name table rather than the jobs list.
    whole: bool,
    pub(super) workers: Vec<Worker>,
    pub(super) dirs: Vec<Directory>,
    pub(super) texts: Vec<Letter>,
    pub(super) states: Vec<State>,
    states_used: usize,
    pending: Vec<u32>,
    pub(super) letters: Vec<Letter>,
    edits: searchtypo::Scratch,
    pub(super) temps: RowTemps,
    row_workers: Vec<(ParentCache, RowTemps)>,
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

    // A strict matcher leaves names only an edit could match to fill_typos; returns how many hold the query exactly, as prefix or word start.
    pub(super) fn names(&mut self, matcher: &Matcher, strict: Option<&Matcher>, rows: &[usize], index: &Index) -> usize {
        // Distinct candidates as many as the rows are every row, so every name a row holds is in the table.
        self.whole = rows.len() == index.len() && rows.len() >= PARALLEL_SCORE_AT;
        if self.whole { return self.all_names(matcher, strict, index); }
        for &row in rows {
            let id = index.rows[row].name;
            let slot = &mut self.names[id as usize];
            if slot.generation == self.generation { continue; }
            *slot = NameSlot { generation: self.generation, ..NameSlot::default() };
            #[cfg(test)]
            { self.counters.scratch_touched += 1; }
            if eligible(matcher, index, id) { self.jobs.push((id, NO_SCORE, Own::NONE)); }
        }
        self.score_jobs(matcher, strict, false, index);
        #[cfg(test)]
        {
            self.counters.names_scored = self.jobs.len();
            index.names_scored.store(self.jobs.len(), std::sync::atomic::Ordering::Relaxed);
        }
        self.jobs.iter().filter(|job| job.1 >= WORD).count()
    }

    // Scores the jobs list: whole names, strict ones, or with fill only the typo arm of names a strict pass left.
    fn score_jobs(&mut self, matcher: &Matcher, strict: Option<&Matcher>, fill: bool, index: &Index) {
        if self.jobs.is_empty() { return; }
        let threads = self.threads(self.jobs.len());
        let work = move |worker: &mut Worker, jobs: &mut [(u32, i32, Own)]| if fill { worker.fill(matcher, index, jobs) } else { worker.score(matcher, strict, index, jobs) };
        if threads == 1 {
            work(&mut self.workers[0], &mut self.jobs);
        } else {
            let chunk = self.jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                for (jobs, worker) in self.jobs.chunks_mut(chunk).zip(&mut self.workers) {
                    scope.spawn(move || work(worker, jobs));
                }
            });
        }
        for &(id, score, own) in &self.jobs { (self.names[id as usize].score, self.names[id as usize].own) = (score, own); }
    }

    fn all_names(&mut self, matcher: &Matcher, strict: Option<&Matcher>, index: &Index) -> usize {
        let (count, generation) = (self.names.len(), self.generation);
        let threads = self.threads(count);
        let chunk = count.div_ceil(threads);
        let (scored, real) = if threads == 1 {
            self.workers[0].score_slots(matcher, strict, index, 0, &mut self.names, generation)
        } else {
            std::thread::scope(|scope| {
                let jobs: Vec<_> = self.names.chunks_mut(chunk).zip(&mut self.workers).enumerate()
                    .map(|(at, (slots, worker))| scope.spawn(move || worker.score_slots(matcher, strict, index, at * chunk, slots, generation)))
                    .collect();
                jobs.into_iter().map(|job| job.join().unwrap()).fold((0, 0), |(scored, real), (s, r)| (scored + s, real + r))
            })
        };
        #[cfg(test)]
        {
            self.counters.names_scored = scored;
            self.counters.scratch_touched += count;
            index.names_scored.store(scored, std::sync::atomic::Ordering::Relaxed);
        }
        let _ = scored;
        real
    }

    // Scores every name the last strict pass left as TYPO_LATER, so a full row pass reads the names as the full matcher scores them.
    pub(super) fn fill_typos(&mut self, matcher: &Matcher, index: &Index) {
        if !self.whole {
            self.jobs.retain(|job| job.1 == TYPO_LATER);
            self.score_jobs(matcher, None, true, index);
            return;
        }
        let count = self.names.len();
        let threads = self.threads(count);
        let chunk = count.div_ceil(threads);
        if threads == 1 {
            self.workers[0].fill_slots(matcher, index, 0, &mut self.names);
        } else {
            std::thread::scope(|scope| {
                for (at, (slots, worker)) in self.names.chunks_mut(chunk).zip(&mut self.workers).enumerate() {
                    scope.spawn(move || worker.fill_slots(matcher, index, at * chunk, slots));
                }
            });
        }
    }

    // The name slots, lent to resolve while the scratch serves as its parent cache.
    pub(super) fn lend_names(&mut self) -> Owned {
        Owned { names: std::mem::take(&mut self.names), generation: self.generation }
    }

    pub(super) fn return_names(&mut self, owned: Owned) {
        self.names = owned.names;
    }

    // One thread under PARALLEL_SCORE_AT jobs, else the machine's cores up to QUERY_THREADS, each with its Worker ready.
    pub(super) fn threads(&mut self, jobs: usize) -> usize {
        let threads = if jobs < PARALLEL_SCORE_AT { 1 } else { self.cores().clamp(1, QUERY_THREADS) };
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
        (score > TYPO_LATER).then_some(score)
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
            let bytes = index.names.get(name);
            matcher.advance(state, bytes, &mut self.letters, false);
            // An ASCII part may skip fold, so its last byte decides; any other part was folded into letters.
            let boundary = if bytes.is_ascii() { bytes.last().is_none_or(|&byte| separator(byte as char)) }
                else { self.letters.last().is_none_or(|letter| separator(letter.lower)) };
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
            let boundary = self.letters.last().is_none_or(|letter| separator(letter.lower));
            self.texts.extend_from_slice(&self.letters);
            self.texts.push(Letter { lower: '/', boundary });
        }
        self.dirs[dir as usize].text = start..self.texts.len();
        self.dirs[dir as usize].text_ready = true;
    }
}

#[path = "searchrowpar.rs"]
mod rowpar;
pub(super) use rowpar::ParentCache;

#[cfg(test)]
#[path = "searchscratch_tests.rs"]
mod tests;
