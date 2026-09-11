// One persistent worker: slow filesystems can delay sizes, never navigation.
use super::{dirsize, events::Event};
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}, mpsc::{channel, Sender}};
use std::time::{Duration, Instant};

pub struct Done {
    pub generation: u64,
    pub row: usize,
    pub result: dirsize::DirSize,
    pub ms: f64,
}

pub struct Worker {
    jobs: Sender<(u64, usize, PathBuf)>,
    generation: Arc<AtomicU64>,
    active: Option<(u64, usize)>,
}

impl Worker {
    pub fn new(events: Sender<Event>) -> Self {
        Self::with_walk(events, |path, cancelled| {
            dirsize::walk_cancellable(path, Instant::now() + Duration::from_millis(dirsize::DEADLINE_MS), &cancelled)
        })
    }

    fn with_walk<F>(events: Sender<Event>, walk: F) -> Self
    where F: Fn(&std::path::Path, &dyn Fn() -> bool) -> dirsize::DirSize + Send + 'static {
        let (jobs, rx) = channel::<(u64, usize, PathBuf)>();
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        // The recursive walker previously used the main thread; retain stack headroom for deep trees.
        std::thread::Builder::new().name("flea-dirsize".into()).stack_size(16 * 1024 * 1024).spawn(move || {
            for (generation, row, path) in rx {
                let t = Instant::now();
                let result = walk(&path, &|| current.load(Ordering::Relaxed) != generation);
                let done = Done { generation, row, result, ms: t.elapsed().as_secs_f64() * 1000.0 };
                if events.send(Event::DirSize(done)).is_err() { break; }
            }
        }).expect("could not start directory size worker");
        Self { jobs, generation, active: None }
    }

    pub fn busy(&self) -> bool { self.active.is_some() }
    pub fn contains(&self, row: usize) -> bool {
        self.active == Some((self.generation.load(Ordering::Relaxed), row))
    }
    pub fn start(&mut self, row: usize, path: PathBuf) {
        assert!(!self.busy());
        let generation = self.generation.load(Ordering::Relaxed);
        if self.jobs.send((generation, row, path)).is_ok() {
            self.active = Some((generation, row));
        }
    }
    pub fn cancel(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
    // Even a cancelled completion releases the single slot, but cannot publish a stale row.
    pub fn accept(&mut self, done: &Done) -> bool {
        if self.active != Some((done.generation, done.row)) { return false; }
        self.active = None;
        self.generation.load(Ordering::Relaxed) == done.generation
    }
}

impl Drop for Worker {
    fn drop(&mut self) { self.cancel(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deep_directory_tree_fits_the_worker_stack() {
        let d = super::super::testdir::TestDir::new("size-deep");
        let mut path = d.path().to_path_buf();
        for _ in 0..1900 {
            path.push("d");
            d.assert_contains(&path);
            std::fs::create_dir(&path).unwrap();
        }
        let (events, rx) = channel();
        let mut worker = Worker::new(events);
        worker.start(0, d.path().to_path_buf());
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(10)).unwrap() else { panic!() };
        assert!(worker.accept(&done));
        assert!(done.result.bytes > 0);
    }

    #[test]
    fn cancelled_running_job_cannot_publish_to_reused_row() {
        let (events, rx) = channel();
        let (entered, entry) = channel();
        let (release, released) = channel();
        let mut worker = Worker::with_walk(events, move |_, cancelled| {
            entered.send(()).unwrap();
            released.recv().unwrap();
            dirsize::DirSize { bytes: 42, partial: cancelled() }
        });
        worker.start(0, PathBuf::new());
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.cancel();
        assert!(worker.busy());
        assert!(!worker.contains(0));
        release.send(()).unwrap();
        let Event::DirSize(old) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(old.result.partial);
        assert!(!worker.accept(&old));
        assert!(!worker.busy());
        worker.start(0, PathBuf::new());
        entry.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!worker.accept(&old));
        assert!(worker.busy());
        release.send(()).unwrap();
        let Event::DirSize(new) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(!new.result.partial);
        assert!(worker.accept(&new));
        assert!(!worker.busy());
    }
}
