// Redo repeats journaled operations against their recorded inputs and never replaces a destination.
use super::copyfile::{copy_any, move_any, Progress};
use super::opsreq::{OpMsg, PROGRESS_EVERY};
use super::undo::{self, Entry, ItemIdentity, Step};
use crate::error::{from_io, FleaError};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;

struct ReplayStep {
    step: Step,
    input: Option<ItemIdentity>,
    parent: Option<(PathBuf, ItemIdentity)>,
}

pub(crate) struct Replay {
    op: String,
    steps: Vec<ReplayStep>,
}

// One alias for the wire form steps_data and from_steps share, so neither signature repeats it.
pub(crate) type SavedStep = (Step, Option<ItemIdentity>, Option<(PathBuf, ItemIdentity)>);
pub(crate) type SavedSteps = Vec<SavedStep>;

fn error(path: &Path, message: &str) -> FleaError {
    FleaError { where_: "redo".into(), path: path.to_string_lossy().into(), msg: message.into() }
}

// Sample output: "1 item was left in place: a: no", or "2 items were left in place: a: no; b: no".
pub(crate) fn left_in_place(skipped: &[String]) -> String {
    match skipped {
        [only] => format!("1 item was left in place: {only}"),
        many => format!("{} items were left in place: {}", many.len(), many.join("; ")),
    }
}

fn source(step: &Step) -> Option<&Path> {
    match step {
        Step::Moved { from, .. } | Step::Copied { from, .. } | Step::Tags { path: from, .. } => Some(from),
        Step::Linked { source, .. } => Some(source),
        Step::Trashed(entry) => Some(&entry.original),
        _ => None,
    }
}

fn destination(step: &Step) -> Option<&Path> {
    match step {
        Step::Moved { to, .. } | Step::Copied { to, .. } => Some(to),
        Step::Linked { path, .. } => Some(path),
        Step::MadeDir { path, .. } | Step::MadeFile { path, .. } | Step::RemovedEmptyDir { path, .. } => Some(path),
        _ => None,
    }
}

// linux/magic.h's exFAT magic: that driver moves a folder's birth time when a redone step writes into it.
const EXFAT_SUPER_MAGIC: i64 = 0x2011BAB0;

// The folder a step lands in is still the one it was; on exFAT dev, inode and kind decide, since its birth time moves.
fn same_folder(recorded: &ItemIdentity, current: &ItemIdentity, magic: impl FnOnce() -> Option<i64>) -> bool {
    recorded.same_item(current) || (recorded.parts() == current.parts() && magic() == Some(EXFAT_SUPER_MAGIC))
}

// A name an earlier Trashed step empties, spelled as recorded or, on a case-insensitive drive, the same item under another case.
fn vacates(vacated: &HashSet<&Path>, path: &Path) -> bool {
    vacated.contains(path) || vacated.iter().any(|gone| gone.parent() == path.parent() && super::renamecompat::is_same_item_for_move(gone, path))
}

impl Replay {
    pub fn capture(entry: Entry) -> Result<Self, FleaError> {
        let mut steps = Vec::new();
        for step in entry.steps {
            if matches!(step, Step::Created { .. } | Step::Barrier) {
                return Err(error(Path::new(""), "this operation cannot be redone"));
            }
            let input = match &step {
                Step::Copied { source, .. } => Some(source.clone()),
                _ => source(&step).map(ItemIdentity::inspect).transpose()?,
            };
            let parent = destination(&step).map(|path| {
                let parent = path.parent().filter(|p| p.is_absolute())
                    .ok_or_else(|| error(path, "the destination has no absolute parent"))?;
                Ok::<_, FleaError>((parent.to_path_buf(), ItemIdentity::inspect(parent)?))
            }).transpose()?;
            steps.push(ReplayStep { step, input, parent });
        }
        Ok(Self { op: entry.op, steps })
    }
    pub fn op(&self) -> &str { &self.op }
    pub fn len(&self) -> usize { self.steps.len() }
    // Every field the file needs to rebuild this replay for a redo in another window.
    pub(crate) fn steps_data(&self) -> (String, SavedSteps) {
        (self.op.clone(), self.steps.iter().map(|saved| (saved.step.clone(), saved.input.clone(), saved.parent.clone())).collect())
    }
    pub(crate) fn from_steps(op: String, steps: SavedSteps) -> Self {
        Self { op, steps: steps.into_iter().map(|(step, input, parent)| ReplayStep { step, input, parent }).collect() }
    }
    pub fn rebase(&mut self, old: &ItemIdentity, new: &ItemIdentity) {
        self.rebase_with(
            &mut |identity| if identity.unchanged_for_move(old) { *identity = new.clone(); },
            &mut |identity| if identity.same_item(old) { *identity = new.clone(); },
        );
    }
    // `moved` rewrites an input or a step's identity; `item` rewrites the parent folder's, which is judged looser.
    pub(crate) fn rebase_with(&mut self, moved: &mut impl FnMut(&mut ItemIdentity), item: &mut impl FnMut(&mut ItemIdentity)) {
        for saved in &mut self.steps {
            if let Some(identity) = &mut saved.input { moved(identity); }
            if let Some((_, identity)) = &mut saved.parent { item(identity); }
            super::undorebase::rebase_step(&mut saved.step, moved);
        }
    }
    fn check(saved: &ReplayStep, vacated: bool) -> Result<(), FleaError> {
        if let (Some(path), Some(identity)) = (source(&saved.step), &saved.input) {
            // A tag write moves the file's ctime, so a tag step is judged by the item alone and its replay compares the tags.
            let judge: fn(&ItemIdentity, &ItemIdentity) -> bool = if matches!(saved.step, Step::Tags { .. }) { ItemIdentity::same_item } else { ItemIdentity::unchanged_for_move };
            if !path.is_absolute() || !judge(identity, &ItemIdentity::inspect(path)?) {
                return Err(error(path, "the original item changed or was replaced; redo left it in place"));
            }
        }
        if let Some((path, identity)) = &saved.parent {
            if !same_folder(identity, &ItemIdentity::inspect(path)?, || super::fsinfo::magic_of(path)) {
                return Err(error(path, "the destination folder was replaced, so nothing was redone"));
            }
        }
        super::mergeapply::check_emptied(&saved.step)?;
        if let Some(path) = destination(&saved.step).filter(|_| !vacated && !matches!(saved.step, Step::RemovedEmptyDir { .. })) {
            match path.symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(from_io("redo", &path.to_string_lossy(), &e)),
                // A case-only rename on a case-insensitive drive finds its own source under the new name, which move_any takes through a temp name.
                Ok(_) if matches!(saved.step, Step::Moved { .. }) && source(&saved.step).is_some_and(|from| super::renamecompat::is_same_item_for_move(from, path)) => {}
                Ok(_) => return Err(error(path, "an item with that name is already there, so redo left it in place")),
            }
        }
        Ok(())
    }
    pub fn run(self, id: usize, cancel: &AtomicBool, tx: &Sender<OpMsg>)
        -> (Entry, Vec<(ItemIdentity, ItemIdentity)>, Result<String, FleaError>) {
        let mut entry = Entry { op: self.op.clone(), steps: Vec::new() };
        let mut changes = Vec::new();
        let result = (|| {
            // A replace trashes the old item before its copy lands, so a name an earlier Trashed step empties is free only once redo reaches it.
            let mut vacated = HashSet::new();
            for saved in &self.steps {
                if cancel.load(Ordering::Relaxed) { return Err(error(Path::new(""), "redo cancelled")); }
                Self::check(saved, destination(&saved.step).is_some_and(|path| vacates(&vacated, path)))?;
                if let Step::Trashed(entry) = &saved.step { vacated.insert(entry.original.as_path()); }
            }
            let mut skipped: Vec<String> = Vec::new();
            for (index, saved) in self.steps.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) { return Err(error(Path::new(""), "redo cancelled")); }
                Self::check(saved, false)?;
                let before = entry.steps.len();
                match apply(saved, id, index, cancel, tx, &mut entry.steps) {
                    Ok(()) => {}
                    Err(e) if matches!(saved.step, Step::Mode { .. } | Step::Tags { .. }) => {
                        skipped.push(format!("{}: {}", e.path, e.msg));
                        continue;
                    }
                    Err(e) => return Err(e),
                }
                if let Some(step) = entry.steps.get(before) {
                    match (&saved.step, step) {
                        (Step::Copied { created: old, .. }, Step::Copied { created: new, .. }) => changes.push((old.clone(), new.clone())),
                        (Step::MadeFile { identity: old, .. }, Step::MadeFile { identity: new, .. }) => changes.push((old.clone(), new.clone())),
                        (Step::MadeDir { identity: old, .. }, Step::MadeDir { identity: new, .. }) => changes.push((old.clone(), new.clone())),
                        (Step::Linked { identity: old, .. }, Step::Linked { identity: new, .. }) => changes.push((old.clone(), new.clone())),
                        (_, Step::Moved { after, .. }) => {
                            if let Some(old) = &saved.input { changes.push((old.clone(), after.clone())); }
                        }
                        _ => {}
                    }
                }
            }
            if !skipped.is_empty() {
                return Err(error(Path::new(""), &left_in_place(&skipped)));
            }
            Ok(self.op.clone())
        })();
        (entry, changes, result.map_err(|mut e| { e.where_ = "redo".into(); e }))
    }
}

fn apply(saved: &ReplayStep, id: usize, index: usize, cancel: &AtomicBool, tx: &Sender<OpMsg>, steps: &mut Vec<Step>) -> Result<(), FleaError> {
    match &saved.step {
        Step::Copied { from, to, .. } | Step::Moved { from, to, .. } => {
            let identity = ItemIdentity::inspect(from)?;
            let name = from.file_name().unwrap_or_default().to_string_lossy().to_string();
            let mut last = Instant::now() - PROGRESS_EVERY;
            let mut sink = |bytes, total| {
                if last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    // corner: a redo runs no sweep of its own, so its card counts bytes without a total, which is
                    // the same state a batch shows while it is still counting.
                    let _ = tx.send(OpMsg::Progress { id, index, name: name.clone(), bytes, total, scanned: 0 });
                }
            };
            let mut durability = super::durable::Durability::begin(to.parent().unwrap_or(to));
            durability.rename_fat = matches!(saved.step, Step::Copied { rename_fat: true, .. } | Step::Moved { rename_fat: true, .. });
            let moving = matches!(saved.step, Step::Moved { .. });
            let mut progress = Progress { cancel, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: Some(&mut durability), for_move: moving };
            let result = if moving { move_any(from, to, &mut progress) } else { copy_any(from, to, &mut progress) };
            let partial = progress.partial.take();
            drop(progress);
            if result.is_ok() {
                // The bytes landed; only the folder confirmation can still fail, so the step stays journalled either way.
                let step = if moving { undo::moved(from, to, identity) } else { undo::copied(from, to, identity) };
                let flushed = durability.flush_dirs();
                let mut step = step?;
                if durability.rename_fat { super::fatnames::mark_step(&mut step); }
                steps.push(step);
                if flushed.is_err() {
                    return Err(error(to, super::durable::DIR_UNCONFIRMED));
                }
            } else if let Some(partial) = partial {
                let mut step = undo::copied(from, &partial, identity)?;
                if durability.rename_fat { super::fatnames::mark_step(&mut step); }
                steps.push(step);
            }
            result
        }
        Step::MadeDir { path, .. } => {
            std::fs::create_dir(path).map_err(|e| from_io("redo", &path.to_string_lossy(), &e))?;
            steps.push(Step::MadeDir { path: path.clone(), identity: ItemIdentity::inspect(path)? });
            Ok(())
        }
        Step::RemovedEmptyDir { path, mode } => super::mergeapply::redo_emptied(path, *mode, steps),
        Step::MadeFile { path, .. } => {
            let parent = path.parent().ok_or_else(|| error(path, "the new file has no parent"))?;
            let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| error(path, "the filename cannot be represented"))?;
            let (path, identity) = super::menu_actions::create_file(parent, name).map_err(|e| error(path, &e))?;
            steps.push(Step::MadeFile { path, identity });
            Ok(())
        }
        Step::Trashed(entry) => {
            let (mut trashed, failed, _) = super::trash::trash(std::slice::from_ref(&entry.original));
            if failed != 0 || trashed.len() != 1 { return Err(error(&entry.original, "could not move the original item back to Trash")); }
            steps.push(Step::Trashed(trashed.remove(0)));
            Ok(())
        }
        Step::Tags { path, before, after, dev, ino, born, generation } => super::tagsundo::replay(path, before, after, (*dev, *ino), (*born, *generation), &saved.step, steps),
        Step::Created { path } => Err(error(path, "this operation cannot be redone")),
        // A barrier never reaches a replay; refusing here keeps it from running as a file.
        Step::Barrier => Err(error(Path::new(""), "That operation was too large to undo.")),
        // A link replays through the same exclusive create, so a name taken
        // since refuses honestly and a fresh identity is recorded.
        Step::Linked { path, source, kind, .. } => {
            match kind {
                super::link::LinkKind::Absolute => super::link::create_absolute(source, path),
                super::link::LinkKind::Hard => super::link::create_hard(source, path),
                super::link::LinkKind::Relative => super::link::create_relative(source, path),
            }.map_err(|e| error(path, &e.msg))?;
            steps.push(Step::Linked { path: path.clone(), source: source.clone(),
                kind: kind.clone(), identity: ItemIdentity::inspect(path)? });
            Ok(())
        }
        Step::Mode { path, before, after, dev, ino, born, generation } => {
            super::permissions::chmod_pinned(path, *dev, *ino, (*born, *generation), *before, *after)
                .map_err(|msg| error(path, &msg))?;
            steps.push(saved.step.clone());
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "redo_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "redo_birth_tests.rs"]
mod birth_tests;

#[cfg(test)]
#[path = "redo_case_tests.rs"]
mod case_tests;
