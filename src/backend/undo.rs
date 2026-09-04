// The undo journal, designed in from the first operation, which is why nothing in Flea needs a confirm dialog.
use crate::backend::ops::rename_noreplace_compatible;
use crate::backend::trash;
use crate::error::{from_io, FleaError};
use std::path::PathBuf;

// One reversible step. An operation is a list of these, reversed newest first.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    // A rename or a move: the entry now lives at `to` and came from `from`.
    Moved { from: PathBuf, to: PathBuf },
    // This operation created `path`, so reversing it removes that path; never a path the operation only read.
    Created { path: PathBuf },
    // This operation made the empty directory `path`; reversing it removes it only while it is still
    // empty, because anything inside it now was put there by someone else, never by this operation.
    MadeDir { path: PathBuf },
    // This operation trashed what was at `original`, and the trash holds it under `uri`.
    Trashed(trash::Entry),
}

// One user-visible operation, however many steps it took, named the way the status bar already named it.
#[derive(Clone, Debug)]
pub struct Entry {
    pub op: String,
    pub steps: Vec<Step>,
}

// The operations design's own number: a 50-entry ring costs nothing to reason about and is process-lifetime, not persisted.
const DEPTH: usize = 50;

pub struct Journal {
    entries: Vec<Entry>,
}

impl Journal {
    pub fn new() -> Journal {
        Journal { entries: Vec::new() }
    }

    // An operation that changed nothing records nothing, so undo never reports a no-op as work.
    pub fn push(&mut self, entry: Entry) {
        if entry.steps.is_empty() {
            return;
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
            reverse(step)?;
        }
        Ok(entry.op)
    }
}

fn reverse(step: &Step) -> Result<(), FleaError> {
    match step {
        // Back the way it came, and still refusing to clobber: something may occupy the old name now.
        Step::Moved { from, to } => rename_noreplace_compatible(to, from),
        Step::Created { path } => remove(path),
        Step::MadeDir { path } => remove_empty(path),
        Step::Trashed(entry) => trash::restore(entry),
    }
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
        j.push(entry("rename", vec![Step::Moved { from: from.clone(), to: to.clone() }]));
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
        j.push(entry("rename", vec![Step::Moved { from: from.clone(), to: to.clone() }]));
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
                Step::Moved { from: d.join("first.txt"), to: a.clone() },
                Step::Created { path: d.file("second.txt", "s") },
            ],
        ));
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
        let e = j.undo().expect_err("the missing path must fail");
        assert_eq!(e.where_, "undo");
        assert!(d.join("keeper.txt").exists(), "the step behind the failure was not reversed");
    }

    #[test]
    fn undoing_a_new_folder_removes_it_while_it_is_still_empty() {
        let d = TestDir::new("undomkdir");
        let made = d.dir("fresh");
        let mut j = Journal::new();
        j.push(entry("mkdir", vec![Step::MadeDir { path: made.clone() }]));
        assert_eq!(j.undo().expect("undo"), "mkdir");
        assert!(!made.exists());
    }

    #[test]
    fn undoing_a_new_folder_the_user_has_filled_refuses_and_keeps_what_is_inside() {
        let d = TestDir::new("undomkdirfilled");
        let made = d.dir("fresh");
        std::fs::write(made.join("theirs.txt"), "not ours to remove").unwrap();
        let mut j = Journal::new();
        j.push(entry("mkdir", vec![Step::MadeDir { path: made.clone() }]));
        let e = j.undo().expect_err("must refuse rather than delete what the operation did not put there");
        assert_eq!(e.where_, "undo");
        assert_eq!(e.msg, "the new folder has been filled since, so undo left it in place");
        assert_eq!(std::fs::read_to_string(made.join("theirs.txt")).unwrap(), "not ours to remove");
        // Spent like every failed reversal, so the next undo reaches the operation before this one.
        assert!(j.is_empty());
    }
}
