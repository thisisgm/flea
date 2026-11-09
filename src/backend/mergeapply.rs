// Folder merge, second half: landing entries, manifests, and emptied-folder records.
use flea::bytename::ExactText as _;
use super::merge::{count_tree, real_dir};
pub(crate) use super::merge::Counts;
use crate::backend::collide::{Place, Policy};
use crate::backend::copyfile::{copy_any, move_any, Progress};
use crate::backend::copymanifest::Writer;
use crate::backend::durable::Durability;
use crate::backend::undo::{self, ItemIdentity, Step};
use crate::error::{from_io, FleaError};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

// The Replace session one transfer shares across its merges lives beside this file, under the file budget.
#[path = "replacedefer.rs"]
mod replacedefer;
pub(crate) use replacedefer::{replacing_deferred, Deferred};
// The seven things one landing carries besides its two paths, so land_one takes three arguments.
pub(crate) struct LandCtx<'a> {
    pub moving: bool,
    pub cancel: &'a AtomicBool,
    pub sink: &'a mut dyn FnMut(u64, u64),
    pub carried: &'a mut u64,
    pub manifest: &'a mut Option<Writer>,
    pub ctx: &'a mut Durability,
    pub steps: &'a mut Vec<Step>,
}

#[cfg(test)]
thread_local! {
    static FAIL_JOURNAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Journals a move the walker already landed; a journal the drive will not take fails the land instead of omitting it.
fn journal_moved(steps: &mut Vec<Step>, src: &Path, to: &Path) -> Result<(), FleaError> {
    #[cfg(test)]
    if FAIL_JOURNAL.with(|fail| fail.get()) {
        return Err(FleaError { where_: "journal".into(), path: to.to_string_lossy().into(), msg: "test journal refusal".into() });
    }
    let meta = to.symlink_metadata().map_err(|e| from_io("move", &to.to_string_lossy(), &e))?;
    steps.push(undo::moved(src, to, ItemIdentity::record(&meta))?);
    Ok(())
}

// Copies or moves one entry and journals a move; move_any touches what its rename leg needs.
fn land_one(src: &Path, to: &Path, land: &mut LandCtx) -> Result<(), FleaError> {
    let mut p = Progress { cancel: land.cancel, on_bytes: &mut *land.sink, partial: None, tree: Some(*land.carried),
        manifest: land.manifest.take(), durability: Some(&mut *land.ctx), for_move: land.moving };
    let r = if land.moving { move_any(src, to, &mut p) } else { copy_any(src, to, &mut p) };
    *land.carried = p.tree.unwrap_or(*land.carried);
    *land.manifest = p.manifest.take();
    if r.is_ok() && land.moving {
        journal_moved(land.steps, src, to)?;
    }
    r
}

// Merges one top-level folder into the destination; inner failures collect and the top answers once.
#[allow(clippy::too_many_arguments)]
pub(crate) fn merge_folder(
    id: usize,
    src: &Path,
    dst_top: &Path,
    dest: &Path,
    moving: bool,
    inner: &Policy,
    steps: &mut Vec<Step>,
    cancel: &AtomicBool,
    tx: &Sender<crate::backend::opsreq::OpMsg>,
    index: usize,
    name: &str,
    settled: &std::sync::atomic::AtomicU64,
    ctx: &mut Durability,
    counts: &mut Counts,
) -> Result<(), FleaError> {
    const EVERY: Duration = Duration::from_millis(150);
    let mut last = Instant::now() - EVERY;
    let mut sink = |done: u64, total: u64| {
        if last.elapsed() < EVERY {
            return;
        }
        last = Instant::now();
        let _ = tx.send(crate::backend::opsreq::OpMsg::Progress { id, index, name: name.to_string(),
            bytes: done, total, scanned: settled.load(Ordering::Relaxed) });
    };
    let mut carried: u64 = 0;
    // Moves journal per-file steps, so no manifest; copies keep one manifest for the whole folder.
    let mut manifest = if moving { None } else { crate::backend::copymanifest::writer_for(src, dst_top) };
    // The top folder already exists, so only its children are recorded; recording it would risk it on undo.
    let mut deferred = Deferred::begin();
    let r = merge_into(src, dst_top, dest, moving, inner, steps, cancel, &mut sink, &mut carried, &mut manifest, ctx, counts, &mut deferred);
    deferred.settle(steps);
    if moving {
        remove_emptied(src, steps);
        return r;
    }
    finish_manifest(src, dst_top, manifest, steps, &r, counts.merged)
}

// The manifest names only what the merge created, so undo removes those and never a file already there.
fn finish_manifest(src: &Path, dst_top: &Path, manifest: Option<Writer>, steps: &mut Vec<Step>, r: &Result<(), FleaError>, merged: usize) -> Result<(), FleaError> {
    let Some(writer) = manifest else {
        if merged > 0 {
            eprintln!("flea: merge of {} left {} items without a manifest; undo restores replaced files only", dst_top.display(), merged);
        }
        return r.as_ref().map(|_| ()).map_err(clone_err);
    };
    let (handle, loud) = crate::backend::copymanifest::finish_loud(Some(writer));
    if let Some(e) = loud {
        let mut err = r.as_ref().err().map(clone_err).unwrap_or_else(|| FleaError { where_: "transfer".into(), path: dst_top.to_string_lossy().to_string(), msg: String::new() });
        err.msg = crate::backend::ops::manifest_failed(&err.msg, &e);
        return Err(err);
    }
    if let Some(handle) = handle {
        if let Ok(meta) = src.symlink_metadata() {
            let source = ItemIdentity::record(&meta);
            if let Ok(step) = undo::merged_partial(src, dst_top, source, Some(handle)) {
                steps.push(step);
            }
        }
    }
    r.as_ref().map(|_| ()).map_err(clone_err)
}

fn clone_err(e: &FleaError) -> FleaError {
    FleaError { where_: e.where_.clone(), path: e.path.clone(), msg: e.msg.clone() }
}

// How many failures the item error names before it counts the rest.
const REPORTED_FAILURES: usize = 3;

// Folds one source directory into its destination twin, all the way down.
#[allow(clippy::too_many_arguments)]
fn merge_into(
    src_dir: &Path,
    dst_dir: &Path,
    dest: &Path,
    moving: bool,
    inner: &Policy,
    steps: &mut Vec<Step>,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(u64, u64),
    carried: &mut u64,
    manifest: &mut Option<Writer>,
    ctx: &mut Durability,
    counts: &mut Counts,
    deferred: &mut Deferred,
) -> Result<(), FleaError> {
    let dir_rel = dst_dir.strip_prefix(dest).unwrap_or(dst_dir).to_string_lossy().to_string();
    let entries = match std::fs::read_dir(src_dir) {
        Ok(entries) => entries,
        Err(e) => {
            let err = from_io("transfer", &src_dir.to_string_lossy(), &e);
            counts.failed.push(format!("{dir_rel}: {}", err.msg));
            return Err(err);
        }
    };
    // The counts run across the whole fold, so this folder is judged by the failures it adds.
    let before = counts.failed.len();
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(FleaError { where_: "copy".into(), path: dst_dir.to_string_lossy().to_string(), msg: "cancelled".into() });
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                let err = from_io("transfer", &src_dir.to_string_lossy(), &e);
                counts.failed.push(format!("{dir_rel}: {}", err.msg));
                return Err(err);
            }
        };
        let name = entry.file_name();
        // A name the drive refuses is looked for, and lands, under the name Rename gives it.
        let landed = crate::backend::fatnames::inside_leaf(&name, ctx.rename_fat);
        let (src_inner, dst_inner) = (src_dir.join(&name), dst_dir.join(&landed));
        let rel = dst_inner.strip_prefix(dest).unwrap_or(&dst_inner).to_string_lossy().to_string();
        let Ok(meta) = src_inner.symlink_metadata() else {
            counts.failed.push(format!("{rel}: gone before it could be merged"));
            continue;
        };
        if dst_inner.symlink_metadata().is_err() {
            // Everything missing is copied or moved in.
            let n = count_tree(&src_inner);
            let mut land = LandCtx { moving, cancel, sink: &mut *sink, carried: &mut *carried, manifest: &mut *manifest, ctx: &mut *ctx, steps: &mut *steps };
            match land_one(&src_inner, &dst_inner, &mut land) {
                Ok(()) => counts.merged += n,
                Err(e) => counts.failed.push(format!("{rel}: {}", e.msg)),
            }
            continue;
        }
        if meta.is_dir() && !meta.file_type().is_symlink() && real_dir(&dst_inner) {
            // A nested fold shares the counts, so a failure naming it fails this folder too.
            let nested = merge_into(&src_inner, &dst_inner, dest, moving, inner, steps, cancel, sink, carried, manifest, ctx, counts, deferred);
            // A cancel inside the last nested folder leaves no later entry for the loop-top check.
            if nested.is_err() && cancel.load(Ordering::Relaxed) {
                return nested;
            }
            let _ = nested;
            if moving {
                remove_emptied(&src_inner, steps);
            }
            continue;
        }
        // Files that exist in both take the second card's answer, asked once before anything was written.
        match inner.place(&src_inner, dst_inner.clone(), false, moving) {
            Place::Skip => counts.skipped += 1,
            Place::Refuse(err) => counts.failed.push(format!("{rel}: {err}")),
            Place::Land { to, replace } => {
                let n = count_tree(&src_inner);
                let kept = to != dst_inner;
                // Replace trashes then creates exclusively, through the merge's deferred batch.
                let r = if replace {
                    replacing_deferred(&dst_inner, steps, deferred, cancel, |steps| {
                        let mut land = LandCtx { moving, cancel, sink: &mut *sink, carried: &mut *carried, manifest: &mut *manifest, ctx: &mut *ctx, steps: &mut *steps };
                        land_one(&src_inner, &to, &mut land)
                    })
                } else {
                    let mut land = LandCtx { moving, cancel, sink: &mut *sink, carried: &mut *carried, manifest: &mut *manifest, ctx: &mut *ctx, steps: &mut *steps };
                    land_one(&src_inner, &to, &mut land)
                };
                match r {
                    Ok(()) => {
                        counts.merged += n;
                        if kept {
                            counts.kept_both += 1;
                        }
                    }
                    Err(e) => counts.failed.push(format!("{rel}: {}", e.msg)),
                }
            }
        }
    }
    let own = &counts.failed[before..];
    if own.is_empty() {
        Ok(())
    } else {
        let shown = own.iter().take(REPORTED_FAILURES).cloned().collect::<Vec<_>>().join("; ");
        let msg = if own.len() > REPORTED_FAILURES { format!("{shown}; and {} more", own.len() - REPORTED_FAILURES) } else { shown };
        Err(FleaError { where_: "transfer".into(), path: dst_dir.to_string_lossy().to_string(), msg })
    }
}

// The journal steps a merge move records for the folders it emptied, kept beside the fold.

// Permission bits alone, the way the tree masks a mode everywhere else.
const MODE_BITS: u32 = 0o7777;

// The mode is read before the remove, so undo recreates the folder as it was, not as the umask would make it.
pub(crate) fn removed_empty_dir(path: &std::path::Path) -> Result<Step, FleaError> {
    let mode = path.symlink_metadata().map(|meta| meta.permissions().mode() & MODE_BITS)
        .map_err(|e| from_io("journal", &path.to_string_lossy(), &e))?;
    Ok(Step::RemovedEmptyDir { path: path.to_path_buf(), mode })
}

// Removes a source folder the merge emptied and journals it only when the remove landed.
fn remove_emptied(path: &Path, steps: &mut Vec<Step>) {
    let removed = removed_empty_dir(path).ok();
    if std::fs::remove_dir(path).is_ok() {
        steps.extend(removed);
    }
}

// Only a folder this operation emptied: a gone parent or a taken name refuses and creates nothing.
pub(crate) fn recreate_emptied(path: &PathBuf, mode: u32) -> Result<(), FleaError> {
    if path.symlink_metadata().is_ok() {
        return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
            msg: "another item now has the emptied folder's name, so undo left the moved items in place".into() });
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let held = match parent {
        // Followed, so a folder reached through a symlinked name still counts as there.
        Some(dir) => dir.metadata().map(|meta| meta.is_dir()).unwrap_or(false),
        None => false,
    };
    if !held {
        return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
            msg: "the emptied folder's parent is gone, so undo left the moved items in place".into() });
    }
    std::fs::DirBuilder::new().mode(mode).create(path).map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
    // The builder's mode passes through the umask, so the exact bits are set after the create.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| from_io("undo", &path.to_string_lossy(), &e))
}

// Redo takes an emptied folder away again, so the folder must still be the one undo left.
pub(crate) fn check_emptied(step: &Step) -> Result<(), FleaError> {
    let Step::RemovedEmptyDir { path, .. } = step else { return Ok(()) };
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(FleaError { where_: "redo".into(), path: path.to_string_lossy().into(),
            msg: "the emptied folder was replaced; redo left it in place".into() }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(FleaError { where_: "redo".into(), path: path.to_string_lossy().into(),
            msg: "the emptied folder is already gone; redo left it in place".into() }),
        Err(e) => Err(from_io("redo", &path.to_string_lossy(), &e)),
    }
}

// The Moved replays above emptied it again, so redo takes it away as the operation did.
pub(crate) fn redo_emptied(path: &Path, mode: u32, steps: &mut Vec<Step>) -> Result<(), FleaError> {
    std::fs::remove_dir(path).map_err(|e| from_io("redo", &path.to_string_lossy(), &e))?;
    steps.push(Step::RemovedEmptyDir { path: path.to_path_buf(), mode });
    Ok(())
}

// The shared journal's wire arm for the step above, beside the codec that owns the version.
pub(crate) fn encode_emptied(path: &Path, mode: u32) -> crate::jsondoc::Json {
    super::undocodec::obj(vec![
        ("k", super::undocodec::s("re")),
        ("path", super::undocodec::s(&path.exact_text())),
        ("mode", super::undocodec::n(mode)),
    ])
}

pub(crate) fn decode_emptied(pairs: &[(String, crate::jsondoc::Json)]) -> Option<Step> {
    let path = super::undocodec::checked_path(super::undocodec::get(pairs, "path")?)?;
    let mode = super::undocodec::parse_u32(super::undocodec::get(pairs, "mode")?)?;
    Some(Step::RemovedEmptyDir { path, mode })
}

// A merge top journals `"merge":1` beside its copy fields; an older journal has no key and reads as a plain copy.
pub(crate) fn mark_merge(mut pairs: Vec<(&'static str, crate::jsondoc::Json)>, merge: bool) -> Vec<(&'static str, crate::jsondoc::Json)> {
    if merge {
        pairs.push(("merge", super::undocodec::n(1)));
    }
    pairs
}

pub(crate) fn decode_merge(pairs: &[(String, crate::jsondoc::Json)]) -> bool {
    super::undocodec::get(pairs, "merge").and_then(super::undocodec::parse_u32) == Some(1)
}

#[cfg(test)]
#[path = "mergeapply_tests.rs"]
mod tests;

// A merge undone outside its own window refuses, because only that window's manifest may take entries away.
pub(crate) fn merge_foreign_undo(to: &Path) -> FleaError {
    FleaError { where_: "undo".into(), path: to.to_string_lossy().into(),
        msg: "this merge can be undone only in the window that made it, so undo left the merged items in place".into() }
}
