// One live search session: the scope's snapshot index plus the standing query and its generation.
use crate::backend::searchmatch::Matcher;
use crate::backend::searchindex::{self, select_head, Index};
use crate::backend::searchreq::Answer;
use crate::backend::searchwalk::WalkProbes;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

// The first window when a query names none, the list's own first-window default.
pub(crate) const FIRST_WINDOW_DEFAULT: usize = 350;
// Update batches while the walk grows go out at most this often, so a fast walk cannot flood the client.
pub(crate) const BATCH_MS: u64 = 100;

// Test-only counters, so release pays nothing for the gate.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Counters {
    pub walks_started: usize,
    pub rows_scored: usize,
    pub rank_compares: usize,
    pub phases: timing::Phases,
}

// The wire lines live in searchreq.rs and the walk thread that fills the index in searchwalk.rs.

pub(crate) struct Session {
    pub scope: PathBuf,
    pub folder: PathBuf,
    pub hidden: bool,
    pub dev: u64,
    pub writable: bool,
    // The mountinfo body read at open, so every patch is bounded with no second read.
    pub(crate) body: String,
    pub(crate) index: Arc<Mutex<Index>>,
    pub(crate) walk_done: Arc<AtomicBool>,
    pub(crate) opened_at: Instant,
    pub(crate) walk_dirs: usize,
    pub(crate) walk_ms: f64,
    pub(crate) probes: Arc<WalkProbes>,
    pub(crate) handle: Option<std::thread::JoinHandle<usize>>,
    pub(crate) watch: Option<crate::backend::searchwatch::Watch>,
    // Unmount freezes this scope until reopen; explicit user cancellation still permits patches.
    pub(crate) unmounted: bool,
    pub(crate) last_query: String,
    pub(crate) matched_index_len: usize,
    pub(crate) last_matches: Vec<usize>,
    // False when last_matches left out typo rows no answer could show, so a narrowed answer that needs typos rescores the index.
    pub(crate) last_typos: bool,
    pub(crate) score_scratch: searchindex::QueryScratch,
    // Matches past the head, ranked on scroll; a patch drops them because it moves row ids.
    pub(crate) pending_rest: Option<Rest>,
    // Directories patched while the walk ran, applied at its landing; see src/backend/searchpatch.rs.
    pub(crate) pending_patches: std::collections::BTreeSet<PathBuf>,
    pub(crate) folder_n: usize,
    pub last_answered_gen: usize,
    pub(crate) standing: Option<(usize, String, usize)>,
    pub(crate) standing_while_walking: bool,
    // A query asked while the walk runs, waiting for it to land; see src/backend/searchhold.rs.
    pub(crate) held: Option<crate::backend::searchhold::Held>,
    // The cap of the walk's one hold window, stamped by the first query held; see src/backend/searchhold.rs.
    pub(crate) hold_until: Option<Instant>,
    pub(crate) announced: bool,
    pub(crate) last_batch: Instant,
    // Folder sizes this session's answers carry inline; see src/backend/answersizes.rs.
    pub(crate) sizes: crate::backend::answersizes::AnswerSizes,
    pub(crate) row_cache: crate::backend::searchrowcache::Cache,
    // The tag terms' names, read once per row off the loop thread; see src/backend/searchtags.rs.
    pub(crate) tags: crate::backend::searchtags::Tags,
    // The desktop priors loaded when the session opened, fixed so every answer of the open ranks alike.
    pub(crate) priors: Option<&'static crate::backend::searchprior::Priors>,
    #[cfg(test)]
    pub counters: Counters,
}

impl Session {
    // The one constructor: reads one mountinfo body, then walks on a background thread.
    pub(crate) fn open_async(
        scope: &str,
        folder: &str,
        hidden: bool,
        walked: crate::backend::events::Sender<crate::backend::events::Event>,
    ) -> Session {
        Self::open_async_inner(scope, folder, hidden, walked, Arc::new(WalkProbes::new()))
    }

    // Tests arm the walk hold on these probes before opening, so the park never races the walk.
    #[cfg(test)]
    pub(crate) fn open_async_with_probes(
        scope: &str,
        folder: &str,
        hidden: bool,
        walked: crate::backend::events::Sender<crate::backend::events::Event>,
        probes: Arc<WalkProbes>,
    ) -> Session {
        Self::open_async_inner(scope, folder, hidden, walked, probes)
    }

    // Tests open the one constructor and join its walk, so no second setup drifts.
    #[cfg(test)]
    pub(crate) fn open_sync(scope: &str, folder: &str, hidden: bool) -> Session {
        let (walked_tx, _walked_rx) = crate::backend::events::channel();
        let mut session = Session::open_async(scope, folder, hidden, walked_tx);
        session.join_walk();
        session
    }

    // True while the walk is still running; a cancelled walk is over even before its thread has stopped.
    pub(crate) fn walking(&self) -> bool {
        !self.walk_done.load(Ordering::Relaxed) && !self.probes.cancel.load(Ordering::Relaxed)
    }

    // The standing query for the landing re-run; None unless it was answered while walking.
    #[cfg(test)]
    pub(crate) fn landing(&self) -> Option<(usize, String, usize)> {
        if self.standing_while_walking { self.standing.clone() } else { None }
    }

    #[cfg(test)]
    pub(crate) fn walk_dirs(&self) -> usize {
        self.walk_dirs
    }

    #[cfg(test)]
    pub(crate) fn indexed(&self) -> usize {
        self.index.lock().unwrap().len()
    }

    pub(crate) fn partial_reason(&self) -> Option<&'static str> {
        self.index.lock().unwrap().partial
    }

    // Bytes the arena holds now; zero once close frees it, counted not sampled.
    #[cfg(test)]
    pub(crate) fn index_bytes(&self) -> usize {
        searchindex::index_bytes(&self.index.lock().unwrap())
    }

    // How many directories the walk has opened; the cancel and close tests read it frozen.
    #[cfg(test)]
    pub(crate) fn probes_opened(&self) -> usize {
        self.probes.opened.load(Ordering::Relaxed)
    }

    // Answers one query from the index with no walk; None means the generation is stale.
    pub(crate) fn query(&mut self, gen: usize, query: &str, count: usize) -> Option<Answer> {
        if gen <= self.last_answered_gen {
            return None;
        }
        self.tags.arm();
        let mut answer = self.answer(query, count);
        answer.gen = gen;
        self.last_query = query.to_string();
        #[cfg(test)]
        let store_started = Instant::now();
        self.last_matches = std::mem::take(&mut answer.all);
        self.last_typos = answer.typos;
        #[cfg(test)]
        { self.counters.phases.last_matches = store_started.elapsed(); }
        self.last_answered_gen = gen;
        self.standing = Some((gen, query.to_string(), count));
        self.standing_while_walking = answer.walking;
        self.last_batch = Instant::now();
        Some(answer)
    }

    // Remembers the folder count an empty query answered, so the landing line repeats it.
    pub(crate) fn note_folder_count(&mut self, n: usize) {
        self.folder_n = n;
    }

    // The landing line for an empty standing query: the standing gen, walking false, and no rows to re-run.
    pub(crate) fn landing_empty(&mut self) -> Option<(usize, usize, usize, f64)> {
        let (gen, query, _) = self.standing.clone()?;
        if !query.is_empty() || !self.standing_while_walking || self.walking() {
            return None;
        }
        self.standing_while_walking = false;
        Some((gen, self.folder_n, self.folder_n, self.walk_ms))
    }

    // Re-runs the standing query once when the walk lands, with its own gen and the walk's ms.
    pub(crate) fn re_answer_standing(&mut self) -> Option<Answer> {
        let (gen, query, count) = self.standing.clone()?;
        // A landed rewrite stands for one re-run of a tagged query, whatever answers came between the write and the landing.
        let rewritten = self.tags.rewrite_due(&query);
        if query.is_empty() || !(self.standing_while_walking || rewritten) {
            return None;
        }
        self.standing_while_walking = false;
        // The index grew under the partial answer, so narrowing to its matches would keep the blind spot.
        self.last_query.clear();
        self.last_matches.clear();
        let mut answer = self.answer(&query, count);
        answer.gen = gen;
        if !answer.walking { answer.ms = self.walk_ms; }
        // A tagged query whose names are still being read stands for one more re-run.
        self.standing_while_walking = answer.walking;
        self.last_query = query;
        self.last_matches = std::mem::take(&mut answer.all);
        self.last_typos = answer.typos;
        self.last_batch = Instant::now();
        Some(answer)
    }

    // Re-runs the standing query for an own-write push; None when no query stands.
    pub(crate) fn re_answer_now(&mut self) -> Option<Answer> {
        let (gen, query, count) = self.standing.clone()?;
        if query.is_empty() {
            return None;
        }
        let mut answer = self.answer(&query, count);
        answer.gen = gen;
        self.last_query = query;
        self.last_matches = std::mem::take(&mut answer.all);
        self.last_typos = answer.typos;
        self.last_batch = Instant::now();
        Some(answer)
    }

    // Claims a generation for the folder answer; false means the generation is stale.
    pub(crate) fn claim_gen(&mut self, gen: usize, query: &str, count: usize) -> bool {
        if gen <= self.last_answered_gen {
            return false;
        }
        self.row_cache.clear();
        self.last_answered_gen = gen;
        self.standing = Some((gen, query.to_string(), count));
        self.standing_while_walking = self.walking();
        self.last_batch = Instant::now();
        self.last_query.clear();
        self.held = None;
        // A folder listing replaces the result rows, so no held rest survives it.
        self.pending_rest = None;
        true
    }

    // How many unranked matches wait past the head; production spends them through take_rest_chunk.
    #[cfg(test)]
    pub(crate) fn pending_rest_len(&self) -> usize {
        self.pending_rest.as_ref().map_or(0, Rest::len)
    }

    // Ranks and materializes the next chunk past the adopted prefix; None when nothing is held.
    pub(crate) fn take_rest_chunk(&mut self, end: usize, adopted: usize) -> Option<Vec<(String, bool)>> {
        let held = self.pending_rest.as_ref()?.len();
        if held == 0 {
            self.pending_rest = None;
            return None;
        }
        // usize::MAX takes the whole rest in one go.
        let take = if end == usize::MAX {
            held
        } else {
            // The need or one window, and at least the adopted length, so a full scroll pays one sort over its chunks.
            let need = end.saturating_sub(adopted);
            need.max(FIRST_WINDOW_DEFAULT).max(adopted).min(held)
        };
        if take == 0 {
            return None;
        }
        let (rows, spent) = {
            let rest = self.pending_rest.as_mut()?;
            let index = self.index.lock().unwrap();
            rest.promote(take, &index, &mut self.score_scratch);
            let rows = rest.take_ranked(take, &index);
            (rows, rest.len() == 0)
        };
        if spent {
            self.pending_rest = None;
        }
        Some(rows)
    }

    // Patches one directory's entries when the open folder's watch reports it; false when its mount does not answer.
    pub(crate) fn patch_open_dir(&mut self, path: &Path) -> bool {
        if self.unmounted { return false; }
        let Ok(rel) = path.strip_prefix(&self.scope) else { return false };
        let probe = path.to_path_buf();
        let Ok(is_dir) = crate::backend::iomount::call(path, &self.body, "search", move || probe.is_dir()) else {
            return false;
        };
        // A reported file patches its own directory; a reported directory patches itself.
        self.patch(if is_dir { rel } else { rel.parent().unwrap_or(Path::new("")) })
    }

    // True when the last batch is old enough for an update push while the walk grows.
    pub(crate) fn batch_due(&self) -> bool {
        self.last_batch.elapsed().as_millis() >= u128::from(BATCH_MS)
    }

    #[cfg(test)]
    pub(crate) fn test_set_last_batch(&mut self, at: Instant) {
        self.last_batch = at;
    }
}

#[cfg(test)]
#[path = "searchsession_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "searchconservative_tests.rs"]
mod conservative_tests;

#[cfg(test)]
#[path = "searchcache_tests.rs"]
mod cache_tests;

#[path = "searchanswer.rs"]
mod answer;

#[path = "searchrest.rs"]
mod rest;
pub(crate) use rest::Rest;

#[cfg(test)]
#[path = "searchtiming.rs"]
pub(crate) mod timing;
