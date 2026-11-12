// One worker at a time, replaced when stuck: slow filesystems delay sizes, never navigation.
use super::dirsizewatch::{Pins, TreeWatch};
use super::{dirsize, events::Event};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{channel, Receiver, Sender},
    Arc,
};
use std::time::{Duration, Instant};

pub struct Done {
    pub generation: u64,
    pub row: usize,
    pub result: dirsize::DirSize,
    pub ms: f64,
    // The watches the walk armed; only a whole, unchanged set lets the loop keep this size by path.
    pub pins: Pins,
}

struct Job {
    generation: u64,
    rows: Vec<(usize, PathBuf, Arc<AtomicBool>)>,
}

struct Active {
    generation: u64,
    pending: HashMap<usize, Arc<AtomicBool>>,
}

// A wedged mount stays skipped this long, then one walk tries it again.
const STUCK_TTL_SECS: u64 = 60;
// One blocking readdir on a slow share may answer past the deadline, so stuck waits this longer.
const STUCK_GRACE_MS: u64 = 1000;

pub struct Worker {
    jobs: super::lazy::Lazy<Sender<Job>>,
    generation: Arc<AtomicU64>,
    active: Option<Active>,
    events: crate::backend::events::Sender<Event>,
    started: Option<Instant>,
    current: Arc<std::sync::Mutex<Option<PathBuf>>>,
    stuck: HashMap<PathBuf, (Instant, Arc<AtomicBool>)>,
    abandoned: HashMap<u64, (usize, Arc<AtomicBool>)>,
    watch: TreeWatch,
}

// One thread per worker generation; a stuck thread is abandoned, never joined.
fn spawn_thread<F>(
    rx: Receiver<Job>,
    current_gen: Arc<AtomicU64>,
    events: crate::backend::events::Sender<Event>,
    walk: F,
    current: Arc<std::sync::Mutex<Option<PathBuf>>>,
) where
    F: Fn(&Path, &dyn Fn() -> bool) -> (dirsize::DirSize, Pins) + Send + 'static,
{
    // The recursive walker previously used the main thread; retain stack headroom for deep trees.
    std::thread::Builder::new()
        .name("flea-dirsize".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            for job in rx {
                for (row, path, stopped) in job.rows {
                    let cancelled = || {
                        current_gen.load(Ordering::Relaxed) != job.generation
                            || stopped.load(Ordering::Relaxed)
                    };
                    // Account for queued rows after cancellation without entering their paths.
                    let (result, pins, ms) = if cancelled() {
                        (
                            dirsize::DirSize {
                                bytes: 0,
                                partial: true,
                            },
                            Pins::default(),
                            0.0,
                        )
                    } else {
                        *current.lock().unwrap_or_else(|p| p.into_inner()) = Some(path.clone());
                        let t = Instant::now();
                        let (result, pins) = walk(&path, &cancelled);
                        (result, pins, t.elapsed().as_secs_f64() * 1000.0)
                    };
                    let done = Done {
                        generation: job.generation,
                        row,
                        result,
                        ms,
                        pins,
                    };
                    if events.send(Event::DirSize(done)).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("could not start directory size worker");
}

fn production_walk(
    watch: TreeWatch,
) -> impl Fn(&Path, &dyn Fn() -> bool) -> (dirsize::DirSize, Pins) + Send + 'static {
    move |path, cancelled| {
        let deadline = Instant::now() + Duration::from_millis(dirsize::DEADLINE_MS);
        super::dirsizewatch::walk_watched(path, deadline, cancelled, &watch)
    }
}

// A fresh thread always runs the production walk; a test mock blocks only the first thread.
fn spawn_production(
    rx: Receiver<Job>,
    current_gen: Arc<AtomicU64>,
    events: crate::backend::events::Sender<Event>,
    current: Arc<std::sync::Mutex<Option<PathBuf>>>,
    watch: &TreeWatch,
) {
    spawn_thread(
        rx,
        current_gen,
        events,
        production_walk(watch.clone()),
        current,
    )
}

// Sample mountinfo: "30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw" keys "/media/stick".
fn mount_key(path: &Path) -> PathBuf {
    super::iomount::mount_key_plain(path, &super::iomount::mount_body())
}

// A stuck mark stays while inside its TTL or its walk is still out; prune and skip read one rule.
fn mark_alive((t, done): &(Instant, Arc<AtomicBool>)) -> bool {
    t.elapsed() < Duration::from_secs(STUCK_TTL_SECS) || !done.load(Ordering::Relaxed)
}

impl Worker {
    pub fn new(events: crate::backend::events::Sender<Event>) -> Self {
        let watch = TreeWatch::new();
        Self::with_pinned_walk(events, production_walk(watch.clone()), watch)
    }

    // A test walk arms nothing, so its sizes are never kept by path.
    #[cfg(test)]
    fn with_walk<F>(events: crate::backend::events::Sender<Event>, walk: F) -> Self
    where
        F: Fn(&std::path::Path, &dyn Fn() -> bool) -> dirsize::DirSize + Send + 'static,
    {
        Self::with_pinned_walk(
            events,
            move |path, cancelled| (walk(path, cancelled), Pins::default()),
            TreeWatch::new(),
        )
    }

    fn with_pinned_walk<F>(
        events: crate::backend::events::Sender<Event>,
        walk: F,
        watch: TreeWatch,
    ) -> Self
    where
        F: Fn(&std::path::Path, &dyn Fn() -> bool) -> (dirsize::DirSize, Pins) + Send + 'static,
    {
        let generation = Arc::new(AtomicU64::new(0));
        let current = Arc::new(std::sync::Mutex::new(None));
        let (worker_gen, worker_events, worker_current) =
            (generation.clone(), events.clone(), Arc::clone(&current));
        let jobs = super::lazy::Lazy::new(move || {
            let (jobs, rx) = channel::<Job>();
            spawn_thread(rx, worker_gen, worker_events, walk, worker_current);
            jobs
        });
        Self {
            jobs,
            generation,
            active: None,
            events,
            started: None,
            current,
            stuck: HashMap::new(),
            abandoned: HashMap::new(),
            watch,
        }
    }

    // Past the walk deadline plus one slow readdir, so the next batch replaces the thread.
    pub fn stuck(&self) -> bool {
        self.started.is_some_and(|t| {
            t.elapsed() >= Duration::from_millis(dirsize::DEADLINE_MS + STUCK_GRACE_MS)
        })
    }
    pub fn busy(&self) -> bool {
        self.active.is_some() && !self.stuck()
    }
    pub fn contains(&self, row: usize) -> bool {
        let generation = self.generation.load(Ordering::Relaxed);
        self.active.as_ref().is_some_and(|active| {
            active.generation == generation
                && active
                    .pending
                    .get(&row)
                    .is_some_and(|stopped| !stopped.load(Ordering::Relaxed))
        })
    }
    fn prune_stuck(&mut self) {
        // A dead mount holds one thread: its mark stays until the abandoned walk returns, whatever the TTL says.
        self.stuck.retain(|_, mark| mark_alive(mark));
    }
    // Sample input: mark 61 s old with its thread still out stays, with its thread back goes.
    fn stuck_skips(&self, path: &Path) -> bool {
        self.stuck.get(&mount_key(path)).is_some_and(mark_alive)
    }
    pub fn start(&mut self, rows: Vec<(usize, PathBuf)>) {
        assert!(!self.busy());
        // A batch past its deadline abandons its thread, so sizes resume on a fresh worker.
        if self.active.is_some() {
            if let Some(held) = self
                .current
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
            {
                let flag = Arc::new(AtomicBool::new(false));
                if let Some(active) = self.active.as_ref() {
                    self.abandoned
                        .insert(active.generation, (active.pending.len(), Arc::clone(&flag)));
                }
                self.stuck.insert(mount_key(&held), (Instant::now(), flag));
            }
            self.generation.fetch_add(1, Ordering::Relaxed);
            let (jobs, rx) = channel::<Job>();
            spawn_production(
                rx,
                self.generation.clone(),
                self.events.clone(),
                Arc::clone(&self.current),
                &self.watch,
            );
            self.jobs = jobs.into();
            self.active = None;
            self.started = None;
        }
        self.prune_stuck();
        let mut pending = HashMap::with_capacity(rows.len());
        let mut walk_rows = Vec::new();
        let mut skipped = Vec::new();
        for (row, path) in rows {
            let std::collections::hash_map::Entry::Vacant(entry) = pending.entry(row) else {
                continue;
            };
            let stopped = Arc::new(AtomicBool::new(false));
            entry.insert(stopped.clone());
            if self.stuck_skips(&path) {
                skipped.push(row);
            } else {
                walk_rows.push((row, path, stopped));
            }
        }
        if walk_rows.is_empty() && skipped.is_empty() {
            return;
        }
        let generation = self
            .generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if !walk_rows.is_empty() {
            let _ = self.jobs.send(Job {
                generation,
                rows: walk_rows,
            });
        }
        self.active = Some(Active {
            generation,
            pending,
        });
        self.started = Some(Instant::now());
        // Wedged mounts answer a floor at once, never a walk that would wedge the fresh thread.
        for row in skipped {
            let _ = self.events.send(Event::DirSize(Done {
                generation,
                row,
                result: dirsize::DirSize {
                    bytes: 0,
                    partial: true,
                },
                ms: 0.0,
                pins: Pins::default(),
            }));
        }
    }
    pub fn cancel(&mut self) {
        // The clock keeps running, so a walk wedged in a syscall after cancel still becomes stuck.
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
    // Row flags stop only viewport exits, without invalidating still-visible rows in this generation.
    pub fn cancel_rows(&mut self, rows: &[usize]) {
        if let Some(active) = &self.active {
            for row in rows {
                if let Some(stopped) = active.pending.get(row) {
                    stopped.store(true, Ordering::Relaxed);
                }
            }
        }
    }
    // Even a cancelled completion releases the single slot, but cannot publish a stale row.
    pub fn accept(&mut self, done: &Done) -> bool {
        let active_gen = match self.active.as_ref() {
            Some(a) => a.generation,
            None => {
                self.note_abandoned(done);
                return false;
            }
        };
        if active_gen != done.generation {
            self.note_abandoned(done);
            return false;
        }
        let (current, finished) = {
            let active = self.active.as_mut().expect("checked above");
            let Some(stopped) = active.pending.remove(&done.row) else {
                return false;
            };
            let current = self.generation.load(Ordering::Relaxed) == done.generation
                && !stopped.load(Ordering::Relaxed);
            (current, active.pending.is_empty())
        };
        if finished {
            self.active = None;
            self.started = None;
        } else {
            // One walk answered, so the next row gets its own deadline rather than the batch's.
            self.started = Some(Instant::now());
        }
        current
    }
    // An abandoned walk's late row counts down its thread, so one dead mount holds at most one thread.
    fn note_abandoned(&mut self, done: &Done) {
        let finished = match self.abandoned.get_mut(&done.generation) {
            Some((left, _)) => {
                *left = left.saturating_sub(1);
                *left == 0
            }
            None => return,
        };
        if finished {
            if let Some((_, flag)) = self.abandoned.remove(&done.generation) {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
impl Worker {
    // Puts the active batch past its deadline and its grace without waiting for either.
    fn force_past_deadline(&mut self) {
        self.started = Some(
            Instant::now()
                - Duration::from_millis(dirsize::DEADLINE_MS + STUCK_GRACE_MS)
                - Duration::from_secs(1),
        );
    }
}

#[cfg(test)]
#[path = "dirsizeworker_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dirsize_selective_tests.rs"]
mod selective_tests;
