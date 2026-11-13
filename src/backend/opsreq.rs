// The operations request layer: the response lines, and the one thread an operation runs on.
use crate::backend::collide::{cancelled, Place, Policy, Skipping, CANCELLED};
use crate::backend::ops;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::error::{from_io, io_message, FleaError};
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

// One progress line per operation at most this often, so a run of small files shares one window, see opspace.rs.
pub(crate) const PROGRESS_EVERY: Duration = Duration::from_millis(150);
#[path = "opspace.rs"]
pub(crate) mod opspace;
use opspace::Pace;
#[path = "transferitem.rs"]
mod transferitem;

// Test builds only: set the cancel flag at this item, past the loop-top check, so a mid-item cancel is deterministic.
#[cfg(test)]
thread_local! {
    static CANCEL_AT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn test_set_cancel_at(index: Option<usize>) {
    CANCEL_AT.with(|v| v.set(index));
}
// Cleared on drop, so a failing test never cancels the next one on its thread.
#[cfg(test)]
pub(crate) struct CancelAtGuard;
#[cfg(test)]
impl CancelAtGuard {
    pub(crate) fn hold(index: usize) -> Self {
        test_set_cancel_at(Some(index));
        CancelAtGuard
    }
}
#[cfg(test)]
impl Drop for CancelAtGuard {
    fn drop(&mut self) {
        test_set_cancel_at(None);
    }
}
#[cfg(test)]
fn cancel_at_fires(index: usize) -> bool {
    CANCEL_AT.with(|v| v.get() == Some(index))
}

#[path = "opmsg.rs"]
mod opmsg;
pub use opmsg::OpMsg;

// A transfer's three tallies, named so the done line cannot take one for another.
pub(crate) struct TransferCounts {
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
}

// What a menu transfer re-checks before it runs: the selection, the destination, and the collision answer.
pub(crate) struct TransferChecks<P> {
    pub selection: Option<Vec<super::menu_actions::Selected>>,
    pub destination: Option<super::menu_actions::Selected>,
    pub collision: P,
}

// One item of a transfer: where it sits in the request, and which file goes where.
pub(crate) struct TransferItem<'a> {
    pub id: usize,
    pub index: usize,
    pub name: &'a str,
    pub src: &'a Path,
    pub dst: &'a Path,
    pub source: ItemIdentity,
}

// One link request's whole answer: its counts, its journal steps and the first failure's words.
pub struct LinkOutcome {
    pub ok: usize,
    pub failed: usize,
    pub skipped: usize,
    pub steps: Vec<Step>,
    pub first_err: String,
    // Each stranded replace sentence, reported on the linked line ahead of the undo hint.
    pub note: String,
}

// moving is the verb the request actually resolved to, so the client names the operation from the
// wire rather than from a clipboard it may have already spent or never owned.
pub fn transferstarted_line(id: usize, n: usize, moving: bool) -> String {
    format!(r#"{{"t":"transferstarted","id":{},"n":{},"moving":{}}}"#, id, n, moving)
}

// total is 0 for a directory, whose size the item's own progress never knows in advance. scanned is
// the whole batch's, 0 until the sweep beside the copy settles on one: directive 45's counting state.
pub fn transferprogress_line(id: usize, index: usize, name: &str, bytes: u64, total: u64, scanned: u64) -> String {
    format!(
        r#"{{"t":"transferprogress","id":{},"index":{},"name":"{}","bytes":{},"total":{},"scanned":{}}}"#,
        id, index, escape(name), bytes, total, scanned
    )
}

// err rides only on a failure, so a successful item's line carries no empty field to reason about.
pub fn transferitem_line(id: usize, index: usize, name: &str, ok: bool, err: &str) -> String {
    // The item name is the exact text the transfer acted on; the progress line shows its display spelling.
    let name = &flea::bytename::display(name);
    if ok {
        return format!(
            r#"{{"t":"transferitem","id":{},"index":{},"name":"{}","ok":true}}"#,
            id, index, escape(name)
        );
    }
    format!(
        r#"{{"t":"transferitem","id":{},"index":{},"name":"{}","ok":false,"err":"{}"}}"#,
        id, index, escape(name), escape(err)
    )
}

// refused rides only when a drive refused an entry, so every other transfer's line is byte-identical to before.
pub fn transferdone_line(id: usize, counts: TransferCounts, cancelled: bool, retry: &[(PathBuf, ItemIdentity)], durable: bool,
                         note: &str, refused: &[(String, String)]) -> String {
    let TransferCounts { ok, failed, skipped } = counts;
    let paths: Vec<_> = retry.iter().map(|(path, _)| format!("\"{}\"", escape(&path.exact_text()))).collect();
    format!(
        r#"{{"t":"transferdone","id":{},"ok":{},"failed":{},"skipped":{},"cancelled":{},"retryPaths":[{}],"durable":{},"note":"{}"{}}}"#,
        id, ok, failed, skipped, cancelled, paths.join(","), durable, escape(note), refused_field(refused)
    )
}

// Sample output: refused_field(&[]) is "", one entry adds ,"refused":[{"name":"a/b","err":"..."}] to a done line.
pub(crate) fn refused_field(refused: &[(String, String)]) -> String {
    let named: Vec<_> = refused.iter().map(|(name, err)| format!(r#"{{"name":"{}","err":"{}"}}"#, escape(name), escape(err))).collect();
    if named.is_empty() { String::new() } else { format!(r#","refused":[{}]"#, named.join(",")) }
}

// Permission repair may change ctime; retry selects the original inode and link kind, never a replacement at its name.
pub fn retain_retry(retry: &[(PathBuf, ItemIdentity)], matches: &mut Vec<(&str, usize)>) {
    let originals: HashMap<_, _> = retry.iter().map(|(path, identity)| (path.as_path(), identity)).collect();
    matches.retain(|(path, _)| originals.get(&*flea::bytename::view(path)).is_some_and(|original|
        ItemIdentity::inspect(&flea::bytename::view(path)).is_ok_and(|current| original.same_item(&current))));
}

// The line names the request it answers and the paths that moved; err rides only on a failure.
pub fn trashed_line(request: usize, ok: usize, failed: usize, reason: &str, moved: &[String]) -> String {
    let moved: Vec<String> = moved.iter().map(|path| format!("\"{}\"", escape(path))).collect();
    if reason.is_empty() {
        return format!(r#"{{"t":"trashed","id":{},"ok":{},"failed":{},"moved":[{}]}}"#, request, ok, failed, moved.join(","));
    }
    format!(r#"{{"t":"trashed","id":{},"ok":{},"failed":{},"err":"{}","moved":[{}]}}"#, request, ok, failed, escape(reason), moved.join(","))
}

pub fn renamed_line(ok: bool, path: &str) -> String {
    format!(r#"{{"t":"renamed","ok":{},"path":"{}"}}"#, ok, escape(path))
}

pub fn duplicated_line(ok: bool, path: &str) -> String {
    format!(r#"{{"t":"duplicated","ok":{},"path":"{}"}}"#, ok, escape(path))
}

pub fn made_line(ok: bool, path: &str) -> String {
    format!(r#"{{"t":"made","ok":{},"path":"{}"}}"#, ok, escape(path))
}

pub fn undone_line(op: &str, ok: bool) -> String {
    format!(r#"{{"t":"undone","op":"{}","ok":{}}}"#, escape(op), ok)
}

// A destination Flea will not create as a side effect, checked once before any item is touched.
pub fn usable_dest(dest: &str) -> Result<PathBuf, FleaError> {
    checked_dest(dest, true)
}

pub(crate) fn directory_dest(dest: &str) -> Result<PathBuf, FleaError> {
    checked_dest(dest, false)
}

fn checked_dest(dest: &str, require_writable: bool) -> Result<PathBuf, FleaError> {
    let p = flea::bytename::exact(dest);
    if !p.is_absolute() {
        return Err(op_err("transfer", dest, "a destination must be an absolute path"));
    }
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let owned = p.clone();
    let dest_owned = dest.to_string();
    super::iomount::call(&p, &body, "transfer", move || match owned.metadata() {
        Ok(m) if m.is_dir() && (!require_writable || crate::backend::ops::dir_writable(&owned)) => Ok(owned),
        Ok(m) if m.is_dir() => Err(op_err("transfer", &dest_owned, "that folder cannot be written")),
        Ok(_) => Err(op_err("transfer", &dest_owned, "the destination is not a folder")),
        Err(e) => Err(from_io("transfer", &dest_owned, &e)),
    })
    .unwrap_or_else(Err)
}

pub fn op_err(where_: &str, path: &str, msg: &str) -> FleaError {
    FleaError { where_: where_.to_string(), path: path.to_string(), msg: msg.to_string() }
}

fn base_name(p: &Path) -> String {
    p.file_name().map(|n| n.exact_text().to_string()).unwrap_or_default()
}

pub const INTO_ITSELF: &str = "cannot move or copy a folder into itself";
pub const ALREADY_THERE: &str = "already in that folder";

// Copy or move, one top-level item at a time, reporting each item's own terminal line as it lands.
// The unchecked transfer, which only the tests that drive it directly call: every product path goes
// through run_transfer_checked, which takes the selection and destination it has to re-check.
#[cfg_attr(not(test), allow(dead_code))]
pub fn run_transfer(
    id: usize,
    moving: bool,
    paths: Vec<String>,
    dest: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<OpMsg>,
) {
    run_transfer_checked(id, moving, paths, dest, cancel, tx, TransferChecks { selection: None, destination: None, collision: Policy::default() })
}

// The sweep's own lifetime: it walks while the transfer is in this function and stops when it leaves.
struct SweepGuard {
    flag: Arc<AtomicBool>,
}

impl Drop for SweepGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Relaxed);
    }
}

// The scan a batch's total comes from, on its own thread so the first byte never waits for it. It
// publishes into the cell every progress sample reads, and publishes nothing at all when it stopped
// early: the card shows no total rather than a floor, and no time left with it. Directive 50: the
// walk answers to the transfer and not to a clock, so it runs until the copy cancels or finishes.
pub(crate) fn spawn_total(paths: &[String], dest: &Path, skipping: Option<Skipping>, cancel: &Arc<AtomicBool>, settled: &Arc<AtomicU64>, sweeping: &Arc<AtomicBool>) {
    let paths: Vec<String> = paths.to_vec();
    let dest = dest.to_path_buf();
    let cancel = Arc::clone(cancel);
    let settled = Arc::clone(settled);
    let sweeping = Arc::clone(sweeping);
    std::thread::spawn(move || {
        let done = |flags: &(Arc<AtomicBool>, Arc<AtomicBool>)| {
            flags.0.load(Ordering::Relaxed) || !flags.1.load(Ordering::Relaxed)
        };
        let flags = (cancel, sweeping);
        let mut bytes = 0u64;
        for raw in &paths {
            if done(&flags) {
                return;
            }
            if !counted(raw, &dest, skipping.as_ref()) {
                continue;
            }
            let meta = match std::fs::symlink_metadata(raw) {
                Ok(meta) => meta,
                Err(_) => return,
            };
            if !meta.is_dir() {
                // A symlink is copied as the link, so its own size is what it costs and not its target's.
                bytes += meta.len();
                continue;
            }
            let seen = crate::backend::dirsize::walk_while(&flea::bytename::view(raw), &|| done(&flags));
            if seen.partial {
                return;
            }
            bytes += seen.bytes;
        }
        if done(&flags) {
            return;
        }
        settled.store(bytes, Ordering::Relaxed);
    });
}

// A skipped item copies nothing, so counting its bytes would hold the time left above what the batch will ever move.
fn counted(raw: &str, dest: &Path, skipping: Option<&Skipping>) -> bool {
    let src = flea::bytename::view(raw); let src = &*src;
    !skipping.is_some_and(|skipping| skipping.leaves(src, &dest.join(flea::bytename::view(&base_name(src)))))
}

pub(crate) fn run_transfer_checked(
    id: usize, moving: bool, paths: Vec<String>, dest: PathBuf,
    cancel: Arc<AtomicBool>, tx: Sender<OpMsg>, checks: TransferChecks<Policy>,
) {
    let TransferChecks { selection, destination, collision: policy } = checks;
    // Directive 45: a batch gets a time left once it knows what it is copying, and a tree's size is
    // not known without a walk. This is that walk, beside the copy rather than before it.
    let settled = Arc::new(AtomicU64::new(0));
    // The walk outlives nothing: the guard clears the flag when this function leaves, a panic included.
    let sweep = SweepGuard { flag: Arc::new(AtomicBool::new(true)) };
    let policy = policy.for_batch(&paths);
    let merging = policy.is_merge();
    spawn_total(&paths, &dest, policy.skipping(), &cancel, &settled, &sweep.flag);
    let mut durability = crate::backend::durable::Durability::begin(&dest);
    durability.rename_fat = policy.renames_inside();
    // The hidden files of copies that died into this folder go before this one lands there; see copystage.rs.
    crate::backend::copysweep::sweep(&dest, &cancel);
    let pace = Pace::new();
    let mut steps: Vec<Step> = Vec::new();
    // One deferred trash session for every Replace in the batch, begun only by the first one.
    let mut deferred: Option<super::mergeapply::Deferred> = None;
    let mut retry = Vec::new();
    let (mut ok, mut failed, mut skipped) = (0usize, 0usize, 0usize);
    let mut refused_all: Vec<(String, String)> = Vec::new();
    // An item that failed only for what the drive refused landed everything else, which the finish still confirms.
    let mut landed_partly = 0usize;
    let (mut merged_total, mut kept_total) = (0usize, 0usize);
    // Every top-level folder a merge folded in, so the terminal line names the one when it is one.
    let mut merged_names: Vec<String> = Vec::new();
    let mut was_cancelled = false;
    // Resolved once: a destination reached through a symlinked directory names the same inode under
    // another string, and the per-item guards below compare against this rather than the raw path.
    let dest_real = dest.canonicalize().unwrap_or_else(|_| dest.clone());
    // Cross-device moves confirm folders once per batch; copies never enter it, so an empty batch closes to nothing.
    let mut batch = super::movebatch::MoveBatch::new();
    // The folders the sources sit in and the mount table the journal reads, each resolved once for the batch and not once per item.
    let (mut parents, table) = (super::collide::Parents::default(), super::collide::Table::read(&dest));
    for (index, raw) in paths.iter().enumerate() {
        // The flag stops the batch, and so does an item whose own error says it was cancelled, see collide::cancelled.
        if was_cancelled || cancel.load(Ordering::Relaxed) {
            // A cancel completes the open batch's landed copies and skips what never started.
            let (closed, retried) = super::movebatch::close_cancelled(&mut batch, id, &tx, &mut steps, &mut durability);
            ok += closed.ok;
            failed += closed.failed;
            skipped += closed.skipped;
            retry.extend(retried);
            was_cancelled = true;
            skipped += 1;
            continue;
        }
        // A test cancel lands here, past the loop-top check, the way a real cancel lands mid-item.
        #[cfg(test)]
        if cancel_at_fires(index) {
            cancel.store(true, Ordering::Relaxed);
        }
        let src = flea::bytename::exact(raw);
        let name = base_name(&src);
        let dst = dest.join(flea::bytename::view(&name));
        // Refuse relative sources so they cannot resolve against the backend's own working directory.
        if !src.is_absolute() {
            failed += 1;
            let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: "a source must be an absolute path".into() });
            continue;
        }
        let checked = if let Some(items) = &selection {
            items.get(index).filter(|item| item.path == src)
                .ok_or_else(|| super::opsdispatch::SELECTION_CHANGED.to_string())
                .and_then(|item| item.current())
        } else {
            src.symlink_metadata().map_err(|error| io_message(&error))
        };
        let metadata = match checked {
            Ok(metadata) => metadata,
            Err(err) => {
                failed += 1;
                if let Some(identity) = selection
                    .as_ref()
                    .and_then(|items| items.get(index))
                    .and_then(|item| item.retry_identity())
                {
                    retry.push((src, identity));
                }
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err });
                continue;
            }
        };
        let source = ItemIdentity::record(&metadata);
        if let Some(destination) = &destination {
            if destination.path != dest || destination.current().is_err() {
                failed += 1;
                retry.push((src, source));
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false,
                    err: "Dropbox account folder changed or disappeared; this item was not moved.".into() });
                continue;
            }
        }
        // A symlink is copied or moved as the link itself (copy_any, move_any), so it holds nothing and its target's tree is not its own; only a real directory can contain the destination.
        let src_is_link = metadata.file_type().is_symlink();
        let src_real = if src_is_link { src.clone() } else { parents.real_of(&src) };
        // A folder into itself or its own subtree: copy_dir would read its own fresh copy until the disk
        // is full, so the refusal ui/js/Drag.js canDropInto makes is made again here, per item.
        if !src_is_link && dest_real.starts_with(&src_real) {
            failed += 1;
            retry.push((src, source));
            let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: INTO_ITSELF.to_string() });
            continue;
        }
        // An item dropped into the folder it already lives in: copy_file would truncate it onto itself.
        let here = super::collide::already_there_in(&src, &name, &dst, &dest_real, &mut parents);
        // A folder landing on a folder folds in, closing any open batch first so earlier items report first; files take the second card's answer.
        let (mut folded_top, mut kept_top) = (false, false);
        enum Landed { Now(Result<(), FleaError>), Later }
        let landed = if merging && !here && !src_is_link && metadata.is_dir() && super::merge::is_mergeable(&src, &dst) && policy.covers(&src, &dst) {
            merged_names.push(flea::bytename::display(&name).into_owned());
            if !batch.is_empty() { let closed = close_open(&mut batch, id, &tx, &mut steps, &mut durability, [&mut ok, &mut failed, &mut skipped], &mut retry); close_merged(&closed, merging, &mut merged_total, &mut kept_total); }
            let inner = policy.inner();
            let mut counts = super::mergeapply::Counts::default();
            let outcome = super::mergeapply::merge_folder(id, &src, &dst, &dest, moving, &inner, &mut steps,
                &cancel, &tx, index, &name, &settled, &mut durability, &mut counts);
            merged_total += counts.merged;
            kept_total += counts.kept_both;
            skipped += counts.skipped;
            folded_top = true;
            Landed::Now(outcome)
        } else {
        let placed = if merging && !here { super::merge::merge_place(&policy, &src, dst, moving) }
            else { policy.place(&src, dst, here, moving) };
        let (dst, replace) = match placed {
            Place::Land { to, replace } => (to, replace),
            Place::Skip => {
                skipped += 1;
                continue;
            }
            Place::Refuse(err) => {
                failed += 1;
                retry.push((src, source));
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err });
                continue;
            }
        };
        if merging { kept_top = super::merge::kept_name(&dst, &name); }
        let mut land = |steps: &mut Vec<Step>| transferitem::one_item(TransferItem { id, index, name: &name, src: &src, dst: &dst, source: source.clone() }, moving, &cancel, &tx, (&settled, &pace, &table), steps, &mut durability);
        // A replace trashes through the batch's deferred session, so its copy still follows its own trash.
        if replace {
            let slot = deferred.get_or_insert_with(super::mergeapply::Deferred::begin);
            Landed::Now(super::mergeapply::replacing_deferred(&dst, &mut steps, slot, &cancel, land))
        } else if moving {
            // A full batch closes before the next item joins it.
            if batch.full() { let closed = close_open(&mut batch, id, &tx, &mut steps, &mut durability, [&mut ok, &mut failed, &mut skipped], &mut retry); close_merged(&closed, merging, &mut merged_total, &mut kept_total); }
            // Only regular files wait on a batch; anything else closes it first and moves alone.
            if metadata.is_file() {
                match super::movebatch::land_move(TransferItem { id, index, name: &name, src: &src, dst: &dst, source: source.clone() }, &cancel, &tx, &settled, &mut steps, &mut durability, &mut batch) {
                    super::movebatch::MoveOutcome::Done(r) => Landed::Now(r),
                    super::movebatch::MoveOutcome::Deferred => Landed::Later,
                }
            } else {
                if !batch.is_empty() { let closed = close_open(&mut batch, id, &tx, &mut steps, &mut durability, [&mut ok, &mut failed, &mut skipped], &mut retry); close_merged(&closed, merging, &mut merged_total, &mut kept_total); }
                Landed::Now(transferitem::one_item(TransferItem { id, index, name: &name, src: &src, dst: &dst, source: source.clone() }, moving, &cancel, &tx, (&settled, &pace, &table), &mut steps, &mut durability))
            }
        } else {
            Landed::Now(land(&mut steps))
        }
        };
        let refused = crate::backend::unheld::take();
        match landed {
            Landed::Later => {}
            Landed::Now(Ok(())) => {
                ok += 1;
                if merging && !here && !folded_top { merged_total += 1; kept_total += usize::from(kept_top); }
                let _ = tx.send(OpMsg::Item { id, index, name, ok: true, err: String::new() });
            }
            Landed::Now(Err(e)) => {
                // A failure closes the open batch first, so earlier items keep their order on the wire.
                if moving {
                    if cancel.load(Ordering::Relaxed) || cancelled(&e.msg) {
                        let (closed, retried) = super::movebatch::close_cancelled(&mut batch, id, &tx, &mut steps, &mut durability);
                        close_merged(&closed, merging, &mut merged_total, &mut kept_total);
                        ok += closed.ok;
                        failed += closed.failed;
                        skipped += closed.skipped;
                        was_cancelled |= closed.cancelled;
                        retry.extend(retried);
                    } else {
                        let closed = close_open(&mut batch, id, &tx, &mut steps, &mut durability, [&mut ok, &mut failed, &mut skipped], &mut retry);
                        close_merged(&closed, merging, &mut merged_total, &mut kept_total);
                    }
                }
                // A cancel whose put-back failed left the old item in Trash, so it stops the batch but is a failure, not a skip.
                was_cancelled |= cancelled(&e.msg);
                if e.msg == CANCELLED {
                    skipped += 1;
                } else {
                    failed += 1;
                    retry.push((src, source));
                    // copy_any keeps its refusals only when its summary is the item's error, which counts each one.
                    if !refused.is_empty() {
                        landed_partly += 1;
                        refused_all.extend(refused.iter().map(|(path, why)| (path.strip_prefix(&dest).unwrap_or(path).to_string_lossy().to_string(), why.clone())));
                    }
                }
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: e.msg });
            }
        }
        refused_all.truncate(crate::backend::unheld::NAMED);
    }
    // The end closes whatever the loop left staged; a cancel on the way out completes it as cancelled.
    if !batch.is_empty() {
        if was_cancelled || cancel.load(Ordering::Relaxed) {
            let (closed, retried) = super::movebatch::close_cancelled(&mut batch, id, &tx, &mut steps, &mut durability);
            close_merged(&closed, merging, &mut merged_total, &mut kept_total);
            ok += closed.ok;
            failed += closed.failed;
            skipped += closed.skipped;
            was_cancelled |= closed.cancelled;
            retry.extend(retried);
        } else {
            let (closed, retried) = super::movebatch::close_normal(&mut batch, id, &tx, &mut steps, &mut durability);
            close_merged(&closed, merging, &mut merged_total, &mut kept_total);
            ok += closed.ok;
            failed += closed.failed;
            skipped += closed.skipped;
            was_cancelled |= closed.cancelled;
            retry.extend(retried);
        }
    }
    // The bytes the throttled lines left unreported go out before the drive flush, which the card shows as copied.
    pace.close(&tx, id, &settled);
    // A batch that replaced nothing lists nothing; every Replace settles here, cancel included.
    if let Some(slot) = deferred.as_mut() {
        slot.settle(&mut steps);
    }
    // Entries that waited on a batch join the counts where they land, copy or move alike.
    if durability.rename_fat { steps.iter_mut().for_each(super::fatnames::mark_step); }
    let entry = Entry { op: if merging && !moving { "merge".to_string() } else if moving { "move".to_string() } else { "copy".to_string() }, steps };
    let finished = crate::backend::durable::finish(id, &tx, &mut durability, &dest, ok + landed_partly);
    let note = finished.note;
    // Test builds read what the operation still holds open exactly where the done line goes out.
    #[cfg(test)]
    crate::backend::durable::test_note_report(&durability);
    if merging {
        let into = super::merge::merge_into_name(&merged_names, &dest);
        let _ = tx.send(OpMsg::MergeDone { id, ok, failed, skipped, cancelled: was_cancelled, entry, retry,
            durable: finished.ok, note, merged: merged_total, kept_both: kept_total, into, refused: refused_all });
    } else {
        let _ = tx.send(OpMsg::TransferDone { id, ok, failed, skipped, cancelled: was_cancelled, entry, retry, durable: finished.ok, note, refused: refused_all });
    }
}

// A full batch closes where it stands; what it landed joins the tallies.
fn close_open(batch: &mut super::movebatch::MoveBatch, id: usize, tx: &Sender<OpMsg>, steps: &mut Vec<Step>,
    durability: &mut crate::backend::durable::Durability, tallies: [&mut usize; 3], retry: &mut Vec<(PathBuf, ItemIdentity)>) -> super::movebatch::CloseCounts {
    let (closed, retried) = super::movebatch::close_normal(batch, id, tx, steps, durability);
    let [ok, failed, skipped] = tallies;
    *ok += closed.ok;
    *failed += closed.failed;
    *skipped += closed.skipped;
    retry.extend(retried);
    closed
}

// Deferred items join the merge counts only for the items the close reports ok.
fn close_merged(closed: &super::movebatch::CloseCounts, merging: bool, merged_total: &mut usize, kept_total: &mut usize) {
    if merging {
        *merged_total += closed.ok;
        *kept_total += closed.kept_ok;
    }
}

pub(crate) use super::trashland::run_trash;

// The same for a duplicate: ui/Pane.qml's own path is run_duplicate_checked.
#[cfg_attr(not(test), allow(dead_code))]
pub fn run_duplicate(path: String, tx: Sender<OpMsg>) {
    run_duplicate_checked(path, tx, None)
}

pub(crate) fn run_duplicate_checked(path: String, tx: Sender<OpMsg>, selection: Option<Vec<super::menu_actions::Selected>>) {
    run_duplicate_requested(path, tx, selection, 0)
}

pub(crate) fn run_duplicate_requested(path: String, tx: Sender<OpMsg>, selection: Option<Vec<super::menu_actions::Selected>>, request: usize) {
    let (outcome, steps) = match super::menu_actions::validate_sources(selection.as_deref(), &[flea::bytename::exact(&path)]) {
        Ok(()) => ops::duplicate(&flea::bytename::view(&path)),
        Err(error) => (Err(op_err("duplicate", &path, &error)), Vec::new()),
    };
    // Carried on a failure too: the steps then name the partial copy the failure left behind.
    let entry = Entry { op: "duplicate".to_string(), steps };
    let msg = match outcome {
        Ok(dst) => OpMsg::Duplicated { ok: true, path: dst.exact_text().to_string(), err: String::new(), entry },
        Err(e) => OpMsg::Duplicated { ok: false, path: String::new(), err: e.msg, entry },
    };
    let _ = tx.send(super::opsdispatch::created_request::wrap(request, msg));
}

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "transfer_group_tests.rs"]
mod group_tests;
