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
    // Patches handed to the current worker, the same set its `done` counts as it finishes them.
    queued: usize,
    // Patches no worker has finished yet, which a settings reread must not undo and a new worker resends.
    owed: Vec<Owed>,
    done: Arc<AtomicUsize>,
    // Every patch in write order, so a test can tell a sort write from a path or tab strip write.
    #[cfg(test)]
    pub patches: Vec<String>,
    // A held writer keeps its worker's end open, so its sends succeed without a worker.
    #[cfg(test)]
    held: Option<mpsc::Receiver<(String, String)>>,
    // A held writer's replacement worker is another held channel, never the real binary.
    #[cfg(test)]
    hold: bool,
}

// `seq` is the patch's place in its worker's queue, 0 while no live worker has it.
struct Owed {
    seq: usize,
    json: Option<Json>,
    patch: String,
    failure: String,
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
            hold: true,
            ..Self::default()
        }
    }

    // The current worker's count; a restart replaces it, so a test takes it again after each write.
    #[cfg(test)]
    pub fn done(&self) -> Arc<AtomicUsize> {
        self.done.clone()
    }

    pub fn write(&mut self, patch: String, failure: String) {
        self.sent += 1;
        #[cfg(test)]
        self.patches.push(patch.clone());
        if !self.live {
            return;
        }
        self.retire();
        self.owed.push(Owed {
            seq: 0,
            json: flea::jsondoc::parse(&patch).ok(),
            patch,
            failure,
        });
        if self.queue.is_some() {
            if self.send(self.owed.len() - 1) {
                return;
            }
            eprintln!("flea-gui: folder state worker is gone; starting another");
        }
        self.restart();
    }

    // Each worker gets its own count, so a dead worker's late finishes land on its own and retire nothing.
    fn restart(&mut self) {
        self.queue = None;
        self.queued = 0;
        self.done = Arc::new(AtomicUsize::new(0));
        for owed in &mut self.owed {
            owed.seq = 0;
        }
        if !self.start() {
            return;
        }
        for index in 0..self.owed.len() {
            if !self.send(index) {
                eprintln!("flea-gui: folder state worker is gone; the next write starts another");
                self.queue = None;
                return;
            }
        }
    }

    fn start(&mut self) -> bool {
        #[cfg(test)]
        if self.hold {
            let (queue, patches) = mpsc::channel();
            self.queue = Some(queue);
            self.held = Some(patches);
            return true;
        }
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
        self.queue.is_some()
    }

    // Only a patch the worker received gets a place, so `seq` and `done` count the same patches.
    fn send(&mut self, index: usize) -> bool {
        let Some(queue) = &self.queue else {
            return false;
        };
        let owed = &mut self.owed[index];
        if queue
            .send((owed.patch.clone(), owed.failure.clone()))
            .is_err()
        {
            return false;
        }
        self.queued += 1;
        owed.seq = self.queued;
        true
    }

    // A finished patch drops, because the file now holds its outcome.
    fn retire(&mut self) {
        let done = self.done.load(Ordering::Acquire);
        self.owed.retain(|owed| owed.seq == 0 || owed.seq > done);
    }

    // The patches that write a folder or global sort; lastPath and lastTabs writes are not sort writes.
    #[cfg(test)]
    pub fn sort_writes(&self) -> usize {
        self.patches
            .iter()
            .filter(|patch| patch.contains("\"folderSorts\"") || patch.contains("\"sort\""))
            .count()
    }

    // Unfinished patches in write order.
    pub fn owed(&mut self) -> Vec<Json> {
        self.retire();
        self.owed
            .iter()
            .filter_map(|owed| owed.json.clone())
            .collect()
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
    fn m9_fix_r2_j2_writer_resends_a_dead_workers_unfinished_patches() {
        let first = Arc::new(AtomicUsize::new(0));
        let mut writer = Writer::held(first.clone());
        writer.write(r#"{"folderSorts":{"/A":null}}"#.into(), String::new());
        writer.write(r#"{"folderSorts":{"/B":null}}"#.into(), String::new());
        first.store(1, Ordering::Release);
        writer.held = None;
        writer.write(r#"{"folderSorts":{"/C":null}}"#.into(), String::new());
        let parse = |text| flea::jsondoc::parse(text).unwrap();
        let (b, c) = (
            parse(r#"{"folderSorts":{"/B":null}}"#),
            parse(r#"{"folderSorts":{"/C":null}}"#),
        );
        assert_eq!(writer.owed(), vec![b.clone(), c.clone()]);
        let received: Vec<String> = writer
            .held
            .as_ref()
            .unwrap()
            .try_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(
            received,
            [
                r#"{"folderSorts":{"/B":null}}"#,
                r#"{"folderSorts":{"/C":null}}"#
            ],
            "the new worker gets the unfinished patch, then the new one"
        );
        first.fetch_add(2, Ordering::Release);
        assert_eq!(
            writer.owed().len(),
            2,
            "the dead worker's count retires nothing"
        );
        assert!(
            !Arc::ptr_eq(&first, &writer.done()),
            "the new worker counts on its own"
        );
        writer.done().store(1, Ordering::Release);
        assert_eq!(writer.owed(), vec![c]);
        writer.done().store(2, Ordering::Release);
        assert!(writer.owed().is_empty());
    }
}
