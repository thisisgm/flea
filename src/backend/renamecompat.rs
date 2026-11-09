// Linux's atomic no-clobber rename, plus the measured mounts that need a safe caller-owned copy fallback.
use crate::backend::copyfile::{copy_any, Progress};
use crate::backend::mountinfo::mount_type_in;
use crate::error::{from_io, FleaError};
use std::ffi::{c_char, CString};
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

// Both paths passed here are absolute, so renameat2 never consults AT_FDCWD.
const AT_FDCWD: i32 = -100;
const RENAME_NOREPLACE: u32 = 1;
pub(crate) const EINVAL: i32 = 22;
// GVFS answers a WebDAV rename with EIO instead of refusing it outright.
const EIO: i32 = 5;
// The error a copy-fallback rename answers when its folder would not confirm and the copy went back.
pub const RENAME_UNCONFIRMED: &str = "the drive did not confirm the folder, so the rename was undone";
const EXDEV: i32 = 18;
const EPERM: i32 = 1;
const ENOSYS: i32 = 38;
const EMLINK: i32 = 31;
const EOPNOTSUPP: i32 = 95;
const EEXIST: i32 = 17;
// The kinds a half-succeeded rename answers; ui/js/Errors.js words each and ui/PaneWire.qml refreshes on both.
pub(crate) const KEPT: &str = "rename-kept";
pub(crate) const STRANDED: &str = "rename-stranded";

// A person's item must stay whole when it will not go; a job's own stage leaves its landed copy as the result instead.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Item,
    Stage,
}

extern "C" {
    fn renameat2(
        olddirfd: i32,
        oldpath: *const c_char,
        newdirfd: i32,
        newpath: *const c_char,
        flags: u32,
    ) -> i32;
}

// Some FUSE mounts reject directory RENAME_NOREPLACE, so EINVAL or EOPNOTSUPP falls back here for every caller.
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let c_from = path_c(from)?;
    let c_to = path_c(to)?;
    let forced = take_forced_fallback();
    let error = if forced {
        io::Error::from_raw_os_error(EINVAL)
    } else {
        #[cfg(test)]
        super::renametiming::mark("syscall_begin");
        let rc = unsafe {
            renameat2(
                AT_FDCWD,
                c_from.as_ptr(),
                AT_FDCWD,
                c_to.as_ptr(),
                RENAME_NOREPLACE,
            )
        };
        if rc == 0 {
            #[cfg(test)]
            super::renametiming::mark("syscall_end");
            return Ok(());
        }
        let error = io::Error::last_os_error();
        #[cfg(test)]
        super::renametiming::mark("syscall_end");
        error
    };
    match error.raw_os_error() {
        Some(EINVAL) | Some(EOPNOTSUPP) => {
            // A copy-fallback mount keeps its EINVAL, so rclone and megafs stay on copy_then_remove.
            if needs_copy_fallback(from, &error) {
                return Err(error);
            }
            noreplace_fallback(from, to)
        }
        _ => Err(error),
    }
}

// The flagless fallback keeps no-clobber by a check, so a name created between the check and a plain rename can still be taken; a file links first and a taken name stays EEXIST.
fn noreplace_fallback(from: &Path, to: &Path) -> io::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Err(io::Error::from_raw_os_error(EEXIST));
    }
    let is_file = from.symlink_metadata().map(|m| m.is_file()).unwrap_or(false);
    if is_file {
        match hard_link(from, to) {
            Ok(()) => return std::fs::remove_file(from),
            Err(link) => match link.raw_os_error() {
                Some(EPERM) | Some(EOPNOTSUPP) | Some(ENOSYS) | Some(EMLINK) => {}
                _ => return Err(link),
            },
        }
    }
    std::fs::rename(from, to)
}

#[cfg(test)]
thread_local! {
    static FORCE_FALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_HARD_LINK: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    static FORCE_COPY_FALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_TWIN_BACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_TWIN_FORWARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_TAKE_BACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// A test makes needs_copy_fallback answer true, so a copy-fallback mount needs no FUSE mount.
#[cfg(test)]
pub fn test_force_copy_fallback(force: bool) {
    FORCE_COPY_FALLBACK.with(|flag| flag.set(force));
}

// The injected EINVAL hook: the next rename_noreplace answers as a kernel without the flag.
#[cfg(test)]
pub fn test_force_fallback(force: bool) {
    FORCE_FALLBACK.with(|flag| flag.set(force));
}

// The injected link failure: the next fallback link answers this errno instead of linking.
#[cfg(test)]
pub(crate) fn test_fail_hard_link(errno: Option<i32>) {
    FAIL_HARD_LINK.with(|fail| fail.set(errno));
}

// A test hook answers first when armed, so a linkless filesystem is reachable without one.
fn hard_link(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    if let Some(code) = FAIL_HARD_LINK.with(|fail| fail.replace(None)) {
        return Err(io::Error::from_raw_os_error(code));
    }
    std::fs::hard_link(from, to)
}

fn take_forced_fallback() -> bool {
    #[cfg(test)]
    {
        FORCE_FALLBACK.with(|flag| flag.replace(false))
    }
    #[cfg(not(test))]
    {
        false
    }
}

// Rename uses the atomic syscall everywhere except a measured fallback, which copies exclusively before removing the source.
pub(crate) fn rename_path(from: &Path, to: &Path) -> Result<(), FleaError> {
    if is_same_item_for_move(from, to) {
        return case_twin_move(from, to);
    }
    rename_inner(from, to)
}

// One rename without the case-twin check, so the twin's own temp steps never recurse.
fn rename_inner(from: &Path, to: &Path) -> Result<(), FleaError> {
    rename_source(from, to, &AtomicBool::new(false), Source::Item)
}

// A job's staged result lands here, so its copy fallback stops on the job's own cancel, and a stage that will not go still publishes (issue 370).
pub(crate) fn rename_cancellable(from: &Path, to: &Path, cancel: &AtomicBool) -> Result<(), FleaError> {
    rename_source(from, to, cancel, Source::Stage)
}

fn rename_source(
    from: &Path,
    to: &Path,
    cancel: &AtomicBool,
    source: Source,
) -> Result<(), FleaError> {
    match rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(EXDEV) || needs_copy_fallback(from, &error) => {
            copy_then_remove_with(from, to, cancel, source)
        }
        Err(error) => Err(from_io("rename", &to.to_string_lossy(), &error)),
    }
}

// Two paths naming one inode are one case twin, so a no-clobber rename takes the temp path.
pub(crate) fn is_same_item_for_move(from: &Path, to: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (from.symlink_metadata(), to.symlink_metadata()) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

// How many temp sibling names a case twin tries before giving up rather than looping forever.
const CASE_TRIES: usize = 100;

// A case twin moves via a temp sibling, so undo and redo share the path the rename took.
pub(crate) fn case_twin_move(from: &Path, to: &Path) -> Result<(), FleaError> {
    let parent = from.parent().unwrap_or(Path::new("/"));
    for n in 0..CASE_TRIES {
        let temp = parent.join(format!(".flea-case-{}-{}", std::process::id(), n));
        if temp.symlink_metadata().is_ok() {
            continue;
        }
        rename_inner(from, &temp)?;
        if to.symlink_metadata().is_ok() {
            // A hardlink twin is a real second name, so the temp goes back and the rename is refused.
            if let Err(back) = move_back(&temp, from) {
                return Err(kept_temp(&temp, from, back));
            }
            return Err(FleaError { where_: "rename".to_string(), path: from.to_string_lossy().to_string(), msg: "an item with that name is already here".to_string() });
        }
        if let Err(error) = move_forward(&temp, to) {
            if let Err(back) = move_back(&temp, from) {
                return Err(kept_temp(&temp, from, back));
            }
            return Err(error);
        }
        return Ok(());
    }
    Err(FleaError { where_: "rename".to_string(), path: from.to_string_lossy().to_string(), msg: "an item with that name is already here".to_string() })
}

// The move-back a twin strands on, injectable in tests so a failed restore needs no sick filesystem.
fn move_back(temp: &Path, from: &Path) -> Result<(), FleaError> {
    #[cfg(test)]
    if FAIL_TWIN_BACK.with(|flag| flag.replace(false)) {
        return Err(from_io("rename", &from.to_string_lossy(), &std::io::Error::from_raw_os_error(EIO)));
    }
    rename_inner(temp, from)
}

// The forward step a twin strands on, injectable in tests like the move-back beside it.
fn move_forward(temp: &Path, to: &Path) -> Result<(), FleaError> {
    #[cfg(test)]
    if FAIL_TWIN_FORWARD.with(|flag| flag.replace(false)) {
        return Err(from_io("rename", &to.to_string_lossy(), &std::io::Error::from_raw_os_error(EIO)));
    }
    rename_inner(temp, to)
}

// A test fails the next twin move-back, so the kept-temp error needs no unwritable parent.
#[cfg(test)]
pub fn test_fail_twin_back() {
    FAIL_TWIN_BACK.with(|flag| flag.set(true));
}

// A test fails the next twin forward step, so the stranded forward arm needs no sick filesystem.
#[cfg(test)]
fn test_fail_twin_forward() {
    FAIL_TWIN_FORWARD.with(|flag| flag.set(true));
}

// A stranded twin answers its own kind with the source path, so the rename request closes and msg names the temp leaf.
fn kept_temp(temp: &Path, from: &Path, back: FleaError) -> FleaError {
    FleaError { where_: STRANDED.to_string(), path: from.to_string_lossy().to_string(), msg: format!("the file was left as {} in this folder: {}", temp.file_name().unwrap_or_default().to_string_lossy(), back.msg) }
}

// WebDAV is decided from the path and errno alone, so a FUSE check never reads mountinfo for it.
pub(crate) fn needs_copy_fallback(from: &Path, error: &io::Error) -> bool {
    #[cfg(test)]
    if FORCE_COPY_FALLBACK.with(|flag| flag.get()) {
        return error.raw_os_error() == Some(EINVAL);
    }
    if needs_gvfs_webdav_fallback(from, error) {
        return true;
    }
    // The errno answers first, so an ordinary collision never reads and parses the whole mount table.
    if error.raw_os_error() != Some(EINVAL) {
        return false;
    }
    std::fs::read_to_string("/proc/self/mountinfo")
        .ok()
        .map(|body| needs_fuse_fallback_in(from, error, &body))
        .unwrap_or(false)
}

// Sample input, from: "/run/user/1000/gvfs/dav:host=slot,ssl=true/notes.txt"
fn needs_gvfs_webdav_fallback(from: &Path, error: &io::Error) -> bool {
    let text = from.to_string_lossy();
    error.raw_os_error() == Some(EIO) && text.starts_with("/run/user/") && text.contains("/gvfs/dav:")
}

pub(crate) fn needs_fuse_fallback_in(from: &Path, error: &io::Error, mountinfo: &str) -> bool {
    if error.raw_os_error() != Some(EINVAL) {
        return false;
    }
    match mount_type_in(from, mountinfo).as_deref() {
        Some("fuse.megafs") => true,
        Some("fuse.rclone") => from
            .symlink_metadata()
            .map(|meta| meta.file_type().is_dir())
            .unwrap_or(false),
        _ => false,
    }
}

// The tests drive the copy fallback uncancelled through this name.
#[cfg(test)]
pub(crate) fn copy_then_remove(from: &Path, to: &Path) -> Result<(), FleaError> {
    copy_then_remove_with(from, to, &AtomicBool::new(false), Source::Item)
}

// The target is built through the exclusive copy primitives, so an existing destination is refused, and a cancel makes copy_any remove what it created.
fn copy_then_remove_with(
    from: &Path,
    to: &Path,
    cancel: &AtomicBool,
    source: Source,
) -> Result<(), FleaError> {
    let mut sink = |_: u64, _: u64| {};
    // EXDEV and a GVFS WebDAV rename land here too, and a dav share is a durable destination.
    let mut durability = crate::backend::durable::Durability::begin(to.parent().unwrap_or(from));
    let mut progress = Progress {
        cancel,
        on_bytes: &mut sink,
        partial: None,
        tree: None,
        manifest: None,
        durability: Some(&mut durability),
        for_move: source == Source::Item,
    };
    if let Err(error) = copy_any(from, to, &mut progress) {
        if progress.partial.as_deref() == Some(to) {
            if let Err(cleanup) = take_back(to) {
                return Err(FleaError {
                    where_: "rename".to_string(),
                    path: to.to_string_lossy().to_string(),
                    msg: format!("{}; the partial copy could not be removed: {}", error.msg, cleanup.msg),
                });
            }
        }
        return Err(rename_error(error));
    }
    drop(progress);
    // The copy landed but its folder is unconfirmed; the landed copy goes back so the tree is as before.
    if durability.flush_dirs().is_err() {
        // corner: a take-back that fails leaves a whole or partial copy the rename wire cannot name, see AGENTS.md.
        let msg = match take_back(to) {
            Ok(()) => RENAME_UNCONFIRMED.to_string(),
            Err(cleanup) => format!("{}; the new copy could not be removed: {}", crate::backend::durable::DIR_UNCONFIRMED, cleanup.msg),
        };
        return Err(FleaError { where_: "rename".to_string(), path: to.to_string_lossy().to_string(), msg });
    }
    remove_source(from, to, source)
}

// A copy the destination does not show whole goes back, and the source stays.
fn remove_source(from: &Path, to: &Path, source: Source) -> Result<(), FleaError> {
    if let Err(e) = crate::backend::copyfile::landed::same_as_source(from, to) {
        return Err(undo_the_copy(to, e.error("rename", to)));
    }
    use crate::backend::copyfile::Unremoved;
    match crate::backend::copyfile::remove_moved(from) {
        Ok(()) => Ok(()),
        // A stage still whole is no person's item, so its copy stays the result and the job clears or names the stage.
        Err(Unremoved::Whole(error) | Unremoved::Part(error)) if source == Source::Stage => {
            Err(kept_error(from, error))
        }
        Err(unremoved) => Err(after_failed_removal(from, to, unremoved)),
    }
}

// A source still whole makes the copy a duplicate to take back; one gone in whole or in part leaves the copy the whole one.
fn after_failed_removal(from: &Path, to: &Path, unremoved: crate::backend::copyfile::Unremoved) -> FleaError {
    match unremoved {
        crate::backend::copyfile::Unremoved::Whole(error) => undo_the_copy(to, error),
        crate::backend::copyfile::Unremoved::Part(error) => kept_error(from, error),
    }
}

// The take-back of an unconfirmed or partial copy; a test fails it on purpose to reach that leftover state.
fn take_back(to: &Path) -> Result<(), FleaError> {
    #[cfg(test)]
    if FAIL_TAKE_BACK.with(|fail| fail.replace(false)) {
        return Err(FleaError { where_: "rename".to_string(), path: to.to_string_lossy().to_string(), msg: "test refused the take-back".to_string() });
    }
    remove_copy(to)
}

// The copy is this operation's own, so a folder its copied modes shut is opened to take it back.
fn remove_copy(to: &Path) -> Result<(), FleaError> {
    crate::backend::copyfile::deep::remove_deep(to, true).map_err(|e| from_io("rename", &to.to_string_lossy(), &e))
}

// The source is not provably whole here, so the target may hold the only complete copy and stays under its own kind.
fn kept_error(from: &Path, error: FleaError) -> FleaError {
    FleaError {
        where_: KEPT.to_string(),
        path: from.to_string_lossy().to_string(),
        msg: error.msg,
    }
}

// Nothing of the source went, so the copy is a duplicate this operation takes back.
fn undo_the_copy(to: &Path, error: FleaError) -> FleaError {
    match remove_copy(to) {
        Ok(()) => rename_error(error),
        Err(cleanup) => FleaError {
            where_: "rename".to_string(),
            path: to.to_string_lossy().to_string(),
            msg: format!("{}; the copy left behind could not be removed: {}", error.msg, cleanup.msg),
        },
    }
}

fn rename_error(mut error: FleaError) -> FleaError {
    error.where_ = "rename".to_string();
    error
}

fn path_c(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains an interior NUL"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    const EACCES: i32 = 13;

    #[test]
    fn copy_fallback_scope_matches_each_measured_fuse_mount() {
        let d = TestDir::new("fuserenamescope");
        let file = d.file("file", "body");
        let directory = d.dir("directory");
        let rclone = format!(
            "1 0 0:1 / {} rw - fuse.rclone remote: rw\n",
            d.path().display()
        );
        let megafs = format!(
            "1 0 0:2 / {} rw - fuse.megafs megafs rw\n",
            d.path().display()
        );
        let ext4 = format!("1 0 8:1 / {} rw - ext4 /dev/a rw\n", d.path().display());
        let invalid = io::Error::from_raw_os_error(EINVAL);
        let exists = io::Error::from_raw_os_error(EEXIST);
        assert!(needs_fuse_fallback_in(&directory, &invalid, &rclone));
        assert!(needs_fuse_fallback_in(&directory, &invalid, &megafs));
        assert!(needs_fuse_fallback_in(&file, &invalid, &megafs));
        assert!(!needs_fuse_fallback_in(&file, &invalid, &rclone));
        assert!(!needs_fuse_fallback_in(&directory, &invalid, &ext4));
        assert!(!needs_fuse_fallback_in(&directory, &exists, &rclone));
        assert_eq!(std::fs::read_to_string(file).unwrap(), "body");
    }
    #[test]
    fn only_a_gvfs_webdav_eio_uses_the_copy_fallback() {
        let eio = io::Error::from_raw_os_error(EIO);
        let denied = io::Error::from_raw_os_error(EACCES);
        assert!(needs_gvfs_webdav_fallback(Path::new("/run/user/1000/gvfs/dav:host=slot,ssl=true/file"), &eio));
        assert!(!needs_gvfs_webdav_fallback(Path::new("/run/user/1000/gvfs/sftp:host=slot/file"), &eio));
        assert!(!needs_gvfs_webdav_fallback(Path::new("/tmp/gvfs/dav:host=fake/file"), &eio));
        assert!(!needs_gvfs_webdav_fallback(Path::new("/run/user/1000/gvfs/dav:host=slot,ssl=true/file"), &denied));
    }
    #[test]
    fn webdav_copy_fallback_refuses_to_remove_an_existing_destination() {
        let d = TestDir::new("webdavrenameclobber");
        let from = d.file("source.txt", "source body");
        let to = d.file("target.txt", "target body");
        d.assert_contains(&from);
        d.assert_contains(&to);
        let error = copy_then_remove(&from, &to).expect_err("the fallback must refuse an existing destination");
        assert_eq!(
            error.msg, "already exists",
            "the exclusive create's EEXIST is what tells this refusal from any other copy failure"
        );
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "target body");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "source body");
    }
    #[test]
    fn copied_directory_rename_refuses_an_existing_empty_directory() {
        let d = TestDir::new("copyrenameclobber");
        let source = d.dir("source");
        std::fs::write(source.join("source.txt"), "source body").unwrap();
        let target = d.dir("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        let error = copy_then_remove(&source, &target).expect_err("must refuse");
        assert_eq!(error.where_, "rename");
        assert_eq!(
            error.msg, "already exists",
            "the directory create's EEXIST is what tells this refusal from any other copy failure"
        );
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
        d.assert_contains(&source);
        d.assert_contains(&target);
        copy_then_remove(&source, &target).expect("rename by exclusive copy");
        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(target.join("nested/inside.txt")).unwrap(), "body");

        d.assert_contains(&source);
        d.assert_contains(&target);
        copy_then_remove(&target, &source).expect("undo by exclusive copy");
        assert!(!target.exists());
        assert_eq!(std::fs::read_to_string(source.join("nested/inside.txt")).unwrap(), "body");
    }
    #[test]
    fn copied_directory_rename_removes_its_partial_target_after_copy_failure() {
        let d = TestDir::new("copyrenamepartial");
        let source = d.dir("source");
        // A socket used to be the failure here, until backend/copynode.rs learned to recreate one;
        // a file with no permission bits answers EACCES to open(2) for every uid but root instead.
        let child = source.join("unreadable");
        std::fs::write(&child, "body").unwrap();
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o000)).unwrap();
        let target = d.join("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        let error = copy_then_remove(&source, &target).expect_err("an unreadable child cannot be copied");
        assert!(child.exists(), "the source remains after a failed copy");
        assert!(!target.exists(), "the failed rename leaves no unjournaled partial target");
        assert_eq!(
            error.path,
            child.to_string_lossy(),
            "the copy failed on a child, so the target directory the cleanup removed had been created"
        );
    }
    #[test]
    fn a_partial_copy_that_will_not_go_is_named_in_the_error() {
        let _in_place = crate::backend::copystage_tests::in_place();
        let d = TestDir::new("copyrenamepartialkept");
        let source = d.dir("source");
        let child = source.join("unreadable");
        std::fs::write(&child, "body").unwrap();
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o000)).unwrap();
        let target = d.join("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        FAIL_TAKE_BACK.with(|fail| fail.set(true));
        let error = { let copied = copy_then_remove(&source, &target); FAIL_TAKE_BACK.with(|fail| fail.set(false)); copied }.expect_err("an unreadable child cannot be copied");
        assert!(error.msg.ends_with("; the partial copy could not be removed: test refused the take-back"), "{}", error.msg);
        assert_eq!(error.path, target.to_string_lossy(), "the leftover is named by its own path");
    }
    // corner: runs as a plain user, where a directory without its write bit cannot remove its child.
    #[test]
    fn a_directory_rename_refuses_a_source_that_cannot_give_up_its_children() {
        let d = TestDir::new("copyrenameremove");
        let source = d.dir("source");
        std::fs::write(source.join("inside.txt"), "body").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o555)).unwrap();
        let target = d.join("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        crate::backend::copyfile::test_take_buffers();
        // A person's rename enters at rename_path, so the item's preflight is pinned through the same dispatch the job's stage skips.
        test_force_fallback(true);
        test_force_copy_fallback(true);
        let renamed = rename_path(&source, &target);
        test_force_copy_fallback(false);
        test_force_fallback(false);
        let error = renamed.expect_err("the source cannot be emptied");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!((error.where_.as_str(), error.msg.as_str()), ("rename", "permission denied"), "refused before anything copied");
        assert_eq!(std::fs::read_to_string(source.join("inside.txt")).unwrap(), "body");
        assert!(!target.exists() && crate::backend::copyfile::test_take_buffers().is_empty(), "nothing was read, so no copy was made");
    }

    // corner: runs as a plain user, where a directory with no write bit cannot lose the child it holds.
    #[test]
    fn a_directory_rename_out_of_a_read_only_folder_keeps_every_child() {
        let d = TestDir::new("copyrenameremnant");
        let hold = d.dir("hold");
        let source = hold.join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("alpha.txt"), "alpha").unwrap();
        std::fs::write(source.join("beta.txt"), "beta").unwrap();
        let target = d.join("target");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o555)).unwrap();
        d.assert_contains(&source);
        d.assert_contains(&target);
        crate::backend::copyfile::test_take_buffers();
        let error = copy_then_remove(&source, &target).expect_err("the folder will not let the source go");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!((error.where_.as_str(), error.msg.as_str()), ("rename", "permission denied"));
        assert_eq!(std::fs::read_to_string(source.join("alpha.txt")).unwrap(), "alpha", "the source lost nothing");
        assert_eq!(std::fs::read_to_string(source.join("beta.txt")).unwrap(), "beta");
        assert!(!target.exists() && crate::backend::copyfile::test_take_buffers().is_empty(), "refused before a byte was read");
    }

    // corner: runs as a plain user, where a directory without its write bit cannot remove its child.
    #[test]
    fn a_file_rename_takes_its_copy_back_when_the_source_will_not_go() {
        let d = TestDir::new("copyrenamefileremove");
        let hold = d.dir("hold");
        let source = hold.join("source.txt");
        std::fs::write(&source, "body").unwrap();
        let target = d.join("target.txt");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o555)).unwrap();
        d.assert_contains(&source);
        d.assert_contains(&target);
        let error = copy_then_remove(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, "rename", "remove_file is atomic, so the source is provably whole");
        assert_eq!(
            error.msg, "permission denied",
            "the removal's EACCES is what tells this arm from a copy failure, which answers rename too"
        );
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "body");
        assert!(!target.exists(), "the copy is taken back rather than left as an unjournalled duplicate");
    }

    // corner: runs as a plain user, where a directory without its write bit cannot lose the child it holds.
    #[test]
    fn a_symlink_rename_takes_its_copy_back_when_the_source_will_not_go() {
        let d = TestDir::new("copyrenamesymlinkremove");
        let payload = d.file("payload.txt", "body");
        let hold = d.dir("hold");
        let source = hold.join("source");
        std::os::unix::fs::symlink(&payload, &source).unwrap();
        let target = d.join("target");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o555)).unwrap();
        d.assert_contains(&source);
        d.assert_contains(&target);
        let error = copy_then_remove(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, "rename", "one unlink removes a symlink too, so the source is provably whole");
        assert_eq!(
            error.msg, "permission denied",
            "the removal's EACCES is what tells this arm from a copy failure, which answers rename too"
        );
        assert_eq!(std::fs::read_link(&source).unwrap(), payload);
        assert!(target.symlink_metadata().is_err(), "the copy is taken back rather than left as an unjournalled duplicate");
    }
    #[test]
    fn a_dir_flush_failure_takes_the_landed_copy_back() {
        let d = TestDir::new("copyrenameflush");
        let source = d.dir("source");
        std::fs::write(source.join("inside.txt"), "body").unwrap();
        let target = d.join("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        crate::backend::durable::test_reset();
        crate::backend::durable::test_mark_durable(d.path());
        crate::backend::durable::test_set_fail_dirs(true);
        let error = copy_then_remove(&source, &target).expect_err("an unconfirmed folder takes its copy back");
        crate::backend::durable::test_set_fail_dirs(false);
        crate::backend::durable::test_reset();
        assert_eq!(error.where_, "rename");
        assert_eq!(error.msg, RENAME_UNCONFIRMED, "nothing stays copied, so the error never says copied");
        assert!(source.join("inside.txt").is_file(), "the whole source stays");
        assert!(!target.exists(), "the landed copy goes back so the tree is as before");
    }
    #[test]
    fn a_failed_take_back_keeps_the_source_whole_and_answers_rename() {
        let d = TestDir::new("copyrenamekept");
        let source = d.dir("source");
        std::fs::write(source.join("inside.txt"), "body").unwrap();
        let target = d.join("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        crate::backend::durable::test_reset();
        crate::backend::durable::test_mark_durable(d.path());
        crate::backend::durable::test_set_fail_dirs(true);
        FAIL_TAKE_BACK.with(|fail| fail.set(true));
        let error = copy_then_remove(&source, &target).expect_err("an unconfirmed folder whose copy will not go back");
        crate::backend::durable::test_set_fail_dirs(false);
        crate::backend::durable::test_reset();
        assert_eq!(error.where_, "rename", "the source is whole, so this is never rename-kept, whose sentence trusts the copy");
        assert!(error.msg.starts_with(crate::backend::durable::DIR_UNCONFIRMED) && error.msg.ends_with("; the new copy could not be removed: test refused the take-back"), "{}", error.msg);
        assert!(source.join("inside.txt").is_file(), "the source was never touched");
    }
    // A source already gone when its removal runs leaves the copy as the only whole name.
    #[test]
    fn a_source_that_no_longer_stats_keeps_the_copy_rather_than_taking_it_back() {
        let d = TestDir::new("copyrenamevanished");
        let to = d.file("target.txt", "body");
        let from = d.join("source.txt");
        let removal = crate::backend::copyfile::remove_moved(&from).expect_err("a source that is not there cannot be removed");
        let error = after_failed_removal(&from, &to, removal);
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "body", "the only complete copy stays on disk");
        assert_eq!(error.where_, KEPT, "a source that proves nothing keeps the copy");
    }
    // corner: runs as a plain user; a folder shut before the copy is refused up front, so this one shuts after its copy landed.
    #[test]
    fn a_source_removed_partway_keeps_the_whole_copy_under_its_own_kind() {
        let d = TestDir::new("copyrenamepart");
        let (source, target) = (d.dir("source"), d.dir("target"));
        for root in [&source, &target] {
            std::fs::write(root.join("a.txt"), "a").unwrap();
            std::fs::create_dir(root.join("shut")).unwrap();
            std::fs::write(root.join("shut/b.txt"), "b").unwrap();
        }
        std::fs::set_permissions(source.join("shut"), std::fs::Permissions::from_mode(0o555)).unwrap();
        let error = remove_source(&source, &target, Source::Item)
            .expect_err("the shut folder keeps b.txt");
        std::fs::set_permissions(source.join("shut"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, KEPT, "a source that lost part of itself keeps the copy under its own kind: {}", error.msg);
        assert!(!error.msg.contains(&target.to_string_lossy().to_string()), "the target stays out of the message ui/js/Errors.js pattern matches for a collision");
        assert!(!source.join("a.txt").exists(), "the removal had begun");
        assert_eq!(std::fs::read_to_string(target.join("a.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(target.join("shut/b.txt")).unwrap(), "b");
    }
    // A copy the destination shows short goes back before the source is touched.
    #[test]
    fn a_copy_shown_short_goes_back_and_the_source_stays() {
        let d = TestDir::new("copyrenameshort");
        let from = d.file("source.txt", "body");
        let to = d.file("target.txt", "bo");
        let error = remove_source(&from, &to, Source::Item)
            .expect_err("a short copy never lets its source go");
        assert_eq!((error.where_.as_str(), error.msg.as_str()), ("rename", crate::backend::copyfile::landed::FILE_UNCONFIRMED));
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "body", "the source stays");
        assert!(!to.exists(), "the short copy went back");
    }
    #[test]
    fn fallback_without_hard_links_moves_by_plain_rename() {
        // EPERM, EOPNOTSUPP, ENOSYS and EMLINK all mean the filesystem holds no hard links.
        for code in [EPERM, EOPNOTSUPP, ENOSYS, EMLINK] {
            let d = TestDir::new(&format!("noreplacenolink-{code}"));
            let from = d.file("source.txt", "source body");
            let to = d.join("target.txt");
            test_force_fallback(true);
            test_fail_hard_link(Some(code));
            let outcome = rename_noreplace(&from, &to);
            test_force_fallback(false);
            outcome.expect("no hard links still renames through the plain fallback");
            assert!(!from.exists(), "the source goes through the fallback too");
            assert_eq!(std::fs::read_to_string(&to).unwrap(), "source body");
        }
    }
    #[test]
    fn fallback_link_collision_stays_eexist() {
        // A link that meets a raced destination keeps EEXIST rather than falling back to a replacing rename.
        let d = TestDir::new("noreplacelinkrace");
        let from = d.file("source.txt", "source body");
        let to = d.join("target.txt");
        test_force_fallback(true);
        test_fail_hard_link(Some(EEXIST));
        let error = rename_noreplace(&from, &to).expect_err("a raced destination stays EEXIST");
        test_force_fallback(false);
        assert_eq!(error.raw_os_error(), Some(EEXIST));
        assert!(from.exists(), "the source stays when the link meets a taken name");
        assert!(to.symlink_metadata().is_err(), "nothing is created under the taken name");
    }
    // Sample input: forced EINVAL on a copy-fallback mount answers EINVAL, so rename_path copies.
    #[test]
    fn a_copy_fallback_mount_keeps_its_einval_for_the_copy_path() {
        let d = TestDir::new("noreplacecopyfallback");
        let from = d.dir("source");
        std::fs::write(from.join("inside.txt"), "body").unwrap();
        let to = d.join("target");
        test_force_fallback(true);
        test_force_copy_fallback(true);
        let error = rename_noreplace(&from, &to).expect_err("a copy-fallback mount keeps EINVAL");
        test_force_copy_fallback(false);
        test_force_fallback(false);
        assert_eq!(error.raw_os_error(), Some(EINVAL), "rclone and megafs stay on copy_then_remove");
        assert!(from.join("inside.txt").is_file(), "the source stays until the copy path runs");
        assert!(to.symlink_metadata().is_err(), "the fallback creates nothing on this path");
    }
    // Sample input: a.txt hardlinked to A.txt, so rename_path meets one inode under two names.
    #[test]
    fn a_same_item_rename_refuses_without_losing_a_name() {
        let d = TestDir::new("casetwinrefuse");
        let from = d.file("a.txt", "body");
        let twin = d.join("A.txt");
        std::fs::hard_link(&from, &twin).unwrap();
        let error = rename_path(&from, &twin).expect_err("a second name is a collision, not a move");
        assert_eq!(error.msg, "an item with that name is already here");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "body", "the source is moved back, not lost");
        assert_eq!(std::fs::read_to_string(&twin).unwrap(), "body", "the twin name is never deleted");
    }
    // Sample input: the same hardlink pair with the move-back forced to fail strands temp.
    #[test]
    fn a_failed_twin_move_back_names_its_temp() {
        let d = TestDir::new("casetwinstranded");
        let from = d.file("a.txt", "body");
        let twin = d.join("A.txt");
        std::fs::hard_link(&from, &twin).unwrap();
        test_fail_twin_back();
        let error = rename_path(&from, &twin).expect_err("a failed move-back strands the file");
        assert_eq!(error.where_, STRANDED, "a stranded twin answers its own kind");
        assert_eq!(error.path, from.to_string_lossy(), "the stranded error names the source");
        assert!(error.msg.starts_with("the file was left as .flea-case-"), "msg names temp leaf: {}", error.msg);
        assert!(error.msg.ends_with("the drive could not read or write the data"), "msg carries move-back cause: {}", error.msg);
        let leaf = error.msg.split(" as ").nth(1).unwrap().split(" in ").next().unwrap();
        assert!(d.path().join(leaf).is_file(), "the temp holds the file");
    }
    // Sample input: the source reached a second time as alias/a.txt through a directory symlink, so no twin survives the move to temp.
    #[test]
    fn a_failed_twin_forward_step_names_its_temp() {
        let d = TestDir::new("casetwinforward");
        let from = d.file("a.txt", "body");
        let alias = d.path().join("alias");
        std::os::unix::fs::symlink(d.path(), &alias).unwrap();
        let to = alias.join("a.txt");
        test_fail_twin_forward();
        test_fail_twin_back();
        let error = rename_path(&from, &to).expect_err("a failed forward step strands the file");
        assert_eq!(error.where_, STRANDED, "a stranded twin answers its own kind");
        assert_eq!(error.path, from.to_string_lossy(), "the stranded error names the source");
        assert!(error.msg.starts_with("the file was left as .flea-case-"), "msg names temp leaf: {}", error.msg);
        assert!(error.msg.ends_with("the drive could not read or write the data"), "msg carries move-back cause: {}", error.msg);
        let leaf = error.msg.split(" as ").nth(1).unwrap().split(" in ").next().unwrap();
        assert!(d.path().join(leaf).is_file(), "the temp holds the file");
        assert!(to.symlink_metadata().is_err(), "the alias names nothing once the source moved, so the forward arm ran");
    }
    // The FUSE arm reads the real mountinfo, so a unit test drives only the WebDAV arm; live mount batteries drive the other.
    #[test]
    fn the_composed_predicate_answers_for_the_webdav_case() {
        let d = TestDir::new("composedfallback");
        assert!(needs_copy_fallback(
            Path::new("/run/user/1000/gvfs/dav:host=x,ssl=true/f"),
            &io::Error::from_raw_os_error(EIO)
        ));
        assert!(!needs_copy_fallback(d.path(), &io::Error::from_raw_os_error(EIO)));
        assert!(!needs_copy_fallback(d.path(), &io::Error::from_raw_os_error(EEXIST)));
    }
}
