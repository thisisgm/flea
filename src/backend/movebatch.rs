// Cross-device file moves in batches: per-file fsyncs as today, one folder confirm per batch.
use crate::backend::copyfile::{copy_any, remove_any, Progress};
use crate::backend::durable::{fsync_dir, Durability, DIR_UNCONFIRMED};
use crate::backend::opsreq::{OpMsg, PROGRESS_EVERY};
use crate::backend::undo::{self, ItemIdentity, Step};
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;

// 64 items keep one folder fsync per batch while bounding one unconfirmed batch.
pub(crate) const BATCH_ITEMS: usize = 64;
// rename(2) sets EXDEV when the two paths are on different filesystems, the one failure that means "copy instead".
const EXDEV: i32 = 18;

// Test builds only: force the copy path on one filesystem, so tests drive the batch without two mounts.
#[cfg(test)]
thread_local! {
    static FORCE_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
fn force_copy() -> bool {
    FORCE_COPY.with(|v| v.get())
}
#[cfg(test)]
pub(crate) fn test_set_force_copy(force: bool) {
    FORCE_COPY.with(|v| v.set(force));
}
// Cleared on drop, so a failing test never leaks the copy path into the next one on its thread.
#[cfg(test)]
pub(crate) struct ForceCopyGuard;
#[cfg(test)]
impl ForceCopyGuard {
    pub(crate) fn hold() -> Self {
        test_set_force_copy(true);
        ForceCopyGuard
    }
}
#[cfg(test)]
impl Drop for ForceCopyGuard {
    fn drop(&mut self) {
        test_set_force_copy(false);
    }
}
#[cfg(not(test))]
fn force_copy() -> bool {
    false
}

// Test builds only: fail the source re-check, so the unverified-source arm drives without a sick filesystem.
#[cfg(test)]
thread_local! {
    static INSPECT_FAIL: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}
// The errno the re-check answers with; 0 lets the real lstat run.
#[cfg(test)]
pub(crate) fn test_set_inspect_fail(errno: i32) {
    INSPECT_FAIL.with(|v| v.set(errno));
}
// Cleared on drop, so a failing test never leaks the failure into the next one on its thread.
#[cfg(test)]
pub(crate) struct InspectFailGuard;
#[cfg(test)]
impl InspectFailGuard {
    pub(crate) fn hold() -> Self {
        Self::hold_errno(EIO_ERRNO)
    }
    pub(crate) fn hold_errno(errno: i32) -> Self {
        test_set_inspect_fail(errno);
        InspectFailGuard
    }
}
#[cfg(test)]
impl Drop for InspectFailGuard {
    fn drop(&mut self) {
        test_set_inspect_fail(0);
    }
}

// ESTALE means the server dropped the handle, so the source is gone the same way ENOENT is.
pub(crate) const ESTALE: i32 = 116;
// EIO, the transient error a sick NFS or SMB mount answers an lstat with.
#[cfg(test)]
const EIO_ERRNO: i32 = 5;

// A transient lstat error answers EIO-shaped, the shape a sick NFS or SMB mount gives back.
fn inspect_source(path: &Path) -> Result<ItemIdentity, std::io::Error> {
    #[cfg(test)]
    {
        let injected = INSPECT_FAIL.with(|v| v.get());
        if injected != 0 {
            return Err(std::io::Error::from_raw_os_error(injected));
        }
    }
    path.symlink_metadata().map(|meta| ItemIdentity::record(&meta))
}

// ENOENT or ESTALE means the bytes at the source name are already gone.
fn source_gone(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(ESTALE)
}

// One landed copy waiting on its batch's folder confirm; the source is still whole.
pub(crate) struct PendingMove {
    index: usize,
    name: String,
    src: PathBuf,
    dst: PathBuf,
    source: ItemIdentity,
    manifest: Option<crate::backend::copymanifest::Writer>,
}

pub(crate) struct MoveBatch {
    pending: Vec<PendingMove>,
}

// What one moving item did: answered now, or copied and waiting on its batch's confirm.
pub(crate) enum MoveOutcome {
    Done(Result<(), FleaError>),
    Deferred,
}

#[derive(Default)]
pub(crate) struct CloseCounts {
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: bool,
    pub kept_ok: usize,
}

#[cfg(test)]
#[path = "movebatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "usbrca_tests.rs"]
mod usbrca_tests;

impl MoveBatch {
    pub(crate) fn new() -> Self {
        MoveBatch { pending: Vec::new() }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    // A batch closes after BATCH_ITEMS items, so one folder fsync never covers an unbounded move.
    pub(crate) fn full(&self) -> bool {
        self.pending.len() >= BATCH_ITEMS
    }

    pub(crate) fn push(&mut self, item: PendingMove) {
        self.pending.push(item);
    }
}

// One terminal line per batched item, sent when its batch closes rather than when its copy landed.
fn send_item(tx: &Sender<OpMsg>, id: usize, index: usize, name: &str, ok: bool, err: &str) {
    let _ = tx.send(OpMsg::Item { id, index, name: name.to_string(), ok, err: err.to_string() });
}

// A copy the batch cannot finish, journaled as the partial one_item journals for any failed copy.
fn journal_partial(
    src: &Path,
    dst: &Path,
    source: &ItemIdentity,
    manifest: Option<crate::backend::copymanifest::Writer>,
    steps: &mut Vec<Step>,
    err: &mut FleaError,
) -> Result<(), FleaError> {
    let (manifest, loud) = crate::backend::copymanifest::finish_loud(manifest);
    if let Some(e) = loud {
        err.msg = crate::backend::ops::manifest_failed(&err.msg, &e);
    }
    steps.push(undo::copied_partial(src, dst, source.clone(), manifest)?);
    Ok(())
}

// The folders a batch filled, confirmed once; held files settle first so sources go after the bytes.
fn confirm_batch(durability: &mut Durability, dsts: &[PathBuf]) -> Result<(), FleaError> {
    let failed = if durability.durable {
        durability.flush_dirs_for_many(dsts).is_err()
    } else {
        // No touched set without a durability context, so each filled parent confirms once.
        let mut parents: Vec<&Path> = Vec::new();
        for dst in dsts {
            if let Some(parent) = dst.parent() {
                if !parents.contains(&parent) {
                    parents.push(parent);
                }
            }
        }
        let mut failed = false;
        for parent in parents {
            if fsync_dir(parent).is_err() {
                failed = true;
            }
        }
        failed
    };
    if failed {
        let first = dsts.first().map(|dst| dst.to_string_lossy().to_string()).unwrap_or_default();
        return Err(FleaError { where_: "move".to_string(), path: first, msg: DIR_UNCONFIRMED.to_string() });
    }
    Ok(())
}

// A same-filesystem rename answers at once; a cross-device copy stages into the batch.
pub(crate) fn land_move(
    id: usize,
    index: usize,
    name: &str,
    src: &Path,
    dst: &Path,
    source: ItemIdentity,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
    settled: &AtomicU64,
    steps: &mut Vec<Step>,
    durability: &mut Durability,
    batch: &mut MoveBatch,
) -> MoveOutcome {
    if force_copy() {
        return stage_copy(id, index, name, src, dst, source, cancel, tx, settled, steps, durability, batch);
    }
    match crate::backend::renamecompat::rename_noreplace(src, dst) {
        Ok(()) => {
            // One rename rewrote two directory entries, so both folders are confirmed.
            if let Some(parent) = dst.parent() {
                durability.touch(parent);
            }
            if let Some(parent) = src.parent() {
                durability.touch(parent);
            }
            match undo::moved(src, dst, source) {
                Ok(step) => {
                    steps.push(step);
                    MoveOutcome::Done(Ok(()))
                }
                Err(e) => MoveOutcome::Done(Err(e)),
            }
        }
        Err(e) if e.raw_os_error() == Some(EXDEV) => {
            stage_copy(id, index, name, src, dst, source, cancel, tx, settled, steps, durability, batch)
        }
        Err(e) => MoveOutcome::Done(Err(from_io("rename", &dst.to_string_lossy(), &e))),
    }
}

// The EXDEV half of land_move: copy with today's file fsync, then wait on the batch's confirm.
pub(crate) fn stage_copy(
    id: usize,
    index: usize,
    name: &str,
    src: &Path,
    dst: &Path,
    source: ItemIdentity,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
    settled: &AtomicU64,
    steps: &mut Vec<Step>,
    durability: &mut Durability,
    batch: &mut MoveBatch,
) -> MoveOutcome {
    let mut last = Instant::now() - PROGRESS_EVERY;
    let mut sink = |done: u64, total: u64| {
        if last.elapsed() < PROGRESS_EVERY {
            return;
        }
        last = Instant::now();
        let _ = tx.send(OpMsg::Progress {
            id,
            index,
            name: name.to_string(),
            bytes: done,
            total,
            scanned: settled.load(Ordering::Relaxed),
        });
    };
    let mut p = Progress {
        cancel,
        on_bytes: &mut sink,
        partial: None,
        tree: None,
        manifest: crate::backend::copymanifest::writer_for_move(src, dst),
        durability: Some(durability),
        for_move: true,
    };
    let outcome = copy_any(src, dst, &mut p);
    let partial = p.partial.take();
    let manifest = p.manifest.take();
    drop(p);
    match outcome {
        Ok(()) => {
            batch.push(PendingMove {
                index,
                name: name.to_string(),
                src: src.to_path_buf(),
                dst: dst.to_path_buf(),
                source,
                manifest,
            });
            MoveOutcome::Deferred
        }
        Err(mut err) => {
            if let Some(path) = partial {
                if let Err(record) = journal_partial(src, &path, &source, manifest, steps, &mut err) {
                    return MoveOutcome::Done(Err(record));
                }
            }
            MoveOutcome::Done(Err(err))
        }
    }
}

// A source edited or replaced while its copy waited on the batch: both names stay.
pub(crate) const BOTH_KEPT: &str = "changed during the move; both kept";

// A confirm the drive refused: every source stays whole and each copy journals as a partial.
fn journal_unconfirmed(
    batch: &mut MoveBatch,
    id: usize,
    tx: &Sender<OpMsg>,
    steps: &mut Vec<Step>,
) -> (CloseCounts, Vec<(PathBuf, ItemIdentity)>) {
    let mut counts = CloseCounts::default();
    let mut retry = Vec::new();
    for item in batch.pending.drain(..) {
        let mut err = FleaError {
            where_: "move".to_string(),
            path: item.dst.to_string_lossy().to_string(),
            msg: DIR_UNCONFIRMED.to_string(),
        };
        let err = match journal_partial(&item.src, &item.dst, &item.source, item.manifest, steps, &mut err) {
            Ok(()) => err,
            Err(record) => record,
        };
        counts.failed += 1;
        retry.push((item.src.clone(), item.source.clone()));
        send_item(tx, id, item.index, &item.name, false, &err.msg);
    }
    (counts, retry)
}

// One staged copy becomes a move once its folder confirms, unless its source changed meanwhile.
fn finish_item(
    item: &PendingMove,
    id: usize,
    tx: &Sender<OpMsg>,
    steps: &mut Vec<Step>,
    durability: &mut Durability,
    counts: &mut CloseCounts,
    retry: &mut Vec<(PathBuf, ItemIdentity)>,
) {
    // An unverifiable source keeps both names; a gone one journals nothing, so undo keeps the only bytes left.
    match inspect_source(&item.src) {
        // The source still holds the bytes its copy took, so the move can finish.
        Ok(current) if item.source.unchanged_for_move(&current) => {}
        // A source that changed under its copy keeps both names; undo then only ever removes the copy.
        Ok(_) => {
            let reason = BOTH_KEPT.to_string();
            match undo::copied(&item.src, &item.dst, item.source.clone()) {
                Ok(step) => {
                    steps.push(step);
                    send_item(tx, id, item.index, &item.name, false, &reason);
                }
                Err(e) => send_item(tx, id, item.index, &item.name, false, &e.msg),
            }
            counts.failed += 1;
            retry.push((item.src.clone(), item.source.clone()));
            return;
        }
        // A source already gone leaves its staged copy in place with no journal step behind it.
        Err(e) if source_gone(&e) => {
            let reason = format!("{} was gone before the move finished; the copy stays", item.name);
            send_item(tx, id, item.index, &item.name, false, &reason);
            counts.failed += 1;
            retry.push((item.src.clone(), item.source.clone()));
            return;
        }
        // A source that cannot be re-checked keeps both names the same way, with the error as its reason.
        Err(e) => {
            let reason = format!("could not verify {}: {}; both kept", item.name, crate::error::io_message(&e));
            match undo::copied(&item.src, &item.dst, item.source.clone()) {
                Ok(step) => {
                    steps.push(step);
                    send_item(tx, id, item.index, &item.name, false, &reason);
                }
                Err(e) => send_item(tx, id, item.index, &item.name, false, &e.msg),
            }
            counts.failed += 1;
            retry.push((item.src.clone(), item.source.clone()));
            return;
        }
    };
    match remove_any(&item.src) {
        Ok(()) => {
            // Logged in test builds, so a batch pins its first removal after its folder fsync.
            #[cfg(test)]
            crate::backend::durable::test_log("remove");
            if let Some(parent) = item.src.parent() {
                durability.touch(parent);
            }
            match undo::moved(&item.src, &item.dst, item.source.clone()) {
                Ok(step) => {
                    steps.push(step);
                    counts.ok += 1;
                    // A deferred merge item counts only where it landed, so its kept name rides its own ok.
                    counts.kept_ok += usize::from(super::merge::kept_name(&item.dst, &item.name));
                    send_item(tx, id, item.index, &item.name, true, "");
                }
                Err(e) => {
                    counts.failed += 1;
                    retry.push((item.src.clone(), item.source.clone()));
                    send_item(tx, id, item.index, &item.name, false, &e.msg);
                }
            }
        }
        Err(e) => {
            counts.failed += 1;
            retry.push((item.src.clone(), item.source.clone()));
            send_item(tx, id, item.index, &item.name, false, &e.msg);
        }
    }
}

// A full batch, a failure or the end: one folder confirm, then each source goes with a re-check.
pub(crate) fn close_normal(
    batch: &mut MoveBatch,
    id: usize,
    tx: &Sender<OpMsg>,
    steps: &mut Vec<Step>,
    durability: &mut Durability,
) -> (CloseCounts, Vec<(PathBuf, ItemIdentity)>) {
    if batch.pending.is_empty() {
        return (CloseCounts::default(), Vec::new());
    }
    let dsts: Vec<PathBuf> = batch.pending.iter().map(|item| item.dst.clone()).collect();
    if confirm_batch(durability, &dsts).is_err() {
        return journal_unconfirmed(batch, id, tx, steps);
    }
    let mut counts = CloseCounts::default();
    let mut retry = Vec::new();
    for item in batch.pending.drain(..) {
        finish_item(&item, id, tx, steps, durability, &mut counts, &mut retry);
    }
    (counts, retry)
}

// A cancel completes what already copied: one confirm, then each source goes with a re-check.
pub(crate) fn close_cancelled(
    batch: &mut MoveBatch,
    id: usize,
    tx: &Sender<OpMsg>,
    steps: &mut Vec<Step>,
    durability: &mut Durability,
) -> (CloseCounts, Vec<(PathBuf, ItemIdentity)>) {
    let mut counts = CloseCounts { cancelled: true, ..CloseCounts::default() };
    if batch.pending.is_empty() {
        return (counts, Vec::new());
    }
    let dsts: Vec<PathBuf> = batch.pending.iter().map(|item| item.dst.clone()).collect();
    if confirm_batch(durability, &dsts).is_err() {
        let (mut counts, retry) = journal_unconfirmed(batch, id, tx, steps);
        counts.cancelled = true;
        return (counts, retry);
    }
    let mut retry = Vec::new();
    for item in batch.pending.drain(..) {
        finish_item(&item, id, tx, steps, durability, &mut counts, &mut retry);
    }
    (counts, retry)
}
