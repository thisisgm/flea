//! The folder-state writer: one worker sends patches in order and acknowledges each one it finishes.
use flea::jsondoc::Json;
use std::{
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
};

// A default writer only counts, so no test reaches the real state file.
#[derive(Default)]
pub struct Writer {
    queue: Option<mpsc::Sender<(String, String)>>,
    live: bool,
    pub sent: usize,
    // Patches handed to the worker, the same set `done` counts as it finishes them.
    queued: usize,
    // Patches the worker has not finished yet, which a settings reread must not undo.
    owed: Vec<(usize, Json)>,
    done: Arc<AtomicUsize>,
    // A held writer keeps its worker's end open, so its sends succeed without a worker.
    #[cfg(test)]
    held: Option<mpsc::Receiver<(String, String)>>,
}

impl Writer {
    pub fn live() -> Self {
        Self {
            live: true,
            ..Self::default()
        }
    }

    // A live writer whose worker never runs; the test finishes writes through `done`.
    #[cfg(test)]
    pub fn held(done: Arc<AtomicUsize>) -> Self {
        let (queue, patches) = mpsc::channel();
        Self {
            queue: Some(queue),
            live: true,
            done,
            held: Some(patches),
            ..Self::default()
        }
    }

    pub fn write(&mut self, patch: String, failure: String) {
        self.sent += 1;
        if !self.live {
            return;
        }
        let owed = flea::jsondoc::parse(&patch).ok();
        if self.queue.is_none() {
            let (queue, patches) = mpsc::channel::<(String, String)>();
            let done = self.done.clone();
            let spawned = thread::Builder::new()
                .name("flea-folder-state".into())
                .spawn(move || {
                    for (patch, failure) in patches {
                        let ok = Command::new(crate::window::startup::binary())
                            .args(["--ui-state", &patch])
                            .output()
                            .is_ok_and(|output| output.status.success());
                        if !ok {
                            eprintln!("flea-gui: {failure}");
                        }
                        done.fetch_add(1, Ordering::Release);
                    }
                });
            match spawned {
                Ok(_) => self.queue = Some(queue),
                Err(error) => eprintln!("flea-gui: folder state worker did not start: {error}"),
            }
        }
        let Some(queue) = &self.queue else {
            return;
        };
        // Only a patch the worker received is owed, so `owed` and `done` count the same patches.
        if queue.send((patch, failure)).is_err() {
            eprintln!("flea-gui: folder state worker is gone; the next write starts another");
            self.queue = None;
            return;
        }
        self.queued += 1;
        if let Some(owed) = owed {
            self.owed.push((self.queued, owed));
        }
    }

    // Unfinished patches in write order; a finished one drops, because the file now holds its outcome.
    pub fn owed(&mut self) -> Vec<Json> {
        let done = self.done.load(Ordering::Acquire);
        self.owed.retain(|(seq, _)| *seq > done);
        self.owed.iter().map(|(_, patch)| patch.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m9_g1rows_05_writer_owes_each_patch_until_its_worker_finishes() {
        let done = Arc::new(AtomicUsize::new(0));
        let mut writer = Writer::held(done.clone());
        writer.write(
            r#"{"folderSorts":{"/A":{"key":"size","reverse":true}}}"#.into(),
            String::new(),
        );
        writer.write(r#"{"folderSorts":{"/B":null}}"#.into(), String::new());
        assert_eq!(writer.owed().len(), 2);
        done.store(1, Ordering::Release);
        let owed = writer.owed();
        assert_eq!(owed.len(), 1);
        assert_eq!(
            owed[0],
            flea::jsondoc::parse(r#"{"folderSorts":{"/B":null}}"#).unwrap()
        );
        done.store(2, Ordering::Release);
        assert!(writer.owed().is_empty());
        let mut counting = Writer::default();
        counting.write(r#"{"hidden":true}"#.into(), String::new());
        assert_eq!(counting.sent, 1);
        assert!(counting.owed().is_empty(), "a counting writer owes nothing");
    }

    #[test]
    fn m9_fix_j2_writer_owes_nothing_for_a_patch_its_worker_never_received() {
        let done = Arc::new(AtomicUsize::new(0));
        let mut writer = Writer::held(done.clone());
        writer.held = None;
        writer.write(r#"{"folderSorts":{"/A":null}}"#.into(), String::new());
        assert_eq!(writer.sent, 1);
        assert_eq!(
            writer.queued,
            done.load(Ordering::Acquire),
            "a failed send is not queued"
        );
        assert!(writer.owed().is_empty(), "a failed send owes nothing");
        assert!(
            writer.queue.is_none(),
            "a dead worker is replaced on the next write"
        );
    }
}
