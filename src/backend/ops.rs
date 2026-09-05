// Rename, duplicate and mkdir: the three operations that answer once, with no started or progress split.
use crate::backend::copyfile::{copy_any, remove_any, Progress};
use crate::backend::renamecompat;
use crate::backend::undo::Step;
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

// How many " copy N" names duplicate will try before giving up rather than looping forever.
const NAME_TRIES: usize = 999;
// The default a nameless mkdir starts from, then "New Folder 2" and up while the name is taken.
const NEW_FOLDER: &str = "New Folder";

// A name from the client is a trust boundary: anything with a separator would move the file out of its own directory.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

// Rename that refuses to overwrite. std::fs::rename alone can replace a target on Unix.
pub fn rename_noreplace(from: &Path, to: &Path) -> Result<(), FleaError> {
    renamecompat::rename_noreplace(from, to).map_err(|e| from_io("rename", &to.to_string_lossy(), &e))
}

// Rename uses the atomic path everywhere except a measured rclone directory EINVAL, which copies exclusively before removing its source.
pub(crate) fn rename_path(from: &Path, to: &Path) -> Result<(), FleaError> {
    match renamecompat::rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(error) if renamecompat::needs_copy_fallback(from, &error) => copy_remove_directory(from, to),
        Err(error) => Err(from_io("rename", &to.to_string_lossy(), &error)),
    }
}

fn copy_remove_directory(from: &Path, to: &Path) -> Result<(), FleaError> {
    let cancel = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut progress = Progress { cancel: &cancel, on_bytes: &mut sink, partial: None };
    if let Err(error) = copy_any(from, to, &mut progress) {
        if progress.partial.as_deref() == Some(to) {
            if let Err(cleanup) = remove_any(to) {
                return Err(named(
                    "rename",
                    to,
                    &format!("{}; partial target could not be removed: {}", error.msg, cleanup.msg),
                ));
            }
        }
        return Err(rename_error(error));
    }
    remove_any(from).map_err(rename_error)
}

fn rename_error(mut error: FleaError) -> FleaError {
    error.where_ = "rename".to_string();
    error
}

fn named(where_: &str, path: &Path, msg: &str) -> FleaError {
    FleaError {
        where_: where_.to_string(),
        path: path.to_string_lossy().to_string(),
        msg: msg.to_string(),
    }
}

// Answers with the new path and the step that puts the old name back.
pub fn rename(path: &Path, to_name: &str) -> Result<(PathBuf, Vec<Step>), FleaError> {
    if !valid_name(to_name) {
        return Err(named("rename", path, "a name cannot be empty, . or .. , or contain a separator"));
    }
    let parent = path.parent().unwrap_or(Path::new("/"));
    let to = parent.join(to_name);
    if to == path {
        // Renaming a file to its own name is not a failure and is not work, so it records nothing.
        return Ok((to, Vec::new()));
    }
    rename_path(path, &to)?;
    Ok((to.clone(), vec![Step::Moved { from: path.to_path_buf(), to }]))
}

// "backup.tar.zst" becomes "backup.tar copy.zst": Path's own stem and extension split the last dot only, and a dotfile keeps its whole name as the stem.
// Convert passes "(converted)" as the word for the parenthesised form the canvas draws; duplicate passes "copy".
pub fn copy_name(original: &Path, word: &str, n: usize) -> Option<String> {
    let name = original.file_name()?.to_str()?;
    let suffix = if n <= 1 { format!(" {}", word) } else { format!(" {} {}", word, n) };
    let stem = Path::new(name).file_stem()?.to_str()?;
    match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) => Some(format!("{}{}.{}", stem, suffix, ext)),
        None => Some(format!("{}{}", stem, suffix)),
    }
}

// The first " copy N" sibling not already taken. The copy itself is still created exclusively, so this is a starting point and not a promise.
pub fn free_copy_path(original: &Path, word: &str) -> Option<PathBuf> {
    let parent = original.parent()?;
    for n in 1..=NAME_TRIES {
        let candidate = parent.join(copy_name(original, word, n)?);
        if candidate.symlink_metadata().is_err() {
            return Some(candidate);
        }
    }
    None
}

// A same-directory copy, which for a directory row is the whole tree; cancellation belongs to transfers, so this one runs to its end.
// Answers the outcome and, either way, the steps it left on disk: the copy, or the partial a failure left for undo to remove.
pub fn duplicate(path: &Path) -> (Result<PathBuf, FleaError>, Vec<Step>) {
    let dst = match free_copy_path(path, "copy") {
        Some(d) => d,
        None => return (Err(named("duplicate", path, "every copy name for this file is already taken")), Vec::new()),
    };
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None };
    match copy_any(path, &dst, &mut p) {
        Ok(()) => (Ok(dst.clone()), vec![Step::Created { path: dst }]),
        Err(e) => (Err(e), p.partial.take().into_iter().map(|path| Step::Created { path }).collect()),
    }
}

// A given name is created exactly or refused; an empty one takes the first free default, because a
// client holds a window of the listing, not the directory, so it cannot know which names are taken.
pub fn mkdir(parent: &Path, name: &str) -> Result<(PathBuf, Vec<Step>), FleaError> {
    // Relative would resolve against the backend's own working directory, which no listing ever names.
    if !parent.is_absolute() {
        return Err(named("mkdir", parent, "a parent must be an absolute path"));
    }
    let dir = if name.is_empty() {
        match free_new_folder(parent) {
            Some(d) => d,
            None => return Err(named("mkdir", parent, "every default folder name here is already taken")),
        }
    } else if valid_name(name) {
        parent.join(name)
    } else {
        return Err(named("mkdir", parent, "a name cannot be . or .., or contain a separator"));
    };
    match std::fs::create_dir(&dir) {
        Ok(()) => Ok((dir.clone(), vec![Step::MadeDir { path: dir }])),
        // create_dir, never create_dir_all: a name already taken is a collision and must never merge.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(named("mkdir", &dir, "a folder or file with that name already exists"))
        }
        Err(e) => Err(from_io("mkdir", &dir.to_string_lossy(), &e)),
    }
}

// "New Folder", then "New Folder 2" and up: the first sibling not already taken by anything. The
// directory is still created exclusively, so like free_copy_path this is a start and not a promise.
fn free_new_folder(parent: &Path) -> Option<PathBuf> {
    for n in 1..=NAME_TRIES {
        let name = if n <= 1 { NEW_FOLDER.to_string() } else { format!("{} {}", NEW_FOLDER, n) };
        let candidate = parent.join(name);
        if candidate.symlink_metadata().is_err() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_name_with_a_separator_is_refused_before_any_syscall() {
        assert!(valid_name("ordinary.txt"));
        assert!(valid_name(".bashrc"));
        assert!(valid_name("a name with spaces"));
        assert!(!valid_name(""), "an empty name would rename onto the parent");
        assert!(!valid_name("."), "renaming to . is the directory itself");
        assert!(!valid_name(".."), "renaming to .. would climb out of the directory");
        assert!(!valid_name("../escape"), "a separator moves the file out of its own directory");
        assert!(!valid_name("sub/child"), "a separator moves the file out of its own directory");
        assert!(!valid_name("nul\0byte"), "an interior NUL truncates the path at the syscall");
    }

    #[test]
    fn rename_moves_the_file_and_records_the_step_that_puts_it_back() {
        let d = TestDir::new("rename");
        let from = d.file("before.txt", "body");
        let (to, steps) = rename(&from, "after.txt").expect("rename");
        assert_eq!(to, d.join("after.txt"));
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "body");
        assert_eq!(steps, vec![Step::Moved { from: from.clone(), to }]);
    }

    #[test]
    fn rename_refuses_to_overwrite_an_existing_file() {
        let d = TestDir::new("clobber");
        let from = d.file("source.txt", "source body");
        d.file("target.txt", "target body");
        let err = rename(&from, "target.txt").expect_err("must refuse");
        assert_eq!(err.where_, "rename");
        assert_eq!(
            std::fs::read_to_string(d.join("target.txt")).unwrap(),
            "target body",
            "the file that was already there is untouched"
        );
        assert!(from.exists(), "the source is left where it was");
    }

    #[test]
    fn copied_directory_rename_refuses_an_existing_empty_directory() {
        let d = TestDir::new("copyrenameclobber");
        let source = d.dir("source");
        std::fs::write(source.join("source.txt"), "source body").unwrap();
        let target = d.dir("target");
        let error = copy_remove_directory(&source, &target).expect_err("must refuse");
        assert_eq!(error.where_, "rename");
        assert!(source.join("source.txt").is_file(), "the source tree stays complete");
        assert!(target.is_dir(), "the directory that owned the target name stays in place");
    }

    #[test]
    fn copied_directory_rename_moves_the_tree_and_the_same_path_reverses_it() {
        let d = TestDir::new("copyrename");
        let source = d.dir("source");
        let nested = source.join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("inside.txt"), "body").unwrap();
        let target = d.join("target");
        copy_remove_directory(&source, &target).expect("rename by exclusive copy");
        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(target.join("nested/inside.txt")).unwrap(), "body");

        copy_remove_directory(&target, &source).expect("undo by exclusive copy");
        assert!(!target.exists());
        assert_eq!(std::fs::read_to_string(source.join("nested/inside.txt")).unwrap(), "body");
    }

    #[test]
    fn copied_directory_rename_removes_its_partial_target_after_copy_failure() {
        let d = TestDir::new("copyrenamepartial");
        let source = d.dir("source");
        let _socket = std::os::unix::net::UnixListener::bind(source.join("socket")).unwrap();
        let target = d.join("target");
        copy_remove_directory(&source, &target).expect_err("a socket cannot be copied");
        assert!(source.join("socket").exists(), "the source remains after a failed copy");
        assert!(!target.exists(), "the failed rename leaves no unjournaled partial target");
    }

    // corner: runs as a plain user, where a directory without its write bit cannot remove its child.
    #[test]
    fn copied_directory_rename_keeps_the_complete_copy_when_source_removal_fails() {
        let d = TestDir::new("copyrenameremove");
        let source = d.dir("source");
        std::fs::write(source.join("inside.txt"), "body").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o555)).unwrap();
        let target = d.join("target");
        let error = copy_remove_directory(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, "rename");
        assert_eq!(std::fs::read_to_string(source.join("inside.txt")).unwrap(), "body");
        assert_eq!(std::fs::read_to_string(target.join("inside.txt")).unwrap(), "body");
    }

    #[test]
    fn renaming_a_file_to_its_own_name_is_not_work() {
        let d = TestDir::new("samename");
        let from = d.file("same.txt", "body");
        let (_, steps) = rename(&from, "same.txt").expect("a no-op rename is not an error");
        assert!(steps.is_empty(), "nothing changed, so undo must have nothing to reverse");
        assert!(from.exists());
    }

    #[test]
    fn a_copy_name_keeps_the_last_extension_and_leaves_a_dotfile_whole() {
        let p = Path::new("/x/backup.tar.zst");
        assert_eq!(copy_name(p, "copy", 1).unwrap(), "backup.tar copy.zst");
        assert_eq!(copy_name(p, "copy", 2).unwrap(), "backup.tar copy 2.zst");
        assert_eq!(copy_name(Path::new("/x/notes"), "copy", 1).unwrap(), "notes copy");
        assert_eq!(copy_name(Path::new("/x/.bashrc"), "copy", 1).unwrap(), ".bashrc copy");
        assert_eq!(
            copy_name(Path::new("/x/photo.jpg"), "copy", 1).unwrap(),
            "photo copy.jpg",
            "the operations design names this exact result"
        );
        assert_eq!(
            copy_name(Path::new("/x/shot.png"), "(converted)", 1).unwrap(),
            "shot (converted).png",
            "convert reuses this helper with its own word, per the canvas"
        );
    }

    #[test]
    fn a_free_copy_path_steps_past_names_that_are_taken() {
        let d = TestDir::new("freename");
        let original = d.file("doc.txt", "body");
        assert_eq!(free_copy_path(&original, "copy").unwrap(), d.join("doc copy.txt"));
        d.file("doc copy.txt", "taken");
        assert_eq!(free_copy_path(&original, "copy").unwrap(), d.join("doc copy 2.txt"));
    }

    #[test]
    fn duplicate_writes_a_sibling_and_records_only_the_file_it_created() {
        let d = TestDir::new("duplicate");
        let original = d.file("photo.jpg", "pixels");
        let (outcome, steps) = duplicate(&original);
        let dst = outcome.expect("duplicate");
        assert_eq!(dst, d.join("photo copy.jpg"));
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "pixels");
        assert!(original.exists(), "the original is untouched");
        assert_eq!(steps, vec![Step::Created { path: dst }]);
    }

    #[test]
    fn duplicating_a_directory_copies_its_whole_tree() {
        let d = TestDir::new("dupdir");
        let src = d.dir("tree");
        std::fs::write(src.join("inside.txt"), "body").unwrap();
        let (outcome, _) = duplicate(&src);
        let dst = outcome.expect("duplicate");
        assert_eq!(dst, d.join("tree copy"));
        assert_eq!(std::fs::read_to_string(dst.join("inside.txt")).unwrap(), "body");
    }

    // A socket answers ENXIO to open(2) for any uid, so it forces the failure a permission error would.
    #[test]
    fn duplicating_a_tree_that_fails_short_of_the_end_still_records_the_partial_copy() {
        let d = TestDir::new("duppartial");
        let src = d.dir("tree");
        std::fs::write(src.join("inside.txt"), "body").unwrap();
        let _sock = std::os::unix::net::UnixListener::bind(src.join("sock")).expect("a socket in the source tree");
        let (outcome, steps) = duplicate(&src);
        assert!(outcome.is_err(), "the socket cannot be opened, so the tree copy fails");
        let partial = d.join("tree copy");
        assert!(partial.is_dir(), "the failure left what it had copied");
        assert_eq!(steps, vec![Step::Created { path: partial }], "and the journal gets the partial, so undo can remove it");
    }

    #[test]
    fn mkdir_makes_the_folder_and_records_the_step_that_removes_it() {
        let d = TestDir::new("mkdir");
        let (made, steps) = mkdir(d.path(), "photos").expect("mkdir");
        assert_eq!(made, d.join("photos"));
        assert!(made.is_dir());
        assert_eq!(steps, vec![Step::MadeDir { path: made }]);
    }

    #[test]
    fn mkdir_refuses_a_taken_name_with_a_sentence_and_touches_nothing() {
        let d = TestDir::new("mkdirtaken");
        let file = d.file("notes", "body");
        let err = mkdir(d.path(), "notes").expect_err("a file holds the name");
        assert_eq!(err.where_, "mkdir");
        assert_eq!(err.msg, "a folder or file with that name already exists");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "body", "the file that owns the name is untouched");
        d.dir("sub");
        let err = mkdir(d.path(), "sub").expect_err("a directory holds the name");
        assert_eq!(err.msg, "a folder or file with that name already exists", "an existing directory is a collision too, never a merge");
    }

    #[test]
    fn mkdir_refuses_the_names_rename_refuses_and_a_relative_parent() {
        let d = TestDir::new("mkdirbadname");
        for bad in [".", "..", "a/b", "../up", "nul\0byte"] {
            let err = mkdir(d.path(), bad).expect_err(bad);
            assert_eq!(err.msg, "a name cannot be . or .., or contain a separator", "{:?}", bad);
        }
        assert_eq!(mkdir(Path::new("relative"), "x").unwrap_err().msg, "a parent must be an absolute path");
        // A name of only spaces is a legal name, as it is for rename; trimming is the field's job.
        assert!(mkdir(d.path(), "   ").expect("spaces").0.is_dir());
        // Read with dotfiles, like ls -A: only the sandbox marker and the spaces folder are here.
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 2, "no refusal made anything");
    }

    #[test]
    fn an_empty_name_takes_the_first_free_new_folder() {
        let d = TestDir::new("mkdirdefault");
        assert_eq!(mkdir(d.path(), "").expect("first").0, d.join("New Folder"));
        assert_eq!(mkdir(d.path(), "").expect("second").0, d.join("New Folder 2"));
        d.file("New Folder 3", "a file holds the name");
        assert_eq!(mkdir(d.path(), "").expect("third").0, d.join("New Folder 4"), "a taken name of any kind is stepped past");
    }

    // corner: runs as a plain user, where a directory without its write bit refuses a new entry.
    #[test]
    fn mkdir_under_a_vanished_or_unwritable_parent_carries_the_os_sentence() {
        let d = TestDir::new("mkdirparent");
        let err = mkdir(&d.join("gone"), "x").expect_err("no parent");
        assert_eq!(err.where_, "mkdir");
        assert!(err.msg.starts_with("No such file or directory"), "{}", err.msg);
        let locked = d.dir("locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = mkdir(&locked, "x").expect_err("denied");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(err.msg.starts_with("Permission denied"), "{}", err.msg);
        assert!(!locked.join("x").exists());
    }
}
