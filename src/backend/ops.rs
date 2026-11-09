// Rename, duplicate and mkdir: the three operations that answer once, with no started or progress split.
use crate::backend::copyfile::{copy_any, Progress};
use crate::backend::renamecompat;
use crate::backend::undo::{self, ItemIdentity, Step};
use crate::error::FleaError;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

// How many " copy N" names duplicate will try before giving up rather than looping forever.
const NAME_TRIES: usize = 999;

// A name from the client is a trust boundary: anything with a separator would move the file out of its own directory.
pub use super::opsname::{name_refusal, name_refusal_sentence};
pub fn valid_name(name: &str) -> bool { name_refusal(name).is_none() }

// Sample output: "permission denied; the record of this copy could not be saved: no space left".
pub(crate) fn manifest_failed(msg: &str, detail: &str) -> String {
    format!("{msg}; the record of this copy could not be saved: {detail}")
}

// Sample output: "cancelled; undo cannot remove the partial copy of a.txt: the journal is full".
fn unjournaled_partial(msg: &str, partial: &Path, detail: &str) -> String {
    format!("{msg}; undo cannot remove the partial copy of {}: {detail}", partial.file_name().unwrap_or(partial.as_os_str()).to_string_lossy())
}

fn named(where_: &str, path: &Path, msg: &str) -> FleaError {
    FleaError {
        where_: where_.to_string(),
        path: path.to_string_lossy().to_string(),
        msg: msg.to_string(),
    }
}

// Answers the new path and the step that puts the old name back; a failure answers no step, a kept copy included, because none of undo.rs's four shapes can say which of the two names is whole.
pub fn rename(path: &Path, to_name: &str) -> Result<(PathBuf, Vec<Step>), FleaError> {
    if let Some(refusal) = name_refusal(to_name) {
        return Err(named("rename", path, refusal));
    }
    let parent = path.parent().unwrap_or(Path::new("/"));
    // The destination's own rules refuse before the syscall, so vfat answers the character, not EINVAL.
    if let Some(refusal) = crate::backend::fsname::refuse_in(parent, to_name) {
        return Err(named("rename", path, &refusal));
    }
    let to = parent.join(to_name);
    if to == path {
        // Renaming a file to its own name is not a failure and is not work, so it records nothing.
        return Ok((to, Vec::new()));
    }
    let before = ItemIdentity::inspect(path)?;
    // A case-only rename on a case-insensitive filesystem stats as the source itself, so the no-clobber rename moves via a temp sibling.
    if let Ok(same) = ItemIdentity::inspect(&to) {
        if same.same_item(&before) {
            return case_only_rename(path, &to, before);
        }
    }
    renamecompat::rename_path(path, &to)?;
    {
        // A same-filesystem rename is atomic, so its folder confirmation stays best effort.
        let mut confirm = crate::backend::durable::Durability::begin(&to);
        if confirm.durable {
            if let Some(parent) = to.parent() {
                confirm.touch(parent);
            }
            if let Err(error) = confirm.flush_dirs() {
                eprintln!("flea: rename {} landed but the drive did not confirm the folder: {}", to.display(), error);
            }
        }
    }
    Ok((to.clone(), vec![undo::moved(path, &to, before)?]))
}

// A rename whose destination is the source itself under another spelling runs the shared twin path.
fn case_only_rename(path: &Path, to: &Path, before: ItemIdentity) -> Result<(PathBuf, Vec<Step>), FleaError> {
    renamecompat::case_twin_move(path, to)?;
    Ok((to.to_path_buf(), vec![undo::moved(path, to, before)?]))
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
    let source = match ItemIdentity::inspect(path) {
        Ok(identity) => identity,
        Err(error) => return (Err(error), Vec::new()),
    };
    let dst = match free_copy_path(path, "copy") {
        Some(d) => d,
        None => return (Err(named("duplicate", path, "every copy name for this file is already taken")), Vec::new()),
    };
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut durability = crate::backend::durable::Durability::begin(dst.parent().unwrap_or(path));
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None,
        manifest: crate::backend::copymanifest::writer_for(path, &dst), durability: Some(&mut durability), for_move: false };
    match copy_any(path, &dst, &mut p) {
        Ok(()) => {
            drop(p);
            // The bytes landed; only the folder confirmation can still fail, so the step stays journalled either way.
            let unconfirmed = durability.flush_dirs().is_err();
            match undo::copied(path, &dst, source) {
                Ok(step) if !unconfirmed => (Ok(dst), vec![step]),
                Ok(step) => (Err(named("duplicate", &dst, crate::backend::durable::DIR_UNCONFIRMED)), vec![step]),
                Err(error) => (Err(error), Vec::new()),
            }
        }
        Err(mut error) => {
            let mut steps = Vec::new();
            if let Some(partial) = p.partial.take() {
                // A success journals the plain step above, so its undo is byte-for-byte today's; only a partial carries what the copy managed to create.
                let (manifest, loud) = crate::backend::copymanifest::finish_loud(p.manifest.take());
                if let Some(e) = loud {
                    error.msg = manifest_failed(&error.msg, &e);
                }
                match undo::copied_partial(path, &partial, source, manifest) {
                    Ok(step) => steps.push(step),
                    Err(record) => error.msg = unjournaled_partial(&error.msg, &partial, &record.msg),
                }
            }
            (Err(error), steps)
        }
    }
}

// True when this user can create entries in path, by access(2) W_OK.
pub(crate) fn dir_writable(path: &Path) -> bool {
    const W_OK: std::ffi::c_int = 2;
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str());
    let Ok(c) = std::ffi::CString::new(bytes) else { return false };
    extern "C" {
        fn access(path: *const std::ffi::c_char, mode: std::ffi::c_int) -> std::ffi::c_int;
    }
    unsafe { access(c.as_ptr(), W_OK) == 0 }
}

pub use super::newfolder::{mkdir, mkdir_with_base};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn illegal_names_are_refused_before_any_syscall() {
        // Sample pair: "a:b" on vfat is EINVAL, while ntfs3 without windows_names accepts it.
        use crate::backend::fsname::refuse_name;
        assert_eq!(refuse_name("a:b", "vfat", false).unwrap(), "':' is not allowed in a name on this vfat drive");
        assert_eq!(refuse_name("q?", "exfat", false).unwrap(), "'?' is not allowed in a name on this exfat drive");
        assert_eq!(refuse_name("trail.", "vfat", false).unwrap(), "a name cannot end in a dot on this vfat drive");
        assert_eq!(refuse_name("trail ", "vfat", false).unwrap(), "a name cannot end in a space on this vfat drive");
        assert_eq!(refuse_name("CON", "vfat", false).unwrap(), "'CON' is reserved on this vfat drive");
        assert_eq!(refuse_name("con.txt", "vfat", false).unwrap(), "'CON' is reserved on this vfat drive");
        assert!(refuse_name("ordinary.txt", "vfat", false).is_none());
        assert!(refuse_name("a:b", "ext4", false).is_none());
        assert!(refuse_name("a:b", "ntfs3", false).is_none(), "ntfs3 without windows_names takes a colon");
        assert!(refuse_name("a:b", "ntfs3", true).is_some(), "ntfs3 with windows_names refuses it");
    }

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
    fn a_loud_manifest_and_an_unjournaled_partial_each_extend_the_error_as_one_template() {
        assert_eq!(manifest_failed("permission denied", "no space"), "permission denied; the record of this copy could not be saved: no space");
        assert_eq!(unjournaled_partial("cancelled", Path::new("/d/a.txt"), "the journal is full"),
            "cancelled; undo cannot remove the partial copy of a.txt: the journal is full");
    }

    #[test]
    fn rename_moves_the_file_and_records_the_step_that_puts_it_back() {
        let d = TestDir::new("rename");
        let from = d.file("before.txt", "body");
        let (to, steps) = rename(&from, "after.txt").expect("rename");
        assert_eq!(to, d.join("after.txt"));
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "body");
        assert!(matches!(&steps[..], [Step::Moved { from: original, to: destination, after, .. }]
            if original == &from && destination == &to && after == &ItemIdentity::inspect(&to).unwrap()));
    }

    #[test]
    fn a_hardlink_twin_is_a_second_name_and_not_a_case_twin() {
        // On a case-sensitive filesystem a `to` that is a hard link to the source is a real second name.
        let d = TestDir::new("casetwin");
        let from = d.file("a.txt", "body");
        let twin = d.join("A.txt");
        std::fs::hard_link(&from, &twin).unwrap();
        let err = rename(&from, "A.txt").expect_err("a second name is a collision, not a case twin");
        assert_eq!(err.where_, "rename");
        assert_eq!(err.msg, "an item with that name is already here");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "body", "the source is moved back, not lost");
        assert_eq!(std::fs::read_to_string(&twin).unwrap(), "body", "the twin name is never deleted");
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
        assert_eq!(steps, vec![undo::copied(&original, &dst, ItemIdentity::inspect(&original).unwrap()).unwrap()]);
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

    // A file with no permission bits answers EACCES to open(2) for every uid but root, and no flea
    // suite runs as root, so it forces the failure a real permission error would.
    #[test]
    fn duplicating_a_tree_that_fails_short_of_the_end_still_records_the_partial_copy() {
        // In place, as a FUSE destination writes a tree; a staged folder goes whole instead (copyfolder_tests).
        let _seams = crate::backend::copystage_tests::in_place();
        let d = TestDir::new("duppartial");
        let src = d.dir("tree");
        std::fs::write(src.join("inside.txt"), "body").unwrap();
        let shut = src.join("shut.txt");
        std::fs::write(&shut, "body").unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
        let (outcome, steps) = duplicate(&src);
        assert!(outcome.is_err(), "the unreadable file cannot be opened, so the tree copy fails");
        let partial = d.join("tree copy");
        assert!(partial.is_dir(), "the failure left what it had copied");
        assert!(matches!(&steps[..], [Step::Copied { from, to, manifest: Some(_), .. }] if from == &src && to == &partial),
            "and the journal gets the partial with what it created: {:?}", steps);
    }

    #[test]
    fn mkdir_makes_the_folder_and_records_the_step_that_removes_it() {
        let d = TestDir::new("mkdir");
        let (made, steps) = mkdir(d.path(), "photos").expect("mkdir");
        assert_eq!(made, d.join("photos"));
        assert!(made.is_dir());
        assert_eq!(steps, vec![Step::MadeDir { path: made.clone(), identity: ItemIdentity::inspect(&made).unwrap() }]);
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
            let clause = match bad { "." | ".." => "a name cannot be a single dot or a double dot", "nul\0byte" => "a name cannot contain a null character", _ => "a name cannot contain /" };
            assert_eq!(err.msg, clause, "{:?}", bad);
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
    fn mkdir_under_a_vanished_or_unwritable_parent_names_the_cause() {
        let d = TestDir::new("mkdirparent");
        let err = mkdir(&d.join("gone"), "x").expect_err("no parent");
        assert_eq!(err.where_, "mkdir");
        assert_eq!(err.msg, "item not found");
        let locked = d.dir("locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = mkdir(&locked, "x").expect_err("denied");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(err.msg, "permission denied");
        assert!(!locked.join("x").exists());
    }

    #[test]
    fn a_directory_without_write_permission_is_not_writable() {
        let d = TestDir::new("dirwrite");
        let locked = d.dir("locked");
        assert!(super::dir_writable(&locked));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let refused = !super::dir_writable(&locked);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(refused, "a drop onto a folder that cannot be written is refused");
    }
}
