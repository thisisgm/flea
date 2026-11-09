// Additive job events observe operation messages before the unchanged legacy reporter consumes them.
use super::jobs::{Job, State};
use super::opsdispatch::Ops;
use super::opsreq::{OpMsg, PROGRESS_EVERY};
use crate::json::{field_str, field_usize};
use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub(crate) fn observe(out: &mut impl Write, ops: &mut Ops, message: &OpMsg) {
    let (id, terminal) = match message {
        OpMsg::SendStatus { id, .. }
        | OpMsg::JobEntries { id, .. }
        | OpMsg::Progress { id, .. }
        | OpMsg::Item { id, .. }
        | OpMsg::JobSample { id, .. }
        | OpMsg::JobReady { id, .. } => (*id, false),
        OpMsg::TransferDone { id, .. }
        | OpMsg::MergeDone { id, .. }
        | OpMsg::DetachedDone { id, .. }
        | OpMsg::JobDone { id, .. } => (*id, true),
        #[cfg(test)]
        OpMsg::SlotDone { line } => (field_usize(line, "id").unwrap_or(0), true),
        _ => return,
    };
    let Some(job) = ops.jobs.rows.iter_mut().find(|job| job.id == id) else {
        return;
    };
    let mut force = terminal;
    match message {
        OpMsg::SendStatus {
            phase,
            check,
            receipt,
            rate,
            left,
            ..
        } => {
            force = job.phase != *phase || job.check != *check || job.receipt != *receipt;
            job.phase = phase.clone();
            job.check = check.clone();
            job.receipt = receipt.clone();
            job.rate = *rate;
            job.left = *left;
            if !force {
                return;
            }
        }
        OpMsg::JobReady { result, .. } => {
            if job.state != State::Waiting {
                return;
            }
            match result {
                Ok((drive, target)) => {
                    job.prepared = true;
                    job.drive = Some(drive.clone());
                    job.target = target.clone();
                }
                Err(error) => {
                    job.state = State::Failed;
                    job.reason = error.msg.clone();
                    if let Some(work) = &job.work {
                        let line = work.failed_line(id, job.total, &error.msg);
                        if !line.is_empty() {
                            writeln!(out, "{line}").ok();
                        }
                    }
                    ops.detached.remove(id);
                    ops.jobs.update(out, id);
                }
            }
            ops.jobs.advance(out, &ops.detached, &ops.tx);
            return;
        }
        OpMsg::Progress {
            index,
            name,
            bytes,
            total,
            scanned,
            ..
        } => {
            if *index < job.done {
                job.moved = job.moved.saturating_add(*bytes);
                job.bytes = job.moved;
            } else {
                job.index = Some(*index);
                job.file = name.clone();
                job.item_bytes = *total;
                job.bytes = job.moved.saturating_add(*bytes);
            }
            job.total_bytes = *scanned;
            let seconds = job.started.elapsed().as_secs_f64();
            job.rate = if seconds >= 0.001 {
                (job.bytes as f64 / seconds) as u64
            } else {
                0
            };
            job.left = (job.total_bytes > 0 && job.rate > 0)
                .then(|| job.total_bytes.saturating_sub(job.bytes) / job.rate);
        }
        OpMsg::Item {
            index,
            name,
            ok,
            err,
            ..
        } => {
            job.done = job.done.max(index + 1);
            job.file = name.clone();
            if *ok {
                if job.index == Some(*index) {
                    job.moved = job.moved.saturating_add(if job.item_bytes > 0 {
                        job.item_bytes
                    } else {
                        job.bytes.saturating_sub(job.moved)
                    });
                    job.bytes = job.moved;
                    job.item_bytes = 0;
                }
            } else if job.reason.is_empty() {
                job.reason = err.clone();
            }
        }
        OpMsg::JobEntries {
            done,
            total,
            file,
            bytes,
            total_bytes,
            item_bytes,
            ..
        } => {
            job.done = *done;
            job.total = *total;
            job.index = Some(*done);
            job.file = file.clone();
            job.bytes = *bytes;
            job.total_bytes = *total_bytes;
            job.item_bytes = *item_bytes;
            let elapsed = job.started.elapsed().as_secs_f64();
            if job.phase.is_empty() {
                job.rate = if elapsed > 0.001 {
                    (*bytes as f64 / elapsed) as u64
                } else {
                    0
                };
                job.left = if job.kind == "emptyTrash" {
                    (*done > 0).then(|| {
                        ((*total - (*done).min(*total)) as f64 * elapsed / *done as f64) as u64
                    })
                } else {
                    (job.rate > 0 && *total_bytes > 0)
                        .then(|| total_bytes.saturating_sub(*bytes) / job.rate)
                };
            }
        }
        OpMsg::JobSample {
            done,
            total,
            file,
            output,
            ..
        } => {
            job.done = *done;
            job.total = *total;
            job.index = Some(*done);
            job.file = file.clone();
            job.output = output.clone();
        }
        OpMsg::TransferDone {
            ok,
            failed,
            cancelled,
            retry,
            ..
        }
        | OpMsg::MergeDone {
            ok,
            failed,
            cancelled,
            retry,
            ..
        } => {
            job.done = *ok;
            job.failed = retry.clone();
            job.cancelled = *cancelled;
            job.state = if *failed > 0 {
                State::Failed
            } else {
                State::Done
            };
            if *failed > 0 && job.reason.is_empty() {
                job.reason = "Some items could not be transferred.".into();
            }
            job.undo = job.undo_key.is_some() && *failed == 0;
        }
        OpMsg::JobDone {
            failed,
            error,
            line,
            ..
        } => {
            job.failed = failed.clone();
            job.reason = error.clone();
            job.state = if error.is_empty() {
                State::Done
            } else {
                State::Failed
            };
            job.cancelled = if job.kind == "emptyTrash" {
                crate::json::field_bool(line, "cancelled")
            } else {
                job.cancel.load(Ordering::Relaxed)
            };
            if job.cancelled {
                job.state = State::Done;
                job.reason.clear();
            }
            if job.state == State::Done && !job.cancelled && job.total > 0 {
                job.done = job.total;
            }
        }
        #[cfg(test)]
        OpMsg::SlotDone { line } => archive_done(job, line),
        OpMsg::DetachedDone { line, .. } => archive_done(job, line),
        _ => {}
    }
    if terminal {
        ops.detached.remove(id);
        let undo = job.undo;
        if undo {
            job.undo = false;
            ops.jobs.invalidate_undo(out);
            if let Some(job) = ops.jobs.rows.iter_mut().find(|j| j.id == id) {
                job.undo = true;
            }
        }
        force = true;
    }
    if let Some(job) = ops.jobs.rows.iter_mut().find(|j| j.id == id) {
        if force || job.reported.elapsed() >= PROGRESS_EVERY {
            job.reported = Instant::now();
            ops.jobs.update(out, id);
        }
    }
    if terminal {
        ops.jobs.advance(out, &ops.detached, &ops.tx);
    }
}

fn archive_done(job: &mut Job, line: &str) {
    let success = crate::json::field_bool(line, "ok");
    job.reason = field_str(line, "err").unwrap_or_default();
    job.state = if success { State::Done } else { State::Failed };
    job.cancelled = job.cancel.load(Ordering::Relaxed);
    if job.cancelled {
        job.state = State::Done;
        job.reason.clear();
    }
    if success {
        job.done = job.total;
    }
}

pub(crate) fn request(out: &mut impl Write, ops: &mut Ops, line: &str) -> super::undo::Reached {
    let op = field_str(line, "op").unwrap_or_else(|| "list".into());
    let id = field_usize(line, "id").unwrap_or(0);
    let error = match op.as_str() {
        "list" => {
            ops.jobs.snapshot(out);
            return Default::default();
        }
        "cancel" => {
            ops.detached.cancel(id);
            ops.live.cancel(id);
            ops.jobs.advance(out, &ops.detached, &ops.tx);
            return Default::default();
        }
        "dismiss" => {
            if ops
                .jobs
                .rows
                .iter()
                .any(|j| j.id == id && matches!(j.state, State::Failed | State::Done))
            {
                ops.jobs.rows.retain(|j| j.id != id);
                ops.jobs.snapshot(out);
                return Default::default();
            }
            "Only a finished or failed job can be dismissed.".to_string()
        }
        "retry" => {
            let Some(job) = ops
                .jobs
                .rows
                .iter_mut()
                .find(|j| j.id == id && j.state == State::Failed)
            else {
                refuse(out, id, "Only a failed job can be retried.");
                return Default::default();
            };
            match retry(job) {
                Ok(()) => {
                    // A retry narrows the work to its failed items, which it names again before it queues.
                    if let Some(work) = &job.work {
                        let roots = super::jobview::roots_line(id, true, &work.roots());
                        writeln!(out, "{roots}").ok();
                    }
                    ops.jobs.next_order += 1;
                    job.order = ops.jobs.next_order;
                    ops.detached.insert(id, &job.cancel);
                    if let Some(target) = job.work.as_ref().filter(|_| !job.prepared).and_then(|w| w.target()) {
                        job.reason = "checking target".into();
                        super::jobstart::check_target(id, target, &ops.tx);
                    }
                    ops.jobs.advance(out, &ops.detached, &ops.tx);
                    return Default::default();
                }
                Err(error) => error,
            }
        }
        "undo" => {
            if ops.jobs.rows.iter().any(|j| j.id == id && j.undo) && ops.jobs.unfinished() == 0 {
                let key = ops
                    .jobs
                    .rows
                    .iter_mut()
                    .find(|j| j.id == id)
                    .and_then(|j| j.undo_key.take());
                if let Some(key) = key {
                    let reached = super::opsdispatch::do_undo_matching(out, ops, Some(&key));
                    ops.jobs.invalidate_undo(out);
                    return reached;
                }
            }
            "That job can no longer be undone.".to_string()
        }
        _ => "Unknown jobs request.".into(),
    };
    refuse(out, id, &error);
    Default::default()
}

// A job whose target check failed never started, so it retries whole and checks its target again.
fn retry(job: &mut Job) -> Result<(), String> {
    let work = job.work.as_mut().ok_or("This job cannot be retried.")?;
    if job.prepared {
        job.total = work.retry(&job.failed)?;
    }
    if job.kind == "extract" {
        job.total = 0;
    }
    job.cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    job.state = State::Waiting;
    job.done = 0;
    job.bytes = 0;
    job.total_bytes = 0;
    job.item_bytes = 0;
    job.cancelled = false;
    job.rate = 0;
    job.left = None;
    job.moved = 0;
    job.index = None;
    job.reason.clear();
    job.failed.clear();
    job.output.clear();
    job.phase.clear();
    job.check.clear();
    job.receipt.clear();
    Ok(())
}

fn refuse(out: &mut impl Write, id: usize, error: &str) {
    writeln!(
        out,
        r#"{{"t":"joberror","id":{id},"error":"{}"}}"#,
        crate::json::escape(error)
    )
    .ok();
    out.flush().ok();
}
