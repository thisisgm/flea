// The loop owns ordered job state; workers own I/O and report through the existing operation channel.
use super::jobwork::Work;
use super::opscancel::DetachedJobs;
use super::opsreq::OpMsg;
use super::undo::ItemIdentity;
use crate::json::escape;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum State {
    Waiting,
    Running,
    Failed,
    Done,
}

pub(crate) struct Job {
    pub id: usize,
    pub order: usize,
    pub kind: String,
    pub label: String,
    pub target: String,
    pub state: State,
    pub file: String,
    pub done: usize,
    pub total: usize,
    pub bytes: u64,
    pub total_bytes: u64,
    pub item_bytes: u64,
    pub rate: u64,
    pub left: Option<u64>,
    pub reason: String,
    pub output: Vec<String>,
    pub undo: bool,
    pub undo_key: Option<super::undo::UndoToken>,
    pub cancelled: bool,
    pub failed: Vec<(PathBuf, ItemIdentity)>,
    pub work: Option<Work>,
    pub cancel: Arc<AtomicBool>,
    pub drive: Option<String>,
    pub prepared: bool,
    pub started: Instant,
    pub reported: Instant,
    pub moved: u64,
    pub index: Option<usize>,
}

#[derive(Default)]
pub(crate) struct Jobs {
    pub rows: Vec<Job>,
    pub next_order: usize,
}

impl Jobs {
    pub fn submit(
        &mut self,
        out: &mut impl Write,
        id: usize,
        spec: super::jobwork::Spec<'_>,
        work: Work,
        flags: &DetachedJobs,
        tx: &Sender<OpMsg>,
    ) {
        let super::jobwork::Spec { kind, label, target } = spec;
        self.invalidate_undo(out);
        self.rows.retain(|job| job.state != State::Done);
        let cancel = Arc::new(AtomicBool::new(false));
        flags.insert(id, &cancel);
        let total = if kind == "extract" {
            0
        } else {
            work.paths().len()
        };
        let file = work.file();
        let target_name = match &work {
            Work::Send { peer, .. } => peer.clone(),
            _ => target
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|name| name.to_string_lossy().into())
                .unwrap_or_default(),
        };
        self.next_order += 1;
        self.rows.push(Job {
            id,
            order: self.next_order,
            kind: kind.into(),
            label: label.into(),
            target: target_name,
            state: State::Waiting,
            file,
            done: 0,
            total,
            bytes: 0,
            total_bytes: 0,
            item_bytes: 0,
            rate: 0,
            left: None,
            reason: "checking target".into(),
            output: Vec::new(),
            undo: false,
            undo_key: None,
            cancelled: false,
            failed: Vec::new(),
            work: Some(work),
            cancel,
            drive: None,
            prepared: target.is_none(),
            started: Instant::now(),
            reported: Instant::now(),
            moved: 0,
            index: None,
        });
        self.snapshot(out);
        if let Some(target) = target {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let (drive, target) = super::jobdrive::drive_of(&target);
                let _ = tx.send(OpMsg::JobReady { id, drive, target });
            });
        }
        self.advance(out, flags, tx);
    }

    pub fn unfinished(&self) -> usize {
        self.rows
            .iter()
            .filter(|j| matches!(j.state, State::Waiting | State::Running))
            .count()
    }

    pub fn snapshot(&self, out: &mut impl Write) {
        let rows: Vec<_> = self.rows.iter().rev().map(Job::json).collect();
        let newest = self
            .rows
            .iter()
            .rev()
            .find(|j| matches!(j.state, State::Waiting | State::Running));
        writeln!(
            out,
            r#"{{"t":"jobs","jobs":[{}],"unfinished":{},"newest":{},"head":"{}"}}"#,
            rows.join(","),
            self.unfinished(),
            newest
                .map(|j| j.id.to_string())
                .unwrap_or_else(|| "null".into()),
            escape(&newest.map(Job::head).unwrap_or_default())
        )
        .ok();
        out.flush().ok();
    }

    pub fn update(&self, out: &mut impl Write, id: usize) {
        if let Some(job) = self.rows.iter().find(|j| j.id == id) {
            writeln!(
                out,
                r#"{{"t":"job","job":{},"unfinished":{}}}"#,
                job.json(),
                self.unfinished()
            )
            .ok();
            out.flush().ok();
        }
    }

    pub fn invalidate_undo(&mut self, out: &mut impl Write) {
        let ids: Vec<_> = self
            .rows
            .iter_mut()
            .filter(|j| j.undo)
            .map(|j| {
                j.undo = false;
                j.undo_key = None;
                j.id
            })
            .collect();
        for id in ids {
            self.update(out, id);
        }
    }

    pub fn advance(&mut self, out: &mut impl Write, flags: &DetachedJobs, tx: &Sender<OpMsg>) {
        for index in 0..self.rows.len() {
            if self.rows[index].state != State::Waiting {
                continue;
            }
            if self.rows[index].cancel.load(Ordering::Relaxed) {
                let job = &self.rows[index];
                if let Some(work) = &job.work {
                    let line = work.cancelled_line(job.id, job.total);
                    if !line.is_empty() {
                        writeln!(out, "{}", line).ok();
                    }
                }
                self.rows[index].state = State::Done;
                self.rows[index].cancelled = true;
                self.rows[index].reason.clear();
                flags.remove(self.rows[index].id);
                self.update(out, self.rows[index].id);
                continue;
            }
            if !self.rows[index].prepared {
                continue;
            }
            let current = &self.rows[index];
            let blocker = self.rows.iter().find(|j| {
                j.id != current.id
                    && (j.state == State::Running
                        && current.drive.is_some()
                        && j.drive == current.drive
                        || j.state == State::Waiting
                            && j.order < current.order
                            && (!j.prepared || current.drive.is_some() && j.drive == current.drive))
            });
            if let Some(blocker) = blocker {
                let reason = format!(
                    "waiting for the {} to {}",
                    blocker.wait_name(),
                    blocker.target
                );
                if self.rows[index].reason != reason {
                    self.rows[index].reason = reason;
                    self.update(out, self.rows[index].id);
                }
                continue;
            }
            let job = &mut self.rows[index];
            job.state = State::Running;
            job.reason.clear();
            job.started = Instant::now();
            if let Some(work) = &mut job.work {
                work.start(job.id, Arc::clone(&job.cancel), tx.clone());
            }
            self.update(out, self.rows[index].id);
        }
        self.snapshot(out);
    }
}

impl Job {
    fn wait_name(&self) -> &str {
        match self.kind.as_str() {
            "copy" => "copy",
            "move" => "move",
            "compress" => "compression",
            "extract" => "extraction",
            _ => "job",
        }
    }

    pub fn head(&self) -> String {
        if self.state == State::Failed {
            return match self.kind.as_str() {
                "copy" => "Copy failed".into(),
                "move" => "Move failed".into(),
                "send" => "Send failed".into(),
                "compress" => "Compression failed".into(),
                "extract" => "Extraction failed".into(),
                "emptyTrash" => "Emptying Trash failed".into(),
                _ => format!("{} failed", self.label),
            };
        }
        if self.kind == "extract" && self.total == 0 {
            return "Extracting".into();
        }
        if self.kind == "emptyTrash" {
            return "Emptying Trash".into();
        }
        if self.state == State::Waiting {
            return match self.kind.as_str() {
                "copy" => format!("Copy {} items to {}", self.total, self.target),
                "move" => format!("Move {} items to {}", self.total, self.target),
                "compress" => format!("Compress {} items to {}", self.total, self.target),
                _ => self.label.clone(),
            };
        }
        let at = if self.state == State::Done {
            self.done
        } else {
            self.index
                .map(|i| i + 1)
                .unwrap_or(self.done)
                .min(self.total)
        };
        match self.kind.as_str() {
            "copy" => format!("Copying {} of {} to {}", at, self.total, self.target),
            "move" => format!("Moving {} of {} to {}", at, self.total, self.target),
            "extract" => format!("Extracting {} of {}", at, self.total),
            "compress" => format!("Compressing {} items", self.total),
            "send" => format!("Sending {} of {} to {}", at, self.total, self.target),
            _ => format!("{} · {} of {}", self.label, at, self.total),
        }
    }

    pub fn json(&self) -> String {
        let state = match self.state {
            State::Waiting => "waiting",
            State::Running => "running",
            State::Failed => "failed",
            State::Done => "done",
        };
        let output: Vec<_> = self
            .output
            .iter()
            .map(|line| format!("\"{}\"", escape(line)))
            .collect();
        let bytes = !matches!(self.kind.as_str(), "action" | "emptyTrash");
        format!(
            r#"{{"id":{},"kind":"{}","head":"{}","file":"{}","done":{},"total":{},"bytes":{},"totalBytes":{},"itemBytes":{},"rate":{},"left":{},"state":"{}","reason":"{}","output":[{}],"undo":{},"cancelled":{}}}"#,
            self.id,
            escape(&self.kind),
            escape(&self.head()),
            escape(&self.file),
            self.done,
            self.total,
            if bytes {
                self.bytes.to_string()
            } else {
                "null".into()
            },
            if bytes {
                self.total_bytes.to_string()
            } else {
                "null".into()
            },
            self.item_bytes,
            self.rate,
            self.left
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into()),
            state,
            escape(&self.reason),
            output.join(","),
            self.undo,
            self.cancelled
        )
    }
}

impl Jobs {
    // Empty Trash and other independently owned workers adopt their existing id and cancel flag here.
    #[allow(dead_code)]
    pub fn adopt(
        &mut self,
        out: &mut impl Write,
        id: usize,
        kind: &str,
        label: &str,
        total: usize,
        cancel: Arc<AtomicBool>,
    ) {
        self.invalidate_undo(out);
        self.rows.retain(|job| job.state != State::Done);
        self.next_order += 1;
        self.rows.push(Job {
            id,
            order: self.next_order,
            kind: kind.into(),
            label: label.into(),
            target: String::new(),
            state: State::Running,
            file: String::new(),
            done: 0,
            total,
            bytes: 0,
            total_bytes: 0,
            item_bytes: 0,
            rate: 0,
            left: None,
            reason: String::new(),
            output: Vec::new(),
            undo: false,
            undo_key: None,
            cancelled: false,
            failed: Vec::new(),
            work: None,
            cancel,
            drive: None,
            prepared: true,
            started: Instant::now(),
            reported: Instant::now(),
            moved: 0,
            index: None,
        });
        self.snapshot(out);
    }
}
