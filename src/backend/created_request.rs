//! Optional creation correlation leaves each original operation, journal and wire reply intact.
use super::{busy, land_mkdir, menu_sources, pending_busy_for, report_op, Ops, SlowClaim};
use crate::backend::{
    iomount::{
        internal_failure, mount_body, slow_sentence, slow_write_with, SlowWrite, SLOW_MKDIR,
    },
    opsreq::{op_err, run_duplicate_requested, OpMsg},
    proto::{error_line, slow_line},
    undo::Step,
};
use crate::{error::FleaError, json::escape};
use std::{
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

pub(crate) fn wrap(request: usize, inner: OpMsg) -> OpMsg {
    if request == 0 {
        inner
    } else {
        OpMsg::CreationReceipt {
            request,
            inner: Box::new(inner),
        }
    }
}

fn publish(
    out: &mut impl Write,
    request: usize,
    kind: &str,
    result: Result<String, String>,
    legacy: Vec<u8>,
) {
    if request == 0 {
        out.write_all(&legacy).ok();
    } else {
        // Tag only creation lines so old clients still receive their unchanged operation payload.
        for line in String::from_utf8_lossy(&legacy).lines() {
            if crate::json::field_str(line, "t").as_deref() == Some(kind) {
                writeln!(
                    out,
                    "{},\"request\":{request}}}",
                    line.trim_end_matches('}')
                )
                .ok();
            } else {
                writeln!(out, "{line}").ok();
            }
        }
        let (ok, field, value) = match result {
            Ok(path) => (true, "path", path),
            Err(error) => (false, "error", error),
        };
        writeln!(out, r#"{{"t":"menuaction","op":"creation","id":{request},"kind":"{kind}","ok":{ok},"{field}":"{}"}}"#, escape(&value)).ok();
    }
    out.flush().ok();
}

pub(super) fn refused(out: &mut impl Write, request: usize, kind: &str, legacy: Vec<u8>) {
    let error = String::from_utf8_lossy(&legacy)
        .lines()
        .find_map(|line| crate::json::field_str(line, "msg"))
        .unwrap_or_else(|| "The creation request was refused.".into());
    publish(out, request, kind, Err(error), legacy);
}

pub(crate) fn duplicate(
    out: &mut impl Write,
    ops: &mut Ops,
    path: &str,
    menu_id: usize,
    request: usize,
) {
    let mut legacy = Vec::new();
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(&mut legacy, "duplicate");
        refused(out, request, "duplicated", legacy);
        return;
    }
    let selection = match menu_sources(ops, menu_id) {
        Ok(selection) => selection,
        Err(message) => {
            writeln!(
                legacy,
                "{}",
                error_line(&op_err("duplicate", path, &message))
            )
            .ok();
            refused(out, request, "duplicated", legacy);
            return;
        }
    };
    let mut sources = vec![PathBuf::from(path)];
    if let Some(items) = &selection {
        sources.extend(items.iter().map(|item| item.path.clone()));
    }
    if pending_busy_for(&mut legacy, ops, "duplicate", &sources, &[]) {
        refused(out, request, "duplicated", legacy);
        return;
    }
    ops.claim_transfer();
    let tx = ops.tx.clone();
    let owned = path.to_owned();
    thread::spawn(move || run_duplicate_requested(owned, tx, selection, request));
}

pub(super) fn mkdir_with<F>(
    out: &mut impl Write,
    ops: &mut Ops,
    parent: &str,
    request: usize,
    deadline: Duration,
    work: F,
) -> Vec<PathBuf>
where
    F: FnOnce() -> Result<(PathBuf, Vec<Step>), FleaError> + Send + 'static,
{
    let key = PathBuf::from(parent);
    let mut legacy = Vec::new();
    if pending_busy_for(&mut legacy, ops, "mkdir", std::slice::from_ref(&key), &[]) {
        refused(out, request, "made", legacy);
        return Vec::new();
    }
    match slow_write_with(&key, &mount_body(), "mkdir", deadline, work) {
        SlowWrite::Ready(result) => {
            let receipt = result
                .as_ref()
                .map(|(path, _)| path.to_string_lossy().into_owned())
                .map_err(|error| error.msg.clone());
            let touched = land_mkdir(&mut legacy, ops, result);
            publish(out, request, "made", receipt, legacy);
            touched
        }
        SlowWrite::Slow { mount, rx } => {
            writeln!(
                out,
                "{}",
                slow_line("mkdir", parent, &slow_sentence(&mount, SLOW_MKDIR))
            )
            .ok();
            out.flush().ok();
            let id = ops.claim_id();
            ops.pending.push(SlowClaim {
                id,
                path: parent.to_owned(),
                mount,
            });
            let tx = ops.tx.clone();
            let failed = internal_failure("mkdir", Path::new(parent));
            thread::spawn(move || {
                let result = rx.recv().unwrap_or(Err(failed));
                let _ = tx.send(wrap(request, OpMsg::MkdirDone { id, result }));
            });
            Vec::new()
        }
    }
}

pub(super) fn report(out: &mut impl Write, ops: &mut Ops, request: usize, inner: OpMsg) {
    let (kind, result) = match &inner {
        OpMsg::Duplicated { ok, path, err, .. } => (
            "duplicated",
            if *ok {
                Ok(path.clone())
            } else {
                Err(err.clone())
            },
        ),
        OpMsg::MkdirDone { result, .. } => (
            "made",
            result
                .as_ref()
                .map(|(path, _)| path.to_string_lossy().into_owned())
                .map_err(|error| error.msg.clone()),
        ),
        _ => unreachable!("only creation results carry creation receipts"),
    };
    let mut legacy = Vec::new();
    report_op(&mut legacy, ops, inner);
    // One event publishes both replies before quit can observe the completed operation slot.
    publish(out, request, kind, result, legacy);
}

#[cfg(test)]
#[path = "created_request_tests.rs"]
mod tests;
