// The session walk: bounded parallel threads over a shared queue locally, one bounded worker per directory remotely.
use super::searchindex::{index_bytes, is_hidden, Index, WALK_THREADS};
use super::searchreq::Cancelled;
use super::searchsession::Session;
use super::searchwalkread::Class;
#[cfg(test)]
use super::searchsession::Counters;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[path = "searchwalk_probes.rs"]
mod probes;
#[path = "searchwalk_queue.rs"]
mod queue;
use queue::next_dir;


// How many directories the walk has opened; tests read it beside the cancel flag.
pub(crate) struct WalkProbes {
    pub opened: AtomicUsize,
    pub cancel: Arc<AtomicBool>,
    pub(crate) watch: Mutex<Option<Arc<super::searchwatch::Core>>>,
    // Test seam: parks this walk alone once it has opened `at` directories, until released.
    #[cfg(test)]
    pub(crate) hold: Mutex<Option<crate::backend::searchwalkhold::WalkHold>>,
    #[cfg(test)]
    pub(crate) read_hold: Arc<Mutex<Option<crate::backend::searchwalkhold::ReadHold>>>,
    #[cfg(test)]
    pub(crate) remote_reads: AtomicUsize,
    #[cfg(test)]
    pub(crate) body: Mutex<Option<String>>,
    #[cfg(test)]
    pub(crate) landing_sent: AtomicBool,
    // Test seam: the prior load this open reads instead of the process's own.
    #[cfg(test)]
    pub(crate) prior_load: Mutex<Option<&'static crate::backend::searchprior::Load>>,
}

// The whole walk: one mountinfo read, then the local or the remote walk.
#[cfg(test)]
pub(crate) fn walk(
    root: &Path,
    hidden: bool,
    remote: bool,
    body: &str,
    probes: &WalkProbes,
) -> (Index, usize) {
    let out = Mutex::new(Index::new());
    let dirs = fill(&out, root, hidden, remote, body, probes);
    (out.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner()), dirs)
}

// Fills a shared index, so a background walk publishes while queries score the partial one.
pub(crate) fn fill(
    out: &Mutex<Index>,
    root: &Path,
    hidden: bool,
    remote: bool,
    body: &str,
    probes: &WalkProbes,
) -> usize {
    let reader = super::searchwalkread::Reader::new(body);
    let dirs = if remote {
        walk_remote(out, root, hidden, &reader, probes)
    } else {
        walk_local(out, root, hidden, body, &reader, probes)
    };
    let mut index = out.lock().unwrap();
    if probes.cancel.load(Ordering::Relaxed) { index.mark_partial("cancelled"); }
    index.finish();
    dirs
}

// Local scopes walk with bounded worker threads over a shared directory queue.
fn walk_local(out: &Mutex<Index>, root: &Path, hidden: bool, body: &str, reader: &super::searchwalkread::Reader<'_>, probes: &WalkProbes) -> usize {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, WALK_THREADS);
    // The queue beside how many threads hold a popped directory; a thread ends only when both are empty.
    let work: Mutex<(VecDeque<Queued>, usize)> = Mutex::new((VecDeque::from([(root.to_path_buf(), None)]), 0));
    let ready = Condvar::new();
    let dirs = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some((dir, parent)) = next_dir(&work, &ready, &probes.cancel) {
                    let subdirs = open_directory(out, root, &dir, parent.as_ref(), hidden, Some(body), reader, probes);
                    if subdirs.is_some() {
                        dirs.fetch_add(1, Ordering::Relaxed);
                    }
                    let mut state = work.lock().unwrap();
                    state.0.extend(subdirs.unwrap_or_default());
                    state.1 -= 1;
                    drop(state);
                    ready.notify_all();
                }
            });
        }
    });
    // No reorder after the walk: a query may already hold row ids, and ranking is total on its own.
    dirs.load(Ordering::Relaxed)
}

// A queued directory and its parent's mount class; the scope has none, so it is classified in full.
type Queued = (PathBuf, Option<Class>);

// Opens one directory, pushes its rows, and returns its subdirectories; None once the walk is cancelled.
#[allow(clippy::too_many_arguments)]
fn open_directory(out: &Mutex<Index>, root: &Path, dir: &Path, parent: Option<&Class>, hidden: bool, watch_body: Option<&str>, reader: &super::searchwalkread::Reader<'_>, probes: &WalkProbes) -> Option<Vec<Queued>> {
    // A close cancels the walk at once: no directory opens after close.
    if probes.cancel.load(Ordering::Relaxed) {
        return None;
    }
    let rel = rel_of(root, dir);
    let class = reader.class(dir, parent);
    let watch_failed = if let (Some(body), Some(watch)) = (watch_body, probes.watch.lock().unwrap().as_ref()) {
        if class.is_some() { !watch.arm_local(&rel, body) }
        else { !watch.arm(&rel) }
    } else { false };
    probes.opened.fetch_add(1, Ordering::Relaxed);
    #[cfg(test)]
    probes.hold_if_due(probes.opened.load(Ordering::Relaxed), dir, false);
    if probes.cancel.load(Ordering::Relaxed) {
        return None;
    }
    let mut rows = Vec::new();
    let mut subdirs = Vec::new();
    let remote = class.is_some();
    let read = match reader.read_classed(dir, class.clone(), dir == root, watch_body.is_none(), probes) {
        Ok(read) => read,
        Err(error) if dir != root && !remote && matches!(error.raw_os_error(), Some(2 | 20 | 40)) => return None,
        Err(_) => {
            out.lock().unwrap().mark_partial("unreadable");
            return None;
        }
    };
    let partial = read.partial;
    for (name, is_dir) in read.entries {
        // A dot-prefixed name is dropped before it is counted, matching scan.rs's own hidden rule.
        if !hidden && is_hidden(&name) {
            continue;
        }
        // A symlink reports its own type, so a link to a directory is never descended.
        if is_dir {
            subdirs.push((dir.join(&name), Some(class.clone())));
        }
        rows.push((name, is_dir));
    }
    #[cfg(test)]
    probes.hold_if_due(probes.opened.load(Ordering::Relaxed), dir, true);
    // Rows land before their subdirectories queue, so a directory's rows always precede its children's.
    let mut index = out.lock().unwrap();
    // A cancel stored before this lock was taken is seen here, so a patch taken after the cancel is never pushed over; see src/backend/searchpatch.rs.
    if probes.cancel.load(Ordering::Relaxed) {
        return None;
    }
    if let Some(reason) = partial { index.mark_partial(reason); }
    if watch_failed { index.mark_partial("watch_failed"); }
    if !index.push_directory(rel.as_os_str(), &rows) { return None; }
    Some(subdirs)
}

// A remote scope keeps today's single bounded worker, so one dead server costs one deadline, never the walk.
fn walk_remote(out: &Mutex<Index>, root: &Path, hidden: bool, reader: &super::searchwalkread::Reader<'_>, probes: &WalkProbes) -> usize {
    let mut pending: Vec<Queued> = vec![(root.to_path_buf(), None)];
    let mut dirs = 0;
    while let Some((dir, parent)) = pending.pop() {
        // A close cancels the walk at once: no directory opens after close.
        if probes.cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Some(subdirs) = open_directory(out, root, &dir, parent.as_ref(), hidden, None, reader, probes) {
            pending.extend(subdirs);
            dirs += 1;
        }
    }
    dirs
}

// The path relative to the scope; the empty path is the scope itself.
fn rel_of(root: &Path, dir: &Path) -> PathBuf {
    dir.strip_prefix(root).map(Path::to_path_buf).unwrap_or_default()
}

// The walk-thread lifecycle of a session: spawn, landing poll, stop and close; the query side is searchsession.rs.
impl Session {
    pub(crate) fn open_async_inner(
        scope: &str,
        folder: &str,
        hidden: bool,
        walked: crate::backend::events::Sender<crate::backend::events::Event>,
        probes: Arc<WalkProbes>,
    ) -> Session {
        #[cfg(test)]
        let body = probes.body.lock().unwrap().take().unwrap_or_else(crate::backend::mountinfo::read_body);
        #[cfg(not(test))]
        let body = crate::backend::mountinfo::read_body();
        let scope_buf = PathBuf::from(scope);
        let remote = crate::backend::iomount::is_remote(&scope_buf, &body);
        // The root's figures go through the bound, so a dead server answers 0 and read-only instead of hanging the loop.
        let root = scope_buf.clone();
        let (dev, writable) = crate::backend::iomount::call(&scope_buf, &body, "search", move || {
            (crate::backend::fsinfo::dev_of(&root), crate::backend::ops::dir_writable(&root))
        }).unwrap_or((0, false));
        let index = Arc::new(Mutex::new(Index::new()));
        let walk_done = Arc::new(AtomicBool::new(false));
        let thread_index = Arc::clone(&index);
        let thread_probes = Arc::clone(&probes);
        let thread_done = Arc::clone(&walk_done);
        let thread_scope = scope_buf.clone();
        let thread_body = body.clone();
        let opened_at = Instant::now();
        let watch = (!remote).then(|| super::searchwatch::Watch::new(&scope_buf, hidden, walked.clone()));
        *probes.watch.lock().unwrap() = watch.as_ref().map(|watch| Arc::clone(&watch.core));
        let tags = crate::backend::searchtags::Tags::new(walked.clone());
        let progress_stop = Arc::new((Mutex::new(false), Condvar::new()));
        let progress_wait = Arc::clone(&progress_stop);
        let progress_cancel = Arc::clone(&probes);
        let progress_events = walked.clone();
        let progress = std::thread::spawn(move || {
            let (lock, ready) = &*progress_wait;
            let mut stop = lock.lock().unwrap();
            while !*stop && !progress_cancel.cancel.load(Ordering::Relaxed) {
                let waited = ready.wait_timeout(stop, Duration::from_millis(super::searchsession::BATCH_MS)).unwrap();
                stop = waited.0;
                if !*stop && waited.1.timed_out() && progress_events.send(crate::backend::events::Event::SearchWalked).is_err() { break; }
            }
        });
        let handle = std::thread::spawn(move || {
            let dirs = fill(&thread_index, &thread_scope, hidden, remote, &thread_body, &thread_probes);
            *progress_stop.0.lock().unwrap() = true;
            progress_stop.1.notify_all();
            let _ = progress.join();
            thread_done.store(true, Ordering::Release);
            let _ = walked.send(crate::backend::events::Event::SearchWalked);
            #[cfg(test)]
            thread_probes.landing_sent.store(true, Ordering::Release);
            dirs
        });
        #[cfg(test)]
        let counters = Counters { walks_started: 1, ..Counters::default() };
        // A search before any listing starts the priors load; this open ranks without them.
        crate::backend::searchprior::start();
        #[cfg(test)]
        let priors = probes.prior_load.lock().unwrap().map_or_else(crate::backend::searchprior::loaded, |load| load.loaded());
        #[cfg(not(test))]
        let priors = crate::backend::searchprior::loaded();
        Session {
            scope: scope_buf,
            folder: PathBuf::from(folder),
            hidden,
            dev,
            writable,
            body,
            index,
            walk_done,
            opened_at,
            walk_dirs: 0,
            walk_ms: 0.0,
            probes,
            handle: Some(handle),
            watch,
            unmounted: false,
            last_query: String::new(),
            matched_index_len: 0,
            last_matches: Vec::new(),
            last_typos: true,
            score_scratch: crate::backend::searchindex::QueryScratch::default(),
            pending_rest: None,
            pending_patches: Default::default(),
            folder_n: 0,
            last_answered_gen: 0,
            standing: None,
            standing_while_walking: false,
            held: None,
            hold_until: None,
            announced: false,
            last_batch: Instant::now(),
            sizes: crate::backend::answersizes::AnswerSizes::new(remote),
            tags,
            row_cache: Default::default(),
            priors,
            #[cfg(test)]
            counters,
        }
    }

    // Joins a finished walk and stamps it landed; tests use this, the loop polls.
    #[cfg(test)]
    pub(crate) fn join_walk(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.walk_dirs = handle.join().unwrap_or(0);
        }
        self.walk_done.store(true, Ordering::Relaxed);
        self.land_changes();
        self.announced = true;
        self.walk_ms = self.opened_at.elapsed().as_secs_f64() * 1000.0;
    }

    // True once after the walk lands, so the loop re-runs the standing query exactly once.
    pub(crate) fn poll_walked(&mut self) -> bool {
        // The tag worker lands through the same event, so the standing tagged query is re-run once its names are in.
        if self.tags.poll_read() {
            return true;
        }
        if !self.walk_done.load(Ordering::Relaxed) {
            return self.standing_while_walking && self.held.is_none() && self.batch_due()
                && self.index.lock().unwrap().len() != self.matched_index_len;
        }
        if self.announced { return false; }
        self.announced = true;
        if let Some(handle) = self.handle.take() {
            // The event arrived, so the walker already sent and joining ends at once.
            self.walk_dirs = handle.join().unwrap_or(0);
        }
        self.walk_ms = self.opened_at.elapsed().as_secs_f64() * 1000.0;
        self.land_changes();
        true
    }

    // A standing answer given once the walk finished predates what the landing applies, so a change re-runs it like one given while walking.
    fn land_changes(&mut self) {
        let changed = self.apply_watch_deltas() | self.flush_patches();
        if changed && self.standing.as_ref().is_some_and(|(_, query, _)| !query.is_empty()) {
            self.standing_while_walking = true;
        }
    }

    // Stops a running walk and keeps the session with its partial index; None when no walk runs.
    pub(crate) fn cancel_walk(&mut self) -> Option<Cancelled> {
        // A cancelled walk is not a running one, so a second cancel stays silent too.
        if !self.walking() || self.probes.cancel.load(Ordering::Relaxed) {
            return None;
        }
        self.probes.cancel.store(true, Ordering::Relaxed);
        self.watch.take();
        self.index.lock().unwrap().mark_partial("cancelled");
        self.flush_patches();
        self.stand_held();
        // The landing event still arrives; swallowing it here stops the re-run it would fire.
        self.announced = true;
        self.standing_while_walking = false;
        // A landed join is quick, a running walk detaches like close: the loop never waits on it.
        drop(self.handle.take());
        let gen = self.standing.as_ref().map(|standing| standing.0).unwrap_or(self.last_answered_gen);
        let indexed = self.index.lock().unwrap().len();
        let answer = match self.standing.clone() {
            Some((_, query, count)) if !query.is_empty() => {
                let mut answer = self.answer(&query, count);
                answer.gen = gen;
                answer.walking = false;
                self.last_query = query;
                self.last_matches = std::mem::take(&mut answer.all);
                self.last_typos = answer.typos;
                self.last_batch = Instant::now();
                Some(answer)
            }
            _ => None,
        };
        Some(Cancelled { answer, gen, indexed })
    }

    // Close cancels a running walk at once; a landed walk frees its arena bytes, a running one detaches and frees none yet.
    pub(crate) fn close(mut self) -> usize {
        self.probes.cancel.store(true, Ordering::Relaxed);
        self.watch.take();
        // A landed walk's thread is already gone, so joining ends at once; a running one detaches behind cancel.
        if self.walk_done.load(Ordering::Relaxed) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            index_bytes(&self.index.lock().unwrap())
        } else {
            0
        }
    }
}

#[cfg(test)]
#[path = "searchwalk_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "searchmount_tests.rs"]
mod mount_tests;

#[cfg(test)]
#[path = "searchmountprobe_tests.rs"]
mod mount_probe;
