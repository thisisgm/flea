// One worker at a time, replaced when stuck: slow filesystems delay sizes, never navigation.
use super::{dirsize, events::Event};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}, mpsc::{channel, Receiver, Sender}};
use std::time::{Duration, Instant};

pub struct Done {
    pub generation: u64,
    pub row: usize,
    pub result: dirsize::DirSize,
    pub ms: f64,
}

struct Job {
    generation: u64,
    rows: Vec<(usize, PathBuf)>,
}

struct Active {
    generation: u64,
    pending: HashSet<usize>,
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
}

// One thread per worker generation; a stuck thread is abandoned, never joined.
fn spawn_thread<F>(rx: Receiver<Job>, current_gen: Arc<AtomicU64>, events: crate::backend::events::Sender<Event>, walk: F, current: Arc<std::sync::Mutex<Option<PathBuf>>>)
where F: Fn(&Path, &dyn Fn() -> bool) -> dirsize::DirSize + Send + 'static {
    // The recursive walker previously used the main thread; retain stack headroom for deep trees.
    std::thread::Builder::new().name("flea-dirsize".into()).stack_size(16 * 1024 * 1024).spawn(move || {
        for job in rx {
            for (row, path) in job.rows {
                // Account for queued rows after cancellation without entering their paths.
                let (result, ms) = if current_gen.load(Ordering::Relaxed) != job.generation {
                    (dirsize::DirSize { bytes: 0, partial: true }, 0.0)
                } else {
                    *current.lock().unwrap_or_else(|p| p.into_inner()) = Some(path.clone());
                    let t = Instant::now();
                    let result = walk(&path, &|| current_gen.load(Ordering::Relaxed) != job.generation);
                    (result, t.elapsed().as_secs_f64() * 1000.0)
                };
                let done = Done { generation: job.generation, row, result, ms };
                if events.send(Event::DirSize(done)).is_err() { return; }
            }
        }
    }).expect("could not start directory size worker");
}

fn production_walk(path: &Path, cancelled: &dyn Fn() -> bool) -> dirsize::DirSize {
    dirsize::walk_cancellable(path, Instant::now() + Duration::from_millis(dirsize::DEADLINE_MS), &cancelled)
}

// A fresh thread always runs the production walk; a test mock blocks only the first thread.
fn spawn_production(rx: Receiver<Job>, current_gen: Arc<AtomicU64>, events: crate::backend::events::Sender<Event>, current: Arc<std::sync::Mutex<Option<PathBuf>>>) {
    spawn_thread(rx, current_gen, events, production_walk, current)
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
        Self::with_walk(events, production_walk)
    }

    fn with_walk<F>(events: crate::backend::events::Sender<Event>, walk: F) -> Self
    where F: Fn(&std::path::Path, &dyn Fn() -> bool) -> dirsize::DirSize + Send + 'static {
        let generation = Arc::new(AtomicU64::new(0));
        let current = Arc::new(std::sync::Mutex::new(None));
        let (worker_gen, worker_events, worker_current) = (generation.clone(), events.clone(), Arc::clone(&current));
        let jobs = super::lazy::Lazy::new(move || {
            let (jobs, rx) = channel::<Job>();
            spawn_thread(rx, worker_gen, worker_events, walk, worker_current);
            jobs
        });
        Self { jobs, generation, active: None, events, started: None, current, stuck: HashMap::new(), abandoned: HashMap::new() }
    }

    // Past the walk deadline plus one slow readdir, so the next batch replaces the thread.
    pub fn stuck(&self) -> bool {
        self.started.is_some_and(|t| t.elapsed() >= Duration::from_millis(dirsize::DEADLINE_MS + STUCK_GRACE_MS))
    }
    pub fn busy(&self) -> bool { self.active.is_some() && !self.stuck() }
    pub fn contains(&self, row: usize) -> bool {
        let generation = self.generation.load(Ordering::Relaxed);
        self.active.as_ref().is_some_and(|active| {
            active.generation == generation && active.pending.contains(&row)
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
            if let Some(held) = self.current.lock().unwrap_or_else(|p| p.into_inner()).clone() {
                let flag = Arc::new(AtomicBool::new(false));
                if let Some(active) = self.active.as_ref() {
                    self.abandoned.insert(active.generation, (active.pending.len(), Arc::clone(&flag)));
                }
                self.stuck.insert(mount_key(&held), (Instant::now(), flag));
            }
            self.generation.fetch_add(1, Ordering::Relaxed);
            let (jobs, rx) = channel::<Job>();
            spawn_production(rx, self.generation.clone(), self.events.clone(), Arc::clone(&self.current));
            self.jobs = jobs.into();
            self.active = None;
            self.started = None;
        }
        self.prune_stuck();
        let mut pending = HashSet::with_capacity(rows.len());
        let mut walk_rows = Vec::new();
        let mut skipped = Vec::new();
        for (row, path) in rows {
            if !pending.insert(row) {
                continue;
            }
            if self.stuck_skips(&path) {
                skipped.push(row);
            } else {
                walk_rows.push((row, path));
            }
        }
        if walk_rows.is_empty() && skipped.is_empty() {
            return;
        }
        let generation = self.generation.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        if !walk_rows.is_empty() {
            let _ = self.jobs.send(Job { generation, rows: walk_rows });
        }
        self.active = Some(Active { generation, pending });
        self.started = Some(Instant::now());
        // Wedged mounts answer a floor at once, never a walk that would wedge the fresh thread.
        for row in skipped {
            let _ = self.events.send(Event::DirSize(Done { generation, row, result: dirsize::DirSize { bytes: 0, partial: true }, ms: 0.0 }));
        }
    }
    pub fn cancel(&mut self) {
        // The clock keeps running, so a walk wedged in a syscall after cancel still becomes stuck.
        self.generation.fetch_add(1, Ordering::Relaxed);
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
            if !active.pending.remove(&done.row) {
                return false;
            }
            let current = self.generation.load(Ordering::Relaxed) == done.generation;
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
    fn drop(&mut self) { self.cancel(); }
}

#[cfg(test)]
impl Worker {
    // Puts the active batch past its deadline and its grace without waiting for either.
    fn force_past_deadline(&mut self) {
        self.started = Some(Instant::now() - Duration::from_millis(dirsize::DEADLINE_MS + STUCK_GRACE_MS) - Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Depth 1900 exceeds the 1024 open-file limit of a per-level-descriptor traversal like remove_dir_all, so cleanup runs on every exit including panic while the walk itself holds one listing (see dirsize.rs).
    const DEEP_DEPTH: usize = 1900;

    struct DeepTree {
        sandbox: super::super::testdir::TestDir,
        leaf: PathBuf,
    }

    impl DeepTree {
        // Depth stays what the stack proof was taken at: under PATH_MAX as paths, over any real tree.
        fn build(tag: &str) -> Self {
            let sandbox = super::super::testdir::TestDir::new(tag);
            let mut leaf = sandbox.path().to_path_buf();
            for _ in 0..DEEP_DEPTH {
                leaf.push("d");
                sandbox.assert_contains(&leaf);
                std::fs::create_dir(&leaf).unwrap();
            }
            Self { sandbox, leaf }
        }
        fn root(&self) -> &std::path::Path {
            self.sandbox.path()
        }
    }

    impl Drop for DeepTree {
        fn drop(&mut self) {
            // Bottom up, one rmdir at a time; whatever this cannot take falls through to the sandbox drop.
            while self.leaf != self.sandbox.path() {
                if std::fs::remove_dir(&self.leaf).is_err() {
                    break;
                }
                self.leaf.pop();
            }
        }
    }

    // A test dying before manual cleanup leaves its tree to the sandbox drop, whose remove_dir_all holds a descriptor per level.
    #[test]
    fn a_deep_tree_is_removed_even_when_its_test_fails_before_cleanup() {
        let slot: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
        // No invariant crosses the panic: the slot write completes before it.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let tree = DeepTree::build("size-deep-unwind");
            *slot.lock().unwrap() = Some(tree.root().to_path_buf());
            panic!("simulated failure before cleanup");
        }));
        assert!(result.is_err(), "the simulated failure did not panic");
        let kept = slot.lock().unwrap().take().unwrap();
        assert!(!kept.exists(), "a failed deep test left its tree behind: {}", kept.display());
    }

    #[test]
    fn deep_directory_tree_fits_the_worker_stack() {
        let tree = DeepTree::build("size-deep");
        let (events, rx) = crate::backend::events::channel();
        let mut worker = Worker::new(events);
        worker.start(vec![(0, tree.root().to_path_buf())]);
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(10)).unwrap() else { panic!() };
        assert!(worker.accept(&done));
        assert!(done.result.bytes > 0);
        assert!(!done.result.partial, "the walk reached the bottom of the tree rather than stopping short of it");
    }

    #[test]
    fn cancelled_running_job_cannot_publish_to_reused_row() {
        let (events, rx) = crate::backend::events::channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let mut worker = Worker::with_walk(events, move |_, cancelled| {
            entered.send(()).unwrap();
            released.recv().unwrap();
            dirsize::DirSize { bytes: 42, partial: cancelled() }
        });
        worker.start(vec![(0, PathBuf::new())]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.cancel();
        assert!(worker.busy());
        assert!(!worker.contains(0));
        release.send(()).unwrap();
        let Event::DirSize(old) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(old.result.partial);
        assert!(!worker.accept(&old));
        assert!(!worker.busy());
        worker.start(vec![(0, PathBuf::new())]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!worker.accept(&old));
        assert!(worker.busy());
        release.send(()).unwrap();
        let Event::DirSize(new) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(!new.result.partial);
        assert!(worker.accept(&new));
        assert!(!worker.busy());
    }

    #[test]
    fn a_completed_row_does_not_release_later_rows_in_same_batch() {
        let (events, rx) = crate::backend::events::channel();
        let (release, released) = channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut worker = Worker::with_walk(events, move |_, _| {
            if seen.fetch_add(1, Ordering::Relaxed) == 1 {
                released.recv().unwrap();
            }
            dirsize::DirSize { bytes: 7, partial: false }
        });
        worker.start(vec![(0, PathBuf::from("first")), (1, PathBuf::from("second"))]);
        let Event::DirSize(first) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&first));
        assert!(worker.busy());
        release.send(()).unwrap();
        let Event::DirSize(second) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&second));
        assert!(!worker.busy());
    }

    #[test]
    fn cancellation_skips_queued_paths_and_fresh_batch_progresses() {
        let (events, rx) = crate::backend::events::channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut worker = Worker::with_walk(events, move |_, cancelled| {
            let call = seen.fetch_add(1, Ordering::Relaxed);
            if call == 0 {
                entered.send(()).unwrap();
                released.recv().unwrap();
            }
            dirsize::DirSize { bytes: 9, partial: cancelled() }
        });
        worker.start(vec![(0, PathBuf::from("running")), (1, PathBuf::from("skipped"))]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.cancel();
        release.send(()).unwrap();
        for _ in 0..2 {
            let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
            assert!(!worker.accept(&done));
        }
        assert!(!worker.busy());
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        worker.start(vec![(2, PathBuf::from("fresh"))]);
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&done));
        assert!(!worker.busy());
    }

    #[test]
    fn duplicate_done_cannot_release_or_reuse_a_batch_slot() {
        let (events, rx) = crate::backend::events::channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut worker = Worker::with_walk(events, move |_, _| {
            if seen.fetch_add(1, Ordering::Relaxed) == 1 {
                entered.send(()).unwrap();
                released.recv().unwrap();
            }
            dirsize::DirSize { bytes: 11, partial: false }
        });
        worker.start(vec![(0, PathBuf::from("old"))]);
        let Event::DirSize(old) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&old));
        worker.start(vec![(0, PathBuf::from("new"))]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!worker.accept(&old));
        assert!(worker.busy());
        release.send(()).unwrap();
        let Event::DirSize(new) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&new));
        assert!(!worker.busy());
    }

    #[test]
    fn a_stuck_batch_is_replaced_so_sizes_resume() {
        let (events, rx) = crate::backend::events::channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let mut worker = Worker::with_walk(events, move |path: &Path, _| {
            if path == Path::new("stuck") {
                entered.send(()).unwrap();
                released.recv().unwrap();
            }
            dirsize::DirSize { bytes: 7, partial: false }
        });
        worker.start(vec![(0, PathBuf::from("stuck"))]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(worker.busy(), "a running walk holds the slot");
        worker.force_past_deadline();
        assert!(worker.stuck(), "past the deadline the batch counts as stuck");
        assert!(!worker.busy(), "a stuck batch releases the slot for a fresh worker");
        worker.start(vec![(1, PathBuf::from("fresh"))]);
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert_eq!(done.row, 1, "the fresh worker answers while the stuck thread is still held");
        assert!(worker.accept(&done));
        release.send(()).unwrap();
    }

    #[test]
    fn a_healthy_batch_restarts_its_clock_per_row() {
        // Sample input: batch start forced past the deadline, then one row lands, so stuck clears.
        let (events, rx) = crate::backend::events::channel();
        let (release, released) = channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut worker = Worker::with_walk(events, move |_, _| {
            if seen.fetch_add(1, Ordering::Relaxed) == 1 {
                released.recv().unwrap();
            }
            dirsize::DirSize { bytes: 7, partial: false }
        });
        worker.start(vec![(0, PathBuf::from("first")), (1, PathBuf::from("second"))]);
        let Event::DirSize(first) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        worker.force_past_deadline();
        assert!(worker.stuck(), "a batch past its deadline counts as stuck");
        assert!(worker.accept(&first), "the first row still publishes");
        assert!(!worker.stuck(), "one answered row restarts the clock for the next walk");
        assert!(worker.busy(), "the batch still holds the slot for its second row");
        release.send(()).unwrap();
        let Event::DirSize(second) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&second));
        assert!(!worker.busy());
    }

    #[test]
    fn a_cancelled_batch_keeps_its_clock_so_a_wedge_still_times_out() {
        // Sample input: cancel with a walk in flight keeps started, so the wedge becomes stuck.
        let (events, _rx) = crate::backend::events::channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let mut worker = Worker::with_walk(events, move |_, _| {
            entered.send(()).unwrap();
            released.recv().unwrap();
            dirsize::DirSize { bytes: 7, partial: false }
        });
        worker.start(vec![(0, PathBuf::from("wedged"))]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.cancel();
        assert!(worker.started.is_some(), "cancel leaves the clock running for the wedged walk");
        assert!(!worker.stuck(), "a fresh cancel is not yet stuck");
        worker.force_past_deadline();
        assert!(worker.stuck(), "the cancelled wedge still becomes stuck and is replaced");
        assert!(!worker.busy(), "so the slot releases for a fresh worker");
        release.send(()).unwrap();
    }

    #[test]
    fn a_dead_mount_holds_one_thread_until_its_walk_returns() {
        // Sample input: mark 61 s old with its thread still out stays, with its thread back goes.
        let (events, _rx) = crate::backend::events::channel();
        let mut worker = Worker::with_walk(events, |_, _| dirsize::DirSize { bytes: 1, partial: false });
        let probe = PathBuf::from("/media/stick");
        let mount = mount_key(&probe);
        let live = Arc::new(AtomicBool::new(false));
        worker.stuck.insert(mount.clone(), (Instant::now() - Duration::from_secs(STUCK_TTL_SECS + 1), Arc::clone(&live)));
        worker.prune_stuck();
        assert!(worker.stuck.contains_key(&mount), "a thread still out keeps its mark past the TTL");
        assert!(worker.stuck_skips(&probe), "so the dead mount is still skipped");
        live.store(true, Ordering::Relaxed);
        worker.prune_stuck();
        assert!(!worker.stuck.contains_key(&mount), "a returned thread lets the TTL prune the mark");
        assert!(!worker.stuck_skips(&probe), "so one walk tries the mount again");
    }

    #[test]
    fn a_batch_just_past_its_deadline_is_not_stuck_until_its_grace_passes() {
        // Sample input: started DEADLINE_MS plus 100 ms answers false, plus the grace answers true.
        let (events, _rx) = crate::backend::events::channel();
        let mut worker = Worker::with_walk(events, |_, _| dirsize::DirSize { bytes: 1, partial: false });
        worker.start(vec![(0, PathBuf::from("slow"))]);
        worker.started = Some(Instant::now() - Duration::from_millis(dirsize::DEADLINE_MS + 100));
        assert!(!worker.stuck(), "one slow readdir past the deadline is grace, not a wedge");
        worker.started = Some(Instant::now() - Duration::from_millis(dirsize::DEADLINE_MS + STUCK_GRACE_MS + 100));
        assert!(worker.stuck(), "past the grace the batch counts as stuck");
    }

    #[test]
    fn an_abandoned_batch_sets_its_mount_flag_once_its_walk_returns() {
        // Sample input: two-row batch wedged on row 0, abandoned, then both stale Dones accepted.
        let (events, rx) = crate::backend::events::channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut worker = Worker::with_walk(events, move |_, _| {
            if seen.fetch_add(1, Ordering::Relaxed) == 0 {
                entered.send(()).unwrap();
                released.recv().unwrap();
            }
            dirsize::DirSize { bytes: 7, partial: false }
        });
        let first = PathBuf::from("wedged-first");
        let second = PathBuf::from("wedged-second");
        worker.start(vec![(0, first.clone()), (1, second)]);
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.force_past_deadline();
        assert!(worker.stuck(), "the wedged batch counts as stuck");
        worker.start(vec![(2, PathBuf::from("fresh"))]);
        release.send(()).unwrap();
        let mut stale = 0;
        for _ in 0..3 {
            let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
            if done.row == 2 {
                assert!(worker.accept(&done), "the fresh row still publishes");
            } else {
                assert!(!worker.accept(&done), "a stale row never publishes");
                stale += 1;
            }
        }
        assert_eq!(stale, 2, "both wedged rows answer once the walk returns");
        let mount = mount_key(&first);
        let done = worker.stuck.get(&mount).map(|(_, flag)| flag.load(Ordering::Relaxed)).unwrap_or(false);
        assert!(done, "two stale Dones store the mount flag the mark owns");
        worker.stuck.get_mut(&mount).unwrap().0 = Instant::now() - Duration::from_secs(STUCK_TTL_SECS + 1);
        worker.prune_stuck();
        assert!(!worker.stuck.contains_key(&mount), "a returned thread lets the TTL prune the mark");
        assert!(!worker.stuck_skips(&first), "so one walk tries the mount again");
    }

    #[test]
    fn duplicate_rows_are_walked_once_and_all_unique_rows_release_batch() {
        let (events, rx) = crate::backend::events::channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut worker = Worker::with_walk(events, move |_, _| {
            seen.fetch_add(1, Ordering::Relaxed);
            dirsize::DirSize { bytes: 13, partial: false }
        });
        worker.start(vec![
            (0, PathBuf::from("first")),
            (0, PathBuf::from("duplicate")),
            (1, PathBuf::from("second")),
            (1, PathBuf::from("duplicate")),
        ]);
        for _ in 0..2 {
            let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
            assert!(worker.accept(&done));
        }
        assert!(rx.try_recv().is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(!worker.busy());
    }
}
