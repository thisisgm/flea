// The operations request layer: the response lines, and the one thread an operation runs on.
use crate::backend::copyfile::{copy_any, move_any, Progress};
use crate::backend::ops;
use crate::backend::trash;
use crate::backend::undo::{Entry, Step};
use crate::error::FleaError;
use crate::json::escape;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

// One progress line per item at most this often, so a fast copy of a small file may emit none at all.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

// What an operation thread sends back, joined onto the same receiver every other event already arrives on.
pub enum OpMsg {
    Progress { id: usize, index: usize, name: String, bytes: u64, total: u64 },
    Item { id: usize, index: usize, name: String, ok: bool, err: String },
    TransferDone { id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool, entry: Entry },
    Trashed { ok: usize, failed: usize, entry: Entry },
    Duplicated { ok: bool, path: String, err: String, entry: Entry },
    // Not an operation: meta rides this channel because a media probe is a subprocess and the loop
    // must not wait on one. Nothing about it claims the one-at-a-time slot.
    Meta { line: String },
}

// moving is the verb the request actually resolved to, so the client names the operation from the
// wire rather than from a clipboard it may have already spent or never owned.
pub fn transferstarted_line(id: usize, n: usize, moving: bool) -> String {
    format!(r#"{{"t":"transferstarted","id":{},"n":{},"moving":{}}}"#, id, n, moving)
}

// total is 0 for a directory, whose size is not known in advance without the sweep this codebase never does.
pub fn transferprogress_line(id: usize, index: usize, name: &str, bytes: u64, total: u64) -> String {
    format!(
        r#"{{"t":"transferprogress","id":{},"index":{},"name":"{}","bytes":{},"total":{}}}"#,
        id, index, escape(name), bytes, total
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

pub fn transferdone_line(id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool) -> String {
    format!(
        r#"{{"t":"transferdone","id":{},"ok":{},"failed":{},"skipped":{},"cancelled":{}}}"#,
        id, ok, failed, skipped, cancelled
    )
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
        Ok(m) if m.is_dir() => Ok(p),
        Ok(_) => Err(op_err("transfer", dest, "the destination is not a directory")),
        Err(e) => Err(op_err("transfer", dest, &e.to_string())),
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
pub fn run_transfer(
    id: usize,
    moving: bool,
    paths: Vec<String>,
    dest: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<OpMsg>,
) {
    let mut steps: Vec<Step> = Vec::new();
    let (mut ok, mut failed, mut skipped) = (0usize, 0usize, 0usize);
    let mut was_cancelled = false;
    // Resolved once: a destination reached through a symlinked directory names the same inode under
    // another string, and the per-item guards below compare against this rather than the raw path.
    let dest_real = dest.canonicalize().unwrap_or_else(|_| dest.clone());
    for (index, raw) in paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            was_cancelled = true;
            skipped += 1;
            continue;
        }
        let src = PathBuf::from(raw);
        let name = base_name(&src);
        let dst = dest.join(&name);
        // A symlink is copied or moved as the link itself (copy_any, move_any), so it holds nothing and its target's tree is not its own; only a real directory can contain the destination.
        let src_is_link = src.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);
        let src_real = if src_is_link { src.clone() } else { src.canonicalize().unwrap_or_else(|_| src.clone()) };
        // A folder into itself or its own subtree: copy_dir would read its own fresh copy until the disk
        // is full, so the refusal ui/js/Drag.js canDropInto makes is made again here, per item.
        if !src_is_link && dest_real.starts_with(&src_real) {
            failed += 1;
            let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: INTO_ITSELF.to_string() });
            continue;
        }
        // Where the entry itself lives, link or not: its parent resolved, plus its own name.
        let src_here = match src.parent() {
            Some(parent) => parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf()).join(&name),
            None => src.clone(),
        };
        // An item dropped into the folder it already lives in: copy_file would truncate it onto itself.
        if dst == src || dest_real.join(&name) == src_here {
            failed += 1;
            let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: ALREADY_THERE.to_string() });
            continue;
        }
        match one_item(id, index, &name, moving, &src, &dst, &cancel, &tx, &mut steps) {
            Ok(()) => {
                ok += 1;
                let _ = tx.send(OpMsg::Item { id, index, name, ok: true, err: String::new() });
            }
            Err(e) => {
                if e.msg == "cancelled" {
                    was_cancelled = true;
                }
                failed += 1;
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: e.msg });
            }
        }
    }
    let entry = Entry { op: if moving { "move".to_string() } else { "copy".to_string() }, steps };
    let _ = tx.send(OpMsg::TransferDone { id, ok, failed, skipped, cancelled: was_cancelled, entry });
}

// A directory has no total without a sweep, so only a file item reports bytes at all. Its journal
// steps land in `steps` either way: a failure that created its destination left a partial there.
fn one_item(
    id: usize,
    index: usize,
    name: &str,
    moving: bool,
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
    steps: &mut Vec<Step>,
) -> Result<(), FleaError> {
    let is_file = src.symlink_metadata().map(|m| m.is_file()).unwrap_or(false);
    let mut last = Instant::now() - PROGRESS_EVERY;
    let mut sink = |done: u64, total: u64| {
        if !is_file || last.elapsed() < PROGRESS_EVERY {
            return;
        }
        last = Instant::now();
        let _ = tx.send(OpMsg::Progress {
            id,
            index,
            name: name.to_string(),
            bytes: done,
            total,
        });
    };
    let mut p = Progress { cancel, on_bytes: &mut sink, partial: None };
    let outcome = if moving { move_any(src, dst, &mut p) } else { copy_any(src, dst, &mut p) };
    match &outcome {
        Ok(()) if moving => steps.push(Step::Moved { from: src.to_path_buf(), to: dst.to_path_buf() }),
        Ok(()) => steps.push(Step::Created { path: dst.to_path_buf() }),
        // The partial is this operation's, so it is journaled and undo removes it like any created path.
        Err(_) => {
            if let Some(path) = p.partial.take() {
                steps.push(Step::Created { path });
            }
        }
    }
    outcome
}

pub fn run_trash(paths: Vec<String>, tx: Sender<OpMsg>) {
    let owned: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let (entries, failed) = trash::trash(&owned);
    let ok = entries.len();
    let steps = entries.into_iter().map(Step::Trashed).collect();
    let entry = Entry { op: "trash".to_string(), steps };
    let _ = tx.send(OpMsg::Trashed { ok, failed, entry });
}

pub fn run_duplicate(path: String, tx: Sender<OpMsg>) {
    let (outcome, steps) = ops::duplicate(Path::new(&path));
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
