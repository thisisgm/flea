// Redo repeats journaled operations against their recorded inputs and never replaces a destination.
use super::copyfile::{copy_any, move_any, Progress};
use super::opsreq::{OpMsg, PROGRESS_EVERY};
use super::undo::{self, Entry, ItemIdentity, Step};
use crate::error::{from_io, FleaError};
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

fn error(path: &Path, message: &str) -> FleaError {
    FleaError { where_: "redo".into(), path: path.to_string_lossy().into(), msg: message.into() }
}

fn source(step: &Step) -> Option<&Path> {
    match step {
        Step::Moved { from, .. } | Step::Copied { from, .. } => Some(from),
        Step::Trashed(entry) => Some(&entry.original),
        _ => None,
    }
}

fn destination(step: &Step) -> Option<&Path> {
    match step {
        Step::Moved { to, .. } | Step::Copied { to, .. } => Some(to),
        Step::MadeDir { path, .. } | Step::MadeFile { path, .. } => Some(path),
        _ => None,
    }
}

impl Replay {
    pub fn capture(entry: Entry) -> Result<Self, FleaError> {
        let mut steps = Vec::new();
        for step in entry.steps {
            if matches!(step, Step::Created { .. }) {
                return Err(error(Path::new(""), "this interrupted operation has no recorded source to redo"));
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
    pub fn rebase(&mut self, old: &ItemIdentity, new: &ItemIdentity) {
        for saved in &mut self.steps {
            if saved.input.as_ref() == Some(old) { saved.input = Some(new.clone()); }
            if let Some((_, identity)) = &mut saved.parent {
                if identity.same_item(old) { *identity = new.clone(); }
            }
            let mut entry = Entry { op: String::new(), steps: vec![saved.step.clone()] };
            entry.rebase(old, new);
            saved.step = entry.steps.remove(0);
        }
    }
    fn check(saved: &ReplayStep) -> Result<(), FleaError> {
        if let (Some(path), Some(identity)) = (source(&saved.step), &saved.input) {
            if !path.is_absolute() || ItemIdentity::inspect(path)? != *identity {
                return Err(error(path, "the original item changed or was replaced; redo left it in place"));
            }
        }
        if let Some((path, identity)) = &saved.parent {
            if !identity.same_item(&ItemIdentity::inspect(path)?) {
                return Err(error(path, "the destination folder was replaced; redo was refused"));
            }
        }
        if let Some(path) = destination(&saved.step) {
            match path.symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(from_io("redo", &path.to_string_lossy(), &e)),
                Ok(_) => return Err(error(path, "the destination already exists; redo never replaces it")),
            }
        }
        Ok(())
    }
    pub fn run(self, id: usize, cancel: &AtomicBool, tx: &Sender<OpMsg>)
        -> (Entry, Vec<(ItemIdentity, ItemIdentity)>, Result<String, FleaError>) {
        let mut entry = Entry { op: self.op.clone(), steps: Vec::new() };
        let mut changes = Vec::new();
        let result = (|| {
            for saved in &self.steps { Self::check(saved)?; }
            for (index, saved) in self.steps.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) { return Err(error(Path::new(""), "redo cancelled")); }
                Self::check(saved)?;
                let before = entry.steps.len();
                apply(saved, id, index, cancel, tx, &mut entry.steps)?;
                if let Some(step) = entry.steps.get(before) {
                    match (&saved.step, step) {
                        (Step::Copied { created: old, .. }, Step::Copied { created: new, .. }) => changes.push((old.clone(), new.clone())),
                        (Step::MadeFile { identity: old, .. }, Step::MadeFile { identity: new, .. }) => changes.push((old.clone(), new.clone())),
                        (Step::MadeDir { identity: old, .. }, Step::MadeDir { identity: new, .. }) => changes.push((old.clone(), new.clone())),
                        (_, Step::Moved { after, .. }) => {
                            if let Some(old) = &saved.input { changes.push((old.clone(), after.clone())); }
                        }
                        _ => {}
                    }
                }
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
                    let _ = tx.send(OpMsg::Progress { id, index, name: name.clone(), bytes, total });
                }
            };
            let mut progress = Progress { cancel, on_bytes: &mut sink, partial: None };
            let moving = matches!(saved.step, Step::Moved { .. });
            let result = if moving { move_any(from, to, &mut progress) } else { copy_any(from, to, &mut progress) };
            if result.is_ok() {
                steps.push(if moving { undo::moved(from, to, identity)? } else { undo::copied(from, to, identity)? });
            } else if let Some(partial) = progress.partial {
                steps.push(undo::copied(from, &partial, identity)?);
            }
            result
        }
        Step::MadeDir { path, .. } => {
            std::fs::create_dir(path).map_err(|e| from_io("redo", &path.to_string_lossy(), &e))?;
            steps.push(Step::MadeDir { path: path.clone(), identity: ItemIdentity::inspect(path)? });
            Ok(())
        }
        Step::MadeFile { path, .. } => {
            let parent = path.parent().ok_or_else(|| error(path, "the new file has no parent"))?;
            let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| error(path, "the filename cannot be represented"))?;
            let (path, identity) = super::menu_actions::create_file(parent, name).map_err(|e| error(path, &e))?;
            steps.push(Step::MadeFile { path, identity });
            Ok(())
        }
        Step::Trashed(entry) => {
            let (mut trashed, failed) = super::trash::trash(std::slice::from_ref(&entry.original));
            if failed != 0 || trashed.len() != 1 { return Err(error(&entry.original, "could not move the original item back to Trash")); }
            steps.push(Step::Trashed(trashed.remove(0)));
            Ok(())
        }
        Step::Created { path } => Err(error(path, "this operation has no recorded replay source")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{ops, testdir::TestDir, undo::Journal};
    use std::sync::mpsc::channel;

    fn guard(sandbox: &TestDir, paths: &[&Path]) {
        for path in paths {
            assert!(!path.as_os_str().is_empty() && path.is_absolute() && path.starts_with(sandbox.path()));
        }
        assert!(sandbox.path().join(".flea-test-sandbox").is_file());
    }
    fn redo(journal: &mut Journal) -> Result<String, FleaError> {
        let (tx, _rx) = channel();
        journal.redo(1, &AtomicBool::new(false), &tx)
    }

    #[test]
    fn copy_rename_chain_survives_two_complete_undo_redo_cycles() {
        let sandbox = TestDir::new("redo-chain");
        let original = sandbox.file("original", "keep this payload");
        let (result, steps) = ops::duplicate(&original);
        let copy = result.unwrap();
        let mut journal = Journal::new();
        journal.push(Entry { op: "duplicate".into(), steps });
        let (renamed, steps) = ops::rename(&copy, "renamed").unwrap();
        journal.push(Entry { op: "rename".into(), steps });
        for _ in 0..2 {
            guard(&sandbox, &[&original, &copy, &renamed]);
            assert_eq!(journal.undo().unwrap(), "rename");
            guard(&sandbox, &[&original, &copy, &renamed]);
            assert_eq!(journal.undo().unwrap(), "duplicate");
            assert!(!copy.exists() && !renamed.exists());
            assert_eq!(redo(&mut journal).unwrap(), "duplicate");
            assert_eq!(redo(&mut journal).unwrap(), "rename");
            assert_eq!(std::fs::read_to_string(&renamed).unwrap(), "keep this payload");
            assert_eq!(std::fs::read_to_string(&original).unwrap(), "keep this payload");
        }
    }

    #[test]
    fn recreated_parent_rebinds_child_destination_without_accepting_a_foreign_parent() {
        let sandbox = TestDir::new("redo-parent");
        let mut journal = Journal::new();
        let (parent, steps) = ops::mkdir(sandbox.path(), "parent").unwrap();
        journal.push(Entry { op: "mkdir".into(), steps });
        let (child, steps) = ops::mkdir(&parent, "child").unwrap();
        journal.push(Entry { op: "mkdir".into(), steps });
        guard(&sandbox, &[&parent, &child]);
        journal.undo().unwrap();
        guard(&sandbox, &[&parent, &child]);
        journal.undo().unwrap();
        redo(&mut journal).unwrap();
        redo(&mut journal).unwrap();
        assert!(child.is_dir());
        guard(&sandbox, &[&parent, &child]);
        journal.undo().unwrap();
        let moved = sandbox.join("old-parent");
        guard(&sandbox, &[&parent, &moved]);
        std::fs::rename(&parent, &moved).unwrap();
        std::fs::create_dir(&parent).unwrap();
        assert!(redo(&mut journal).unwrap_err().msg.contains("replaced"));
        assert!(!child.exists());
    }

    #[test]
    fn redo_refuses_changed_sources_and_destination_collisions() {
        let sandbox = TestDir::new("redo-refusal");
        let original = sandbox.file("original", "before");
        let (result, steps) = ops::duplicate(&original);
        let copy = result.unwrap();
        let mut journal = Journal::new();
        journal.push(Entry { op: "duplicate".into(), steps });
        guard(&sandbox, &[&original, &copy]);
        journal.undo().unwrap();
        std::fs::write(&original, "changed payload").unwrap();
        assert!(redo(&mut journal).unwrap_err().msg.contains("changed"));
        assert!(!copy.exists());
        let (result, steps) = ops::duplicate(&original);
        let copy = result.unwrap();
        journal.push(Entry { op: "duplicate".into(), steps });
        guard(&sandbox, &[&original, &copy]);
        journal.undo().unwrap();
        std::fs::write(&copy, "foreign destination").unwrap();
        assert!(redo(&mut journal).unwrap_err().msg.contains("already exists"));
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "foreign destination");
    }

    #[test]
    fn new_file_is_recreated_but_changed_or_replaced_files_survive_undo() {
        let sandbox = TestDir::new("redo-newfile");
        let (path, identity) = super::super::menu_actions::create_file(sandbox.path(), "new").unwrap();
        let mut journal = Journal::new();
        journal.push(Entry { op: "newfile".into(), steps: vec![Step::MadeFile { path: path.clone(), identity }] });
        guard(&sandbox, &[&path]);
        journal.undo().unwrap();
        redo(&mut journal).unwrap();
        assert_eq!(path.metadata().unwrap().len(), 0);
        std::fs::write(&path, "user content").unwrap();
        guard(&sandbox, &[&path]);
        assert!(journal.undo().unwrap_err().msg.contains("changed"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user content");
        let (empty, identity) = super::super::menu_actions::create_file(sandbox.path(), "empty").unwrap();
        journal.push(Entry { op: "newfile".into(), steps: vec![Step::MadeFile { path: empty.clone(), identity }] });
        let saved = sandbox.join("saved");
        guard(&sandbox, &[&empty, &saved]);
        std::fs::rename(&empty, &saved).unwrap();
        std::fs::write(&empty, "replacement").unwrap();
        guard(&sandbox, &[&empty, &saved]);
        assert!(journal.undo().is_err());
        assert_eq!(std::fs::read_to_string(empty).unwrap(), "replacement");
    }
}
