// The operations request layer: the response lines, and the one thread an operation runs on.
use crate::backend::collide::{already_there, cancelled, replacing, Place, Policy, Skipping, CANCELLED};
use crate::backend::copyfile::{copy_any, move_any, Progress};
use crate::backend::ops;
use crate::backend::trash;
use crate::backend::undo::{self, Entry, ItemIdentity, Step};
use crate::error::{from_io, io_message, FleaError};
use crate::json::escape;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

// One progress line per item at most this often, so a fast copy of a small file may emit none at all.
pub(crate) const PROGRESS_EVERY: Duration = Duration::from_millis(150);

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

// What an operation thread sends back, joined onto the same receiver every other event already arrives on.
pub enum OpMsg {
    // scanned is the batch's own total, 0 until the sweep beside the copy settles on one.
    Progress { id: usize, index: usize, name: String, bytes: u64, total: u64, scanned: u64 },
    Item { id: usize, index: usize, name: String, ok: bool, err: String },
    TransferDone { id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool, entry: Entry,
                   retry: Vec<(PathBuf, ItemIdentity)>, durable: bool, note: String },
    Trashed { ok: usize, failed: usize, entry: Entry },
    Duplicated { ok: bool, path: String, err: String, entry: Entry },
    RedoDone { journal: super::undo::Journal, result: Result<String, FleaError> },
    MenuDeleteDone { line: String },
    // A terminal line for a slot-holding operation: written like Meta, and it releases the slot.
    SlotDone { line: String },
    // A collisions answer, kept as the latest question before its line goes out; it claims no slot.
    Asked { turn: usize, question: crate::backend::collide::Question, line: String },
    // Not an operation: meta rides this channel because a media probe is a subprocess and the loop
    // must not wait on one. Nothing about it claims the one-at-a-time slot.
    Meta { line: String },
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

pub fn transferdone_line(id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool,
                         retry: &[(PathBuf, ItemIdentity)], durable: bool, note: &str) -> String {
    let paths: Vec<_> = retry.iter().map(|(path, _)| format!("\"{}\"", escape(&path.to_string_lossy()))).collect();
    format!(
        r#"{{"t":"transferdone","id":{},"ok":{},"failed":{},"skipped":{},"cancelled":{},"retryPaths":[{}],"durable":{},"note":"{}"}}"#,
        id, ok, failed, skipped, cancelled, paths.join(","), durable, escape(note)
    )
}

// Permission repair may change ctime; retry selects the original inode and link kind, never a replacement at its name.
pub fn retain_retry(retry: &[(PathBuf, ItemIdentity)], matches: &mut Vec<(&str, usize)>) {
    let originals: HashMap<_, _> = retry.iter().map(|(path, identity)| (path.as_path(), identity)).collect();
    matches.retain(|(path, _)| originals.get(Path::new(path)).is_some_and(|original|
        ItemIdentity::inspect(Path::new(path)).is_ok_and(|current| original.same_item(&current))));
}

pub fn trashed_line(ok: usize, failed: usize) -> String {
    format!(r#"{{"t":"trashed","ok":{},"failed":{}}}"#, ok, failed)
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
    let p = PathBuf::from(dest);
    if !p.is_absolute() {
        return Err(op_err("transfer", dest, "a destination must be an absolute path"));
    }
    match p.metadata() {
        Ok(m) if m.is_dir() && crate::backend::ops::dir_writable(&p) => Ok(p),
        Ok(m) if m.is_dir() => Err(op_err("transfer", dest, "that folder cannot be written")),
        Ok(_) => Err(op_err("transfer", dest, "the destination is not a directory")),
        Err(e) => Err(from_io("transfer", dest, &e)),
    }
}

pub fn op_err(where_: &str, path: &str, msg: &str) -> FleaError {
    FleaError { where_: where_.to_string(), path: path.to_string(), msg: msg.to_string() }
}

fn base_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
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
    run_transfer_checked(id, moving, paths, dest, cancel, tx, None, None, Policy::default())
}

// A tree this far in is a tree the operator is watching, so the sweep gives up rather than holding a
// thread on a mount that has stopped answering; what it costs then is the estimate, never the copy.

// The scan a batch's total comes from, on its own thread so the first byte never waits for it. It
// The sweep's own lifetime: it walks while the transfer is in this function and stops when it leaves.
struct SweepGuard {
    flag: Arc<AtomicBool>,
}

impl Drop for SweepGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Relaxed);
    }
}

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
            let seen = crate::backend::dirsize::walk_while(Path::new(raw), &|| done(&flags));
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
    let src = Path::new(raw);
    !skipping.is_some_and(|skipping| skipping.leaves(src, &dest.join(base_name(src))))
}

pub(crate) fn run_transfer_checked(
    id: usize, moving: bool, paths: Vec<String>, dest: PathBuf,
    cancel: Arc<AtomicBool>, tx: Sender<OpMsg>, selection: Option<Vec<super::menu_actions::Selected>>,
    destination: Option<super::menu_actions::Selected>, policy: Policy,
) {
    // Directive 45: a batch gets a time left once it knows what it is copying, and a tree's size is
    // not known without a walk. This is that walk, beside the copy rather than before it.
    let settled = Arc::new(AtomicU64::new(0));
    // The walk outlives nothing: the guard clears the flag when this function leaves, a panic included.
    let sweep = SweepGuard { flag: Arc::new(AtomicBool::new(true)) };
    let policy = policy.for_batch(&paths);
    spawn_total(&paths, &dest, policy.skipping(), &cancel, &settled, &sweep.flag);
    let mut durability = crate::backend::durable::Durability::begin(&dest);
    let mut steps: Vec<Step> = Vec::new();
    let mut retry = Vec::new();
    let (mut ok, mut failed, mut skipped) = (0usize, 0usize, 0usize);
    let mut was_cancelled = false;
    // Resolved once: a destination reached through a symlinked directory names the same inode under
    // another string, and the per-item guards below compare against this rather than the raw path.
    let dest_real = dest.canonicalize().unwrap_or_else(|_| dest.clone());
    // Cross-device moves confirm folders once per batch; copies never enter it, so an empty batch closes to nothing.
    let mut batch = super::movebatch::MoveBatch::new();
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
        let src = PathBuf::from(raw);
        let name = base_name(&src);
        let dst = dest.join(&name);
        let checked = if let Some(items) = &selection {
            items.get(index).filter(|item| item.path == src)
                .ok_or_else(|| "Menu selection no longer matches this transfer.".to_string())
                .and_then(|item| item.current())
        } else {
            src.symlink_metadata().map_err(|error| io_message(&error))
        };
        let metadata = match checked {
            Ok(metadata) => metadata,
            Err(err) => {
                failed += 1;
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
        let src_real = if src_is_link { src.clone() } else { src.canonicalize().unwrap_or_else(|_| src.clone()) };
        // A folder into itself or its own subtree: copy_dir would read its own fresh copy until the disk
        // is full, so the refusal ui/js/Drag.js canDropInto makes is made again here, per item.
        if !src_is_link && dest_real.starts_with(&src_real) {
            failed += 1;
            retry.push((src, source));
            let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: INTO_ITSELF.to_string() });
            continue;
        }
        // An item dropped into the folder it already lives in: copy_file would truncate it onto itself.
        let here = already_there(&src, &name, &dst, &dest_real);
        let (dst, replace) = match policy.place(&src, dst, here, moving) {
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
        let mut land = |steps: &mut Vec<Step>| one_item(id, index, &name, moving, &src, &dst, source.clone(), &cancel, &tx, &settled, steps, &mut durability);
        // A replace runs the single-item path, so its trash and put-back stay synchronous; other moves may stage.
        enum Landed { Now(Result<(), FleaError>), Later }
        let landed = if replace {
            Landed::Now(replacing(&dst, &mut steps, land))
        } else if moving {
            // A full batch closes before the next item joins it.
            if batch.full() {
                let (closed, retried) = super::movebatch::close_normal(&mut batch, id, &tx, &mut steps, &mut durability);
                ok += closed.ok;
                failed += closed.failed;
                skipped += closed.skipped;
                retry.extend(retried);
            }
            // Only regular files wait on a batch; anything else closes it first and moves alone.
            if metadata.is_file() {
                match super::movebatch::land_move(id, index, &name, &src, &dst, source.clone(), &cancel, &tx, &settled, &mut steps, &mut durability, &mut batch) {
                    super::movebatch::MoveOutcome::Done(r) => Landed::Now(r),
                    super::movebatch::MoveOutcome::Deferred => Landed::Later,
                }
            } else {
                if !batch.is_empty() {
                    let (closed, retried) = super::movebatch::close_normal(&mut batch, id, &tx, &mut steps, &mut durability);
                    ok += closed.ok;
                    failed += closed.failed;
                    skipped += closed.skipped;
                    retry.extend(retried);
                }
                Landed::Now(one_item(id, index, &name, moving, &src, &dst, source.clone(), &cancel, &tx, &settled, &mut steps, &mut durability))
            }
        } else {
            Landed::Now(land(&mut steps))
        };
        match landed {
            Landed::Later => {}
            Landed::Now(Ok(())) => {
                ok += 1;
                let _ = tx.send(OpMsg::Item { id, index, name, ok: true, err: String::new() });
            }
            Landed::Now(Err(e)) => {
                // A failure closes the open batch first, so earlier items keep their order on the wire.
                if moving {
                    if cancel.load(Ordering::Relaxed) || cancelled(&e.msg) {
                        let (closed, retried) = super::movebatch::close_cancelled(&mut batch, id, &tx, &mut steps, &mut durability);
                        ok += closed.ok;
                        failed += closed.failed;
                        skipped += closed.skipped;
                        was_cancelled |= closed.cancelled;
                        retry.extend(retried);
                    } else {
                        let (closed, retried) = super::movebatch::close_normal(&mut batch, id, &tx, &mut steps, &mut durability);
                        ok += closed.ok;
                        failed += closed.failed;
                        skipped += closed.skipped;
                        retry.extend(retried);
                    }
                }
                // A cancel whose put-back failed left the old item in Trash, so it stops the batch but is a failure, not a skip.
                was_cancelled |= cancelled(&e.msg);
                if e.msg == CANCELLED {
                    skipped += 1;
                } else {
                    failed += 1;
                    retry.push((src, source));
                }
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: e.msg });
            }
        }
    }
    // The end closes whatever the loop left staged; a cancel on the way out completes it as cancelled.
    if !batch.is_empty() {
        if was_cancelled || cancel.load(Ordering::Relaxed) {
            let (closed, retried) = super::movebatch::close_cancelled(&mut batch, id, &tx, &mut steps, &mut durability);
            ok += closed.ok;
            failed += closed.failed;
            skipped += closed.skipped;
            was_cancelled |= closed.cancelled;
            retry.extend(retried);
        } else {
            let (closed, retried) = super::movebatch::close_normal(&mut batch, id, &tx, &mut steps, &mut durability);
            ok += closed.ok;
            failed += closed.failed;
            skipped += closed.skipped;
            was_cancelled |= closed.cancelled;
            retry.extend(retried);
        }
    }
    let entry = Entry { op: if moving { "move".to_string() } else { "copy".to_string() }, steps };
    let finished = crate::backend::durable::finish(id, &tx, &durability, &dest, ok);
    let _ = tx.send(OpMsg::TransferDone { id, ok, failed, skipped, cancelled: was_cancelled, entry, retry, durable: finished.ok, note: finished.note });
}

// A directory reports the bytes its tree has copied so far and no total, see copyfile.rs Progress.
// Its journal steps land in `steps` either way: a failure that created its destination left a partial there.
fn one_item(
    id: usize,
    index: usize,
    name: &str,
    moving: bool,
    src: &Path,
    dst: &Path,
    source: ItemIdentity,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
    settled: &AtomicU64,
    steps: &mut Vec<Step>,
    durability: &mut super::durable::Durability,
) -> Result<(), FleaError> {
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
            // Still 0 while the sweep counts, and the batch's own total from the moment it settles.
            scanned: settled.load(Ordering::Relaxed),
        });
    };
    let mut p = Progress { cancel, on_bytes: &mut sink, partial: None, tree: None, manifest: if moving { super::copymanifest::writer_for_move(src, dst) } else { super::copymanifest::writer_for(src, dst) }, durability: Some(durability) };
    let mut outcome = if moving { move_any(src, dst, &mut p) } else { copy_any(src, dst, &mut p) };
    match &outcome {
        Ok(()) if moving => steps.push(undo::moved(src, dst, source)?),
        Ok(()) => steps.push(undo::copied(src, dst, source)?),
        // The partial is this operation's, so it is journaled and undo removes it like any created path.
        Err(_) => {
            if let Some(path) = p.partial.take() {
                let (manifest, loud) = super::copymanifest::finish_loud(p.manifest.take());
                if let (Err(outcome), Some(e)) = (&mut outcome, loud) {
                    outcome.msg.push_str(&format!("; copy manifest failed: {e}"));
                }
                steps.push(undo::copied_partial(src, &path, source, manifest)?);
            }
        }
    }
    outcome
}

pub(crate) fn run_trash(paths: Vec<String>, tx: Sender<OpMsg>, selection: Option<Vec<super::menu_actions::Selected>>) {
    let owned: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let (entries, failed) = match trash::trash_checked(&owned, selection.as_deref()) {
        Ok(result) => result,
        Err(error) => {
            let line = super::proto::error_line(&op_err("trash", "", &error));
            let _ = tx.send(OpMsg::Meta { line });
            (Vec::new(), owned.len())
        }
    };
    let ok = entries.len();
    let steps = entries.into_iter().map(Step::Trashed).collect();
    let entry = Entry { op: "trash".to_string(), steps };
    let _ = tx.send(OpMsg::Trashed { ok, failed, entry });
}

// The same for a duplicate: ui/Pane.qml's own path is run_duplicate_checked.
#[cfg_attr(not(test), allow(dead_code))]
pub fn run_duplicate(path: String, tx: Sender<OpMsg>) {
    run_duplicate_checked(path, tx, None)
}

pub(crate) fn run_duplicate_checked(path: String, tx: Sender<OpMsg>, selection: Option<Vec<super::menu_actions::Selected>>) {
    let (outcome, steps) = match super::menu_actions::validate_sources(selection.as_deref(), &[PathBuf::from(&path)]) {
        Ok(()) => ops::duplicate(Path::new(&path)),
        Err(error) => (Err(op_err("duplicate", &path, &error)), Vec::new()),
    };
    // Carried on a failure too: the steps then name the partial copy the failure left behind.
    let entry = Entry { op: "duplicate".to_string(), steps };
    let msg = match outcome {
        Ok(dst) => OpMsg::Duplicated { ok: true, path: dst.to_string_lossy().to_string(), err: String::new(), entry },
        Err(e) => OpMsg::Duplicated { ok: false, path: String::new(), err: e.msg, entry },
    };
    let _ = tx.send(msg);
}

#[cfg(test)]
mod tests;
