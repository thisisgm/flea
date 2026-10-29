// Verbose member boundaries and the current staged file supply absolute, unpacked progress samples.
use super::extractindex::{relative, Index};
use super::opsreq::{OpMsg, PROGRESS_EVERY};
use std::collections::HashSet;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Instant;

// One sample stats at most this many unfinished members while it looks for the one being written.
const SAMPLE_PROBES: usize = 256;

pub(crate) struct Progress {
    pub index: Index,
    done: usize,
    bytes: u64,
    sampled: u64,
    current: String,
    current_complete: bool,
    preview: String,
    preview_bytes: Option<u64>,
    finished: HashSet<String>,
    probe: usize,
    growing: Option<String>,
    reported: Instant,
    protected: bool,
}

impl Progress {
    pub fn new(index: Index, protected: bool) -> Self {
        Self {
            index,
            done: 0,
            bytes: 0,
            sampled: 0,
            current: String::new(),
            current_complete: false,
            preview: String::new(),
            preview_bytes: None,
            finished: HashSet::new(),
            probe: 0,
            growing: None,
            reported: Instant::now() - PROGRESS_EVERY,
            protected,
        }
    }

    // Sample: 7z -bb1 -bso2 stderr "- docs/a.txt", or bsdtar -v stderr "x docs/a.txt".
    pub fn line(&mut self, line: &str, seven: bool, stage: &Path) {
        let name = if seven {
            line.strip_prefix("- ")
        } else {
            line.strip_prefix("x ")
        };
        let Some(name) = name.and_then(relative).filter(|n| !n.is_empty()) else {
            return;
        };
        self.complete(stage);
        self.current = name;
        self.current_complete = false;
        if !seven {
            self.preview.clear();
            self.preview_bytes = None;
            self.complete(stage);
        }
    }

    // Sample: bsdtar -v stderr "x docs/big.iso" before its "\n", while that member is still being written.
    pub fn preview(&mut self, line: &str) {
        let name = line
            .strip_prefix("x ")
            .and_then(relative)
            .unwrap_or_default();
        if name != self.preview {
            self.preview = name;
            self.preview_bytes = None;
        }
    }

    fn complete(&mut self, stage: &Path) {
        if self.current.is_empty() || self.current_complete {
            return;
        }
        let Some(bytes) = staged_size(stage, &self.current) else {
            return;
        };
        if self
            .index
            .members
            .get(&self.current)
            .is_some_and(|size| bytes < *size)
        {
            return;
        }
        self.done += 1;
        self.current_complete = true;
        self.bytes = self.bytes.saturating_add(
            self.index
                .members
                .get(&self.current)
                .copied()
                .unwrap_or(bytes),
        );
        if self.index.known {
            self.finished.insert(self.current.clone());
        }
    }

    fn find_growing(&mut self, stage: &Path) -> Option<(String, u64, u64)> {
        let probes = SAMPLE_PROBES - usize::from(self.growing.is_some());
        if let Some(name) = self.growing.take() {
            if !self.finished.contains(&name) {
                if let Some(size) = self.index.members.get(&name).copied() {
                    if let Some(bytes) =
                        staged_size(stage, &name).filter(|bytes| *bytes > 0 && *bytes < size)
                    {
                        self.growing = Some(name.clone());
                        return Some((name, size, bytes));
                    }
                }
            }
        }
        let count = self.index.members.len();
        for (name, size) in self
            .index
            .members
            .iter()
            .cycle()
            .skip(self.probe)
            .take(count.min(probes))
        {
            self.probe = (self.probe + 1) % count;
            if self.finished.contains(name) {
                continue;
            }
            if let Some(bytes) = staged_size(stage, name).filter(|bytes| *bytes > 0) {
                if bytes < *size {
                    self.growing = Some(name.clone());
                }
                return Some((name.clone(), *size, bytes.min(*size)));
            }
        }
        None
    }

    pub fn sample(&mut self, stage: &Path, id: usize, tx: &Sender<OpMsg>, final_sample: bool) {
        if !final_sample && self.reported.elapsed() < PROGRESS_EVERY {
            return;
        }
        if self.index.members.contains_key(&self.current) {
            self.complete(stage);
        }
        let mut file = self.current.clone();
        let mut item = self.index.members.get(&file).copied().unwrap_or(0);
        let mut active = if self.current_complete {
            0
        } else {
            staged_size(stage, &file).unwrap_or(0)
        };
        if !self.preview.is_empty() {
            if let Some(bytes) = staged_size(stage, &self.preview) {
                let growing = self.preview_bytes.is_some_and(|before| bytes > before);
                self.preview_bytes = Some(bytes);
                let known = self.index.members.get(&self.preview).copied();
                if known.is_some() && !self.finished.contains(&self.preview)
                    || !self.index.known && growing
                {
                    file = self.preview.clone();
                    item = known.unwrap_or(0);
                    active = known.map_or(bytes, |size| bytes.min(size));
                }
            }
        }
        if self.index.known && active == 0 {
            if let Some((name, size, bytes)) = self.find_growing(stage) {
                file = name;
                item = size;
                active = bytes;
            }
        }
        if final_sample {
            self.complete(stage);
            if self.index.known {
                self.done = self.index.total;
                self.bytes = self.index.bytes;
            }
            active = 0;
        }
        self.sampled = self.sampled.max(self.bytes.saturating_add(active));
        if self.index.known {
            self.sampled = self.sampled.min(self.index.bytes);
        }
        if !final_sample
            && (self.done == 0 && self.sampled == 0 || self.protected && self.sampled == 0)
        {
            return;
        }
        self.reported = Instant::now();
        let _ = tx.send(OpMsg::JobEntries {
            id,
            done: self.done,
            total: if self.index.known {
                self.index.total
            } else {
                0
            },
            file: Path::new(&file)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes: self.sampled,
            total_bytes: if self.index.known {
                self.index.bytes
            } else {
                0
            },
            item_bytes: item,
        });
    }
}

// Descriptor-relative O_PATH opens never follow a staged link, including in an intermediate component.
fn staged_size(stage: &Path, name: &str) -> Option<u64> {
    const O_PATH: i32 = 0o10000000;
    let name = relative(name)?;
    if name.is_empty() {
        return None;
    }
    let mut file = File::open(stage).ok()?;
    let mut parts = name.split('/').peekable();
    while let Some(part) = parts.next() {
        if !file.metadata().ok()?.is_dir() {
            return None;
        }
        file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_PATH | crate::oflags::O_NOFOLLOW)
            .open(format!("/proc/self/fd/{}/{}", file.as_raw_fd(), part))
            .ok()?;
        if parts.peek().is_none() {
            let metadata = file.metadata().ok()?;
            return Some(if metadata.is_dir() { 0 } else { metadata.len() });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::symlink;

    #[test]
    fn extraction_samples_never_follow_staged_links_or_parent_components() {
        let root = TestDir::new("extractsamplelinks");
        root.dir("stage");
        root.file("outside", "private bytes");
        symlink(root.path(), root.join("stage/link")).unwrap();
        assert_eq!(staged_size(&root.join("stage"), "../outside"), None);
        assert_eq!(staged_size(&root.join("stage"), "link/outside"), None);
        assert_eq!(staged_size(&root.join("stage"), "/outside"), None);
    }

    #[test]
    fn extraction_unknown_totals_keep_absolute_samples_through_completion() {
        let root = TestDir::new("extractsampleunknown");
        root.file("first", "123");
        root.file("last", "12345");
        let mut progress = Progress::new(Index::default(), false);
        let (tx, rx) = std::sync::mpsc::channel();
        progress.line("- first", true, root.path());
        progress.line("- last", true, root.path());
        progress.sample(root.path(), 41, &tx, true);
        match rx.recv().unwrap() {
            OpMsg::JobEntries {
                done,
                total,
                bytes,
                total_bytes,
                ..
            } => {
                assert_eq!((done, total, bytes, total_bytes), (2, 0, 8, 0));
            }
            _ => panic!("expected extraction sample"),
        }
    }

    #[test]
    fn extraction_password_attempt_has_no_samples_from_empty_failed_files() {
        let root = TestDir::new("extractsamplepassword");
        root.file("first", "");
        root.file("last", "");
        let mut progress = Progress::new(Index::default(), true);
        let (tx, rx) = std::sync::mpsc::channel();
        progress.line("- first", true, root.path());
        progress.line("- last", true, root.path());
        progress.sample(root.path(), 42, &tx, false);
        assert!(rx.try_recv().is_err());
    }
}

#[cfg(test)]
#[path = "extractprogress_live_tests.rs"]
mod live_tests;
