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
    pub phase: String,
    pub check: String,
    pub receipt: String,
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

    // A job keeps its Undo only while the journal still holds the token it recorded.
    pub fn refresh_undo(&mut self, out: &mut impl Write, journal: &super::lazy::Local<super::undo::Journal>) {
        // A journal no request has started holds no token, so the check never starts it.
        let Some(journal) = journal.get() else { return };
        if journal.deferred.is_some() {
            return;
        }
        let expired: Vec<_> = self
            .rows
            .iter_mut()
            .filter_map(|job| {
                if job.undo
                    && job
                        .undo_key
                        .as_ref()
                        .is_some_and(|key| !journal.token_is_current(key))
                {
                    job.undo = false;
                    job.undo_key = None;
                    Some(job.id)
                } else {
                    None
                }
            })
            .collect();
        for id in expired {
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
            phase: String::new(),
            check: String::new(),
            receipt: String::new(),
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
