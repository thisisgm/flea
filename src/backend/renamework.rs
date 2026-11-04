// Rename work and ordered journal publication never wait on the request dispatcher.
use super::opsdispatch::{forget_pending, Ops, SlowClaim};
use super::opsreq::{renamed_line, OpMsg};
use super::proto::error_line;
use super::undo::{Entry, Journal, Step};
use crate::error::FleaError;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) type Outcome = Result<(PathBuf, Vec<Step>), FleaError>;

// The placeholder records every journal mutation while its history belongs to the publisher.
pub(crate) enum Publication {
    Push(Entry),
    Reserve(usize),
    Fill(usize, Entry),
    Release,
    Rename(usize, Outcome),
}

pub(crate) fn start(
    ops: &mut Ops,
    path: String,
    deadline: Duration,
    work: impl FnOnce() -> Outcome + Send + 'static,
) {
    let id = ops.claim_id();
    ops.journal.reserve(id);
    let body = super::iomount::mount_body();
    let mount = super::iomount::mount_key(std::path::Path::new(&path), &body);
    ops.pending.push(SlowClaim {
        id,
        path: path.clone(),
        mount: mount.clone(),
    });
    let tx = ops.tx.clone();
    let timer = tx.clone();
    let (finished, wait) = std::sync::mpsc::channel();
    let failure_path = PathBuf::from(&path);
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .unwrap_or_else(|_| Err(super::iomount::internal_failure("rename", &failure_path)));
        let _ = finished.send(());
        let _ = tx.send(OpMsg::RenameDone { id, result });
    });
    std::thread::spawn(move || {
        if wait.recv_timeout(deadline) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
            let line = super::proto::slow_line(
                "rename",
                &path,
                &super::iomount::slow_sentence(&mount, super::iomount::SLOW_RENAME),
            );
            let _ = timer.send(OpMsg::RenameSlow { id, line });
        }
    });
}

pub(crate) fn publish(ops: &mut Ops, command: Publication) {
    if let Some(queue) = ops.journal.deferred.as_mut() {
        queue.push(command);
        return;
    }
    spawn(ops, vec![command]);
}

pub(crate) fn flush_pending(ops: &mut Ops) {
    if ops.journal.deferred.is_none() && ops.journal.has_pending() {
        spawn(ops, Vec::new());
    }
}

fn spawn(ops: &mut Ops, commands: Vec<Publication>) {
    let mut placeholder = Journal::new();
    placeholder.deferred = Some(Vec::new());
    let mut journal = std::mem::replace(&mut *ops.journal, placeholder);
    let tx = ops.tx.clone();
    std::thread::spawn(move || {
        // Entries pending before this run get one bounded attempt; an entry these commands queue already spent its own.
        let retry = journal.has_pending();
        let mut renames = Vec::new();
        for command in commands {
            match command {
                Publication::Push(entry) => journal.push(entry),
                Publication::Reserve(id) => journal.reserve(id),
                Publication::Fill(id, entry) => journal.fill(id, entry),
                Publication::Release => journal.release_held(),
                Publication::Rename(id, result) => {
                    #[cfg(test)]
                    super::renametiming::mark("journal_begin");
                    let steps = result
                        .as_ref()
                        .map(|(_, steps)| steps.clone())
                        .unwrap_or_default();
                    journal.fill(
                        id,
                        Entry {
                            op: "rename".into(),
                            steps,
                        },
                    );
                    #[cfg(test)]
                    super::renametiming::mark("journal_end");
                    renames.push((id, result));
                }
            }
        }
        if retry {
            journal.retry_pending();
        }
        let _ = tx.send(OpMsg::JournalDone {
            journal: Box::new(journal),
            renames,
        });
    });
}

pub(crate) fn published(
    out: &mut impl Write,
    ops: &mut Ops,
    journal: Journal,
    renames: Vec<(usize, Outcome)>,
) {
    let queued = ops
        .journal
        .deferred
        .take()
        .expect("only the current publisher returns its history");
    *ops.journal = journal;
    for (id, result) in renames {
        match result {
            Ok((path, _)) => {
                writeln!(out, "{}", renamed_line(true, &path.to_string_lossy())).ok();
            }
            Err(error) => {
                writeln!(out, "{}", error_line(&error)).ok();
            }
        }
        forget_pending(ops, id);
    }
    out.flush().ok();
    #[cfg(test)]
    {
        super::renametiming::mark("reply_written");
        super::renametiming::end();
    }
    if !queued.is_empty() {
        spawn(ops, queued);
    }
}

// Existing dispatch tests consume the same asynchronous events as the real loop, including the slow notice.
#[cfg(test)]
pub(crate) fn test_request(
    out: &mut Vec<u8>,
    ops: &mut Ops,
    request: impl FnOnce(&mut Vec<u8>, &mut Ops) -> Vec<PathBuf>,
) -> Vec<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel();
    let previous = std::mem::replace(&mut ops.tx, tx.into()).clone();
    let before = ops.pending.len();
    let touched = request(out, ops);
    while ops.pending.len() > before || ops.journal.deferred.is_some() {
        let message = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("rename worker reports");
        let slow = matches!(message, OpMsg::RenameSlow { .. });
        super::opsdispatch::report_op(out, ops, message);
        if slow {
            break;
        }
    }
    ops.tx = previous.clone().into();
    std::thread::spawn(move || {
        for message in rx {
            if previous.send(message).is_err() {
                break;
            }
        }
    });
    touched
}

#[cfg(test)]
#[path = "renamework_tests.rs"]
mod tests;
