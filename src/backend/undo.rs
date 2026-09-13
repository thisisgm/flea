// The undo journal, designed in from the first operation, which is why nothing in Flea needs a confirm dialog.
use crate::backend::renamecompat::rename_path;
use crate::backend::trash;
use crate::error::{from_io, FleaError};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct ItemIdentity {
    dev: u64,
    ino: u64,
    kind: u32,
    changed: (i64, i64),
}

impl ItemIdentity {
    pub fn record(meta: &std::fs::Metadata) -> Self {
        Self { dev: meta.dev(), ino: meta.ino(), kind: meta.mode() & 0o170000, changed: (meta.ctime(), meta.ctime_nsec()) }
    }
    pub fn inspect(path: &std::path::Path) -> Result<Self, FleaError> {
        path.symlink_metadata().map(|meta| Self::record(&meta))
            .map_err(|e| from_io("journal", &path.to_string_lossy(), &e))
    }
    pub fn same_item(&self, other: &Self) -> bool {
        self.dev == other.dev && self.ino == other.ino && self.kind == other.kind
    }
}

// One reversible step. An operation is a list of these, reversed newest first.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    // A rename or a move: the entry now lives at `to` and came from `from`.
    Moved { from: PathBuf, to: PathBuf, before: ItemIdentity, after: ItemIdentity },
    // This operation created `path`, so reversing it removes that path; never a path the operation only read.
    Created { path: PathBuf },
    Copied { from: PathBuf, to: PathBuf, source: ItemIdentity, created: ItemIdentity },
    // This operation made the empty directory `path`; reversing it removes it only while it is still
    // empty, because anything inside it now was put there by someone else, never by this operation.
    MadeDir { path: PathBuf, identity: ItemIdentity },
    // New File is removable only while it remains the same untouched empty regular file.
    MadeFile { path: PathBuf, identity: ItemIdentity },
    // This operation trashed what was at `original`, and the trash holds it under `uri`.
    Trashed(trash::Entry),
}

// One user-visible operation, however many steps it took, named the way the status bar already named it.
#[derive(Clone, Debug)]
pub struct Entry {
    pub op: String,
    pub steps: Vec<Step>,
}

impl Entry {
    pub(crate) fn rebase(&mut self, old: &ItemIdentity, new: &ItemIdentity) {
        for step in &mut self.steps {
            match step {
                Step::Moved { before, after, .. } => {
                    if before == old { *before = new.clone(); }
                    if after == old { *after = new.clone(); }
                }
                Step::Copied { source, created, .. } => {
                    if source == old { *source = new.clone(); }
                    if created == old { *created = new.clone(); }
                }
                Step::MadeFile { identity, .. } | Step::MadeDir { identity, .. } if identity == old => *identity = new.clone(),
                _ => {}
            }
        }
    }
}

// The operations design's own number: a 50-entry ring costs nothing to reason about and is process-lifetime, not persisted.
const DEPTH: usize = 50;

pub struct Journal {
    entries: Vec<Entry>,
    redo: Vec<Result<super::redo::Replay, FleaError>>,
}

impl Journal {
    pub fn new() -> Journal {
        Journal { entries: Vec::new(), redo: Vec::new() }
    }

    // An operation that changed nothing records nothing, so undo never reports a no-op as work.
    pub fn push(&mut self, entry: Entry) {
        if entry.steps.is_empty() {
            return;
        }
        self.redo.clear();
        for step in &entry.steps {
            if let Step::Moved { before, after, .. } = step { self.rebase(before, after); }
        }
        self.entries.push(entry);
        if self.entries.len() > DEPTH {
            self.entries.remove(0);
        }
    }

    // Test-only: production reads the journal by undoing it, never by asking how deep it is.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // The whole entry is reversed or the failure is reported; a step that fails stops the rest, because
    // continuing past it would leave the operation half-reversed with nothing recording which half.
    pub fn undo(&mut self) -> Result<String, FleaError> {
        let entry = match self.entries.pop() {
            Some(e) => e,
            None => return Err(err("there is nothing to undo")),
        };
        for step in entry.steps.iter().rev() {
            if let Some((old, new)) = reverse(step)? { self.rebase(&old, &new); }
        }
        let op = entry.op.clone();
        self.redo.push(super::redo::Replay::capture(entry));
        Ok(op)
    }

    pub fn redo_info(&self) -> Result<(String, usize), FleaError> {
        match self.redo.last() {
            Some(Ok(replay)) => Ok((replay.op().to_string(), replay.len())),
            Some(Err(error)) => Err(FleaError { where_: "redo".into(), path: error.path.clone(), msg: error.msg.clone() }),
            None => Err(FleaError { where_: "redo".into(), path: String::new(), msg: "there is nothing to redo".into() }),
        }
    }

    pub fn redo(&mut self, id: usize, cancel: &std::sync::atomic::AtomicBool,
                tx: &std::sync::mpsc::Sender<super::opsreq::OpMsg>) -> Result<String, FleaError> {
        let replay = self.redo.pop().ok_or_else(|| FleaError { where_: "redo".into(), path: String::new(), msg: "there is nothing to redo".into() })??;
        let (entry, changes, result) = replay.run(id, cancel, tx);
        for (old, new) in changes { self.rebase(&old, &new); }
        if !entry.steps.is_empty() { self.entries.push(entry); }
        if result.is_err() { self.redo.clear(); }
        result
    }

    fn rebase(&mut self, old: &ItemIdentity, new: &ItemIdentity) {
        for entry in &mut self.entries { entry.rebase(old, new); }
        for replay in self.redo.iter_mut().flatten() { replay.rebase(old, new); }
    }
}

pub fn copied(from: &std::path::Path, to: &std::path::Path, source: ItemIdentity) -> Result<Step, FleaError> {
    Ok(Step::Copied { from: from.to_path_buf(), to: to.to_path_buf(), source, created: ItemIdentity::inspect(to)? })
}

pub fn moved(from: &std::path::Path, to: &std::path::Path, before: ItemIdentity) -> Result<Step, FleaError> {
    Ok(Step::Moved { from: from.to_path_buf(), to: to.to_path_buf(), before, after: ItemIdentity::inspect(to)? })
}

fn reverse(step: &Step) -> Result<Option<(ItemIdentity, ItemIdentity)>, FleaError> {
    match step {
        // Back the way it came, and still refusing to clobber: something may occupy the old name now.
        Step::Moved { from, to, after, .. } => {
            let current = ItemIdentity::inspect(to)?;
            if !after.same_item(&current) {
                return Err(FleaError { where_: "undo".into(), path: to.to_string_lossy().into(), msg: "the moved item was replaced, so undo left it in place".into() });
            }
            rename_path(to, from)?;
            return Ok(if current == *after { Some((current, ItemIdentity::inspect(from)?)) } else { None });
        }
        Step::Created { path } => remove(path)?,
        Step::Copied { to, created, .. } => {
            if ItemIdentity::inspect(to)? != *created {
                return Err(FleaError { where_: "undo".into(), path: to.to_string_lossy().into(),
                    msg: "the copied item changed since this operation, so undo left it in place".into() });
            }
            remove(to)?
        }
        Step::MadeDir { path, identity } => {
            if !identity.same_item(&ItemIdentity::inspect(path)?) {
                return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(), msg: "the new folder was replaced, so undo left it in place".into() });
            }
            remove_empty(path)?;
        }
        Step::MadeFile { path, identity } => remove_new_file(path, identity)?,
        Step::Trashed(entry) => trash::restore(entry)?,
    }
    Ok(None)
}

fn remove_new_file(path: &PathBuf, identity: &ItemIdentity) -> Result<(), FleaError> {
    let meta = path.symlink_metadata().map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
    if !meta.is_file() || meta.len() != 0 || ItemIdentity::record(&meta) != *identity {
        return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
            msg: "the new file changed since creation, so undo left it in place".into() });
    }
    std::fs::remove_file(path).map_err(|e| from_io("undo", &path.to_string_lossy(), &e))
}

// Only ever a path this operation itself created, so a directory it made is removed with its contents.
fn remove(path: &PathBuf) -> Result<(), FleaError> {
    let meta = path
        .symlink_metadata()
        .map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
    let r = if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.map_err(|e| from_io("undo", &path.to_string_lossy(), &e))
}

// Only ever an empty directory this operation made. A folder the user has filled since is theirs now, so
// undo refuses and leaves it, the way a rename undo refuses a name something else has taken meanwhile.
fn remove_empty(path: &PathBuf) -> Result<(), FleaError> {
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => Err(FleaError {
            where_: "undo".to_string(),
            path: path.to_string_lossy().to_string(),
            msg: "the new folder has been filled since, so undo left it in place".to_string(),
        }),
        Err(e) => Err(from_io("undo", &path.to_string_lossy(), &e)),
    }
}

fn err(msg: &str) -> FleaError {
    FleaError { where_: "undo".to_string(), path: String::new(), msg: msg.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    fn entry(op: &str, steps: Vec<Step>) -> Entry {
        Entry { op: op.to_string(), steps }
    }

    #[test]
    fn an_empty_journal_answers_an_error_rather_than_claiming_it_undid_something() {
        let mut j = Journal::new();
        let e = j.undo().expect_err("nothing to undo");
        assert_eq!(e.where_, "undo");
        assert!(e.msg.contains("nothing to undo"));
    }

    #[test]
    fn an_operation_that_changed_nothing_is_not_recorded() {
        let mut j = Journal::new();
        j.push(entry("rename", Vec::new()));
        assert!(j.is_empty(), "an empty step list would undo as a no-op reported as work");
    }

    #[test]
    fn the_ring_is_bounded_at_fifty_and_drops_its_oldest() {
        let mut j = Journal::new();
        for i in 0..DEPTH + 10 {
            j.push(entry(&format!("op {}", i), vec![Step::Created { path: PathBuf::from("/x") }]));
        }
        assert_eq!(j.len(), DEPTH);
    }

    #[test]
    fn undoing_a_rename_puts_the_old_name_back() {
        let d = TestDir::new("undorename");
        let from = d.join("before.txt");
        let to = d.file("after.txt", "body");
        let mut j = Journal::new();
        j.push(entry("rename", vec![moved(&from, &to, ItemIdentity::inspect(&to).unwrap()).unwrap()]));
        assert_eq!(j.undo().expect("undo"), "rename");
        assert!(from.exists(), "the original name is back");
        assert!(!to.exists(), "the new name is gone");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "body");
    }

    #[test]
    fn undoing_a_rename_refuses_to_clobber_a_file_that_took_the_old_name_since() {
        let d = TestDir::new("undoclobber");
        let from = d.file("before.txt", "something else wrote this");
        let to = d.file("after.txt", "body");
        let mut j = Journal::new();
        j.push(entry("rename", vec![moved(&from, &to, ItemIdentity::inspect(&to).unwrap()).unwrap()]));
        let e = j.undo().expect_err("must refuse rather than destroy the newer file");
        assert_eq!(e.where_, "rename");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "something else wrote this");
    }

    #[test]
    fn undoing_a_duplicate_removes_only_the_copy_it_created() {
        let d = TestDir::new("undodup");
        let original = d.file("doc.txt", "original");
        let copy = d.file("doc copy.txt", "original");
        let mut j = Journal::new();
        j.push(entry("duplicate", vec![Step::Created { path: copy.clone() }]));
        d.assert_contains(&copy);
        j.undo().expect("undo");
        assert!(!copy.exists(), "the copy is gone");
        assert!(original.exists(), "the file it was copied from is untouched");
    }

    #[test]
    fn undoing_a_created_directory_takes_its_contents_with_it() {
        let d = TestDir::new("undodir");
        let made = d.dir("copied-tree");
        std::fs::write(made.join("inside.txt"), "body").unwrap();
        let mut j = Journal::new();
        j.push(entry("copy", vec![Step::Created { path: made.clone() }]));
        d.assert_contains(&made);
        j.undo().expect("undo");
        assert!(!made.exists());
    }

    #[test]
    fn the_steps_of_one_operation_reverse_newest_first() {
        let d = TestDir::new("undoorder");
        // A move recorded as two steps: undoing them out of order would leave b.txt where a.txt belongs.
        let a = d.file("a.txt", "a");
        let mut j = Journal::new();
        j.push(entry(
            "move",
            vec![
                moved(&d.join("first.txt"), &a, ItemIdentity::inspect(&a).unwrap()).unwrap(),
                Step::Created { path: d.file("second.txt", "s") },
            ],
        ));
        d.assert_contains(&d.join("second.txt"));
        j.undo().expect("undo");
        assert!(!d.join("second.txt").exists(), "the newest step reversed");
        assert!(d.join("first.txt").exists(), "the oldest step reversed too");
    }

    #[test]
    fn a_step_that_fails_stops_the_rest_rather_than_half_reversing() {
        let d = TestDir::new("undofail");
        let mut j = Journal::new();
        j.push(entry(
            "copy",
            vec![
                Step::Created { path: d.file("keeper.txt", "k") },
                // Reversed first, and it cannot be: nothing is at this path to remove.
                Step::Created { path: d.join("never-existed.txt") },
            ],
        ));
        d.assert_contains(&d.join("keeper.txt"));
        d.assert_contains(&d.join("never-existed.txt"));
        let e = j.undo().expect_err("the missing path must fail");
        assert_eq!(e.where_, "undo");
        assert!(d.join("keeper.txt").exists(), "the step behind the failure was not reversed");
    }

    #[test]
    fn undoing_a_new_folder_removes_it_while_it_is_still_empty() {
        let d = TestDir::new("undomkdir");
        let made = d.dir("fresh");
        let mut j = Journal::new();
        j.push(entry("mkdir", vec![Step::MadeDir { path: made.clone(), identity: ItemIdentity::inspect(&made).unwrap() }]));
        d.assert_contains(&made);
        assert_eq!(j.undo().expect("undo"), "mkdir");
        assert!(!made.exists());
    }

    #[test]
    fn undoing_a_new_folder_the_user_has_filled_refuses_and_keeps_what_is_inside() {
        let d = TestDir::new("undomkdirfilled");
        let made = d.dir("fresh");
        std::fs::write(made.join("theirs.txt"), "not ours to remove").unwrap();
        let mut j = Journal::new();
        j.push(entry("mkdir", vec![Step::MadeDir { path: made.clone(), identity: ItemIdentity::inspect(&made).unwrap() }]));
        d.assert_contains(&made);
        let e = j.undo().expect_err("must refuse rather than delete what the operation did not put there");
        assert_eq!(e.where_, "undo");
        assert_eq!(e.msg, "the new folder has been filled since, so undo left it in place");
        assert_eq!(std::fs::read_to_string(made.join("theirs.txt")).unwrap(), "not ours to remove");
        // Spent like every failed reversal, so the next undo reaches the operation before this one.
        assert!(j.is_empty());
    }
}
