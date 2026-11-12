// A stalled device can occupy two readers, while only the newest queued read waits behind them.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

pub(super) type Work = Box<dyn FnOnce() + Send>;

// A held device keeps at most this many let-go reads; another device still gets its own reader.
const READERS_PER_DEVICE: usize = 2;

fn held(reading: &[PathBuf], mount: &Path) -> usize {
    reading
        .iter()
        .filter(|read| read.as_path() == mount)
        .count()
}

#[derive(Default)]
struct Queue {
    pending: Option<(PathBuf, Work)>,
    workers: usize,
    // The device of every read in progress, one entry per busy worker.
    reading: Vec<PathBuf>,
    spare: bool,
    closed: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
}

#[derive(Default)]
pub(super) struct Scanner {
    shared: Arc<Shared>,
}

impl Scanner {
    pub(super) fn abandon(&self) {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .spare = true;
    }

    pub(super) fn send(&self, mount: PathBuf, work: Work) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let idle = queue.workers > queue.reading.len();
        let on_device = held(&queue.reading, &mount);
        queue.pending = Some((mount, work));
        // Only a let-go read earns a new thread, so ordinary lists keep their one reader and their order.
        if !idle && (queue.workers == 0 || (queue.spare && on_device < READERS_PER_DEVICE)) {
            queue.workers += 1;
            let shared = self.shared.clone();
            std::thread::spawn(move || loop {
                let (mount, work) = {
                    let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
                    loop {
                        if queue.closed {
                            return;
                        }
                        // An idle reader leaves a device at its cap queued until one of that device's readers frees.
                        let ready = queue.pending.as_ref().is_some_and(|(mount, _)| {
                            held(&queue.reading, mount) < READERS_PER_DEVICE
                        });
                        if let Some((mount, work)) = ready.then(|| queue.pending.take()).flatten() {
                            queue.reading.push(mount.clone());
                            break (mount, work);
                        }
                        queue = shared
                            .changed
                            .wait(queue)
                            .unwrap_or_else(PoisonError::into_inner);
                    }
                };
                work();
                let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(at) = queue.reading.iter().position(|read| *read == mount) {
                    queue.reading.swap_remove(at);
                }
            });
        }
        queue.spare = false;
        self.shared.changed.notify_one();
    }

    #[cfg(test)]
    pub(super) fn workers(&self) -> usize {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .workers
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        queue.closed = true;
        queue.pending = None;
        self.shared.changed.notify_all();
    }
}
