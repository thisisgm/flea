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
    // Patches the worker has not finished yet, which a settings reread must not undo.
    owed: Vec<(usize, Json)>,
    done: Arc<AtomicUsize>,
    // Every patch in write order, so a test can tell a sort write from a path or tab strip write.
    #[cfg(test)]
    pub patches: Vec<String>,
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
        let (queue, _) = mpsc::channel();
        Self {
            queue: Some(queue),
            live: true,
            done,
            ..Self::default()
        }
    }

    pub fn write(&mut self, patch: String, failure: String) {
        self.sent += 1;
        #[cfg(test)]
        self.patches.push(patch.clone());
        if !self.live {
            return;
        }
        if let Ok(owed) = flea::jsondoc::parse(&patch) {
            self.owed.push((self.sent, owed));
        }
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
            if spawned.is_ok() {
                self.queue = Some(queue);
            }
        }
        if let Some(queue) = &self.queue {
            let _ = queue.send((patch, failure));
        }
    }

    // The patches that write a folder or global sort; lastPath and lastTabs writes are not sort writes.
    #[cfg(test)]
    pub fn sort_writes(&self) -> usize {
        self.patches
            .iter()
            .filter(|patch| patch.contains("\"folderSorts\"") || patch.contains("\"sort\""))
            .count()
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
}
