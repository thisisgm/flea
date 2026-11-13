// Reviewed deletion shares the transfer slot, cancellation flag and progress stream.
use super::*;
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::PROGRESS_EVERY;
use std::cell::Cell;
use std::io::Write;

pub(crate) fn request(out: &mut impl Write, ops: &mut Ops, line: String) {
    let deleting = field_str(&line, "op").as_deref() == Some("delete");
    let replies = ops.tx.clone();
    if deleting && (ops.live.running().is_some() || !ops.settling.is_empty()) {
        writeln!(out, r#"{{"t":"trashbrowse","id":{},"op":"delete","ok":false,"stale":false,"error":"An operation is already running."}}"#,
            field_usize(&line, "id").unwrap_or(0)).ok();
        out.flush().ok();
        return;
    }
    let job = if deleting {
        let (id, cancel) = ops.claim_transfer();
        ops.jobs
            .adopt(out, id, "emptyTrash", "Emptying Trash", 0, cancel.clone());
        writeln!(
            out,
            r#"{{"t":"trashdeletestarted","id":{},"requestId":{}}}"#,
            id,
            field_usize(&line, "id").unwrap_or(0)
        )
        .ok();
        writeln!(out, "{}", progress_line(id, "checking", 0, 0, 0, ("", 0))).ok();
        out.flush().ok();
        Some((
            Progress {
                id,
                tx: replies.clone(),
                count: Cell::new(0),
                nodes: Cell::new(0),
                last: Cell::new(None),
                named: Default::default(),
            },
            Cancellation::operation(cancel),
        ))
    } else {
        None
    };
    let pending = ops.pending.iter().map(|held| held.mount.clone()).collect();
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let browser = ops
        .trashbrowser
        .get_or_insert_with(|| TrashBrowser::new(replies));
    let Err(error) = browser.enqueue(line, pending, body, job) else {
        return;
    };
    ops.trashbrowser = None;
    let (line, _, _, _, progress) = error.0;
    let refusal = format!(
        r#"{{"t":"trashbrowse","id":{},"op":"{}","ok":false,"stale":false,"error":"Trash worker unavailable."}}"#,
        field_usize(&line, "id").unwrap_or(0),
        escape(&field_str(&line, "op").unwrap_or_default())
    );
    match progress {
        Some(progress) => {
            crate::backend::opsdispatch::report_op(out, ops, progress.finish(refusal, false));
        }
        None => {
            writeln!(out, "{}", refusal).ok();
        }
    }
    out.flush().ok();
}

impl TrashBrowser {
    pub(super) fn enqueue(
        &self,
        line: String,
        pending: Vec<PathBuf>,
        body: String,
        job: Option<(Progress, Cancellation)>,
    ) -> Result<(), Box<std::sync::mpsc::SendError<BrowserRequest>>> {
        // Every request supersedes browser reads; a delete keeps its independent operation flag.
        let next = self.cancellation.next();
        let (cancellation, job) = match job {
            Some((progress, cancellation)) => (cancellation, Some(progress)),
            None => (next, None),
        };
        self.tx.send((line, cancellation, pending, body, job)).map_err(Box::new)
    }
}

fn progress_line(
    id: usize,
    phase: &str,
    done: usize,
    total: usize,
    checked: usize,
    (name, size): (&str, u64),
) -> String {
    format!(
        r#"{{"t":"transferprogress","id":{id},"index":0,"name":"{}","itemSize":{size},"bytes":{done},"total":{total},"unit":"items","phase":"{phase}","checked":{checked}}}"#,
        escape(name)
    )
}

pub(super) struct Progress {
    id: usize,
    tx: Sender<OpMsg>,
    pub count: Cell<usize>,
    pub nodes: Cell<usize>,
    last: Cell<Option<Instant>>,
    /// The phase and the last item a sample named in it, with its size.
    named: std::cell::RefCell<(String, String, u64)>,
}
impl Progress {
    pub fn sample(
        &self,
        phase: &str,
        done: usize,
        checked: usize,
        item: Option<&Detail>,
        force: bool,
    ) {
        self.remember(phase, item);
        let now = Instant::now();
        if force
            || self
                .last
                .get()
                .map_or(true, |last| now.duration_since(last) >= PROGRESS_EVERY)
        {
            self.last.set(Some(now));
            let (name, size) = {
                let named = self.named.borrow();
                (named.1.clone(), named.2)
            };
            let _ = self.tx.send(OpMsg::JobEntries {
                id: self.id,
                done,
                total: self.nodes.get(),
                file: name.clone(),
                bytes: 0,
                total_bytes: 0,
                item_bytes: size,
            });
            let _ = self.tx.send(OpMsg::Meta {
                line: progress_line(
                    self.id,
                    phase,
                    done,
                    self.nodes.get(),
                    checked,
                    (&name, size),
                ),
            });
        }
    }
    // A sample between two items names none; the card keeps the last item this phase named, so its item line never blanks.
    fn remember(&self, phase: &str, item: Option<&Detail>) {
        let mut named = self.named.borrow_mut();
        if named.0 != phase {
            *named = (phase.into(), String::new(), 0);
        }
        if let Some(item) = item {
            let shown = flea::bytename::display(&item.item.original);
            let name = shown.rsplit('/').next().unwrap_or("");
            if named.1 != name {
                named.1 = name.into();
            }
            named.2 = item.backing.as_ref().map_or(0, Reviewed::reviewed_bytes);
        }
    }
    pub fn finish(self, line: String, cancelled: bool) -> OpMsg {
        let ok = field_usize(&line, "done").unwrap_or(0);
        let failed = field_usize(&line, "failed").unwrap_or(0);
        let skipped = self.count.get().saturating_sub(ok + failed);
        let note = field_str(&line, "error").unwrap_or_default();
        let cancelled = if line.contains("\"cancelled\":") {
            field_bool(&line, "cancelled")
        } else {
            cancelled
        };
        let done = crate::backend::opsreq::transferdone_line(
            self.id,
            crate::backend::opsreq::TransferCounts { ok, failed, skipped },
            cancelled,
            &[],
            false,
            &note,
            &[],
        );
        OpMsg::JobDone {
            id: self.id,
            failed: Vec::new(),
            error: note,
            line: format!("{done}\n{line}"),
        }
    }
}

impl Session {
    pub(super) fn delete(&mut self, line: &str) -> Result<String, Failure> {
        if let Some(confirmation) = self
            .confirmation
            .as_ref()
            .filter(|c| c.token == field_usize(line, "token").unwrap_or(0))
        {
            if self.targets_blocked(&confirmation.targets)? {
                return Err("an operation is already running".into());
            }
        }
        if !self.valid(field_usize(line, "token").unwrap_or(0))? {
            return Err(Failure::Stale(
                Refusal::new(Outcome::ConfirmAgain, "Trash changed; confirm again.").logged(),
            ));
        }
        self.cancellation.check()?;
        let confirmation = self
            .confirmation
            .take()
            .ok_or("Trash confirmation expired.")?;
        let progress = self.job.as_ref();
        if let Some(progress) = progress {
            progress.sample("deleting", 0, confirmation.count, None, true);
        }
        Ok(delete_with(
            &confirmation,
            &crate::backend::trashdelete::recovery_root(),
            &self.cancellation,
            |done, item, force| {
                if let Some(progress) = progress {
                    progress.sample("deleting", done, confirmation.count, item, force);
                }
            },
        ))
    }
}

#[cfg(test)]
pub(super) fn delete_confirmed(
    confirmation: &Confirmation,
    recovery: &Result<PathBuf, String>,
) -> String {
    delete_with(
        confirmation,
        recovery,
        &Cancellation::default(),
        |_, _, _| {},
    )
}

pub(super) fn delete_with(
    confirmation: &Confirmation,
    recovery: &Result<PathBuf, String>,
    cancel: &Cancellation,
    mut progress: impl FnMut(usize, Option<&Detail>, bool),
) -> String {
    let mut thumbnails = ThumbnailPurge(Vec::new());
    let mut done = 0;
    let mut failures = Vec::new();
    let mut offset = 0;
    let mut reached = 0;
    let mut folded = 0;
    let mut cancelled = false;
    let mut removed = 0usize;
    loop {
        if cancel.check().is_err() {
            cancelled |= reached < confirmation.count;
            break;
        }
        let bytes = match confirmation.items.records().next(&mut offset) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(error) => {
                damaged(&error);
                let (rows, left) = left_unread(confirmation, reached);
                folded += left.saturating_sub(rows.len());
                failures.extend(rows);
                break;
            }
        };
        reached += 1;
        let text = match record_text(bytes) {
            Ok(text) => text,
            Err(_) => {
                failures.push(unread(None));
                continue;
            }
        };
        match load_detail(&text, &confirmation.trees) {
            Ok(item) => {
                progress(removed, Some(&item), reached == 1);
                let result = recovery
                    .as_ref()
                    .map_err(|error| Refusal::said(error.clone()))
                    .and_then(|root| {
                        let reviewed = item
                            .backing
                            .as_ref()
                            .ok_or_else(|| unchecked(Op::Delete))?;
                        let result = reviewed.delete_cancellable(root, cancel, || {
                            removed += 1;
                            progress(removed, Some(&item), false);
                        });
                        thumbnails.record(reviewed, &result);
                        result
                    });
                match result {
                    Ok(()) => done += 1,
                    Err(error) if error.outcome() == &Outcome::Cancelled => {
                        cancelled = true;
                        break;
                    }
                    Err(error) => {
                        cancelled |= cancel.check().is_err();
                        failures.push(failure(
                            &item.item,
                            &surviving_identity(&item),
                            &error.shown(),
                        ));
                    }
                }
            }
            Err(_) => failures.push(unread(record_item(&text).ok())),
        }
        progress(removed, None, false);
    }
    // What the listing left out goes last, in the same thumbnail purge, and a cancel leaves it.
    let leftover = match (cancelled, recovery) {
        (false, Ok(root)) => confirmation.sweep.remove(root, cancel, &mut thumbnails),
        _ => 0,
    };
    let failed = failures.len() + folded;
    progress(removed, None, true);
    format!(
        r#""done":{},"failed":{},"failures":[{}],"skipped":{},"cancelled":{},"leftover":{}"#,
        done,
        failed,
        failures.join(","),
        confirmation.count.saturating_sub(done + failed),
        cancelled,
        leftover
    )
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
