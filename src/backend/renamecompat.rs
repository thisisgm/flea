// Atomic no-clobber rename, a checked native retry, and cross-device copy fallback.
use crate::backend::copyfile::{copy_any, remove_any, Progress};
use crate::error::{from_io, FleaError};
use std::ffi::{c_char, CString};
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

// Both paths passed here are absolute, so renameat2 never consults AT_FDCWD.
const AT_FDCWD: i32 = -100;
const RENAME_NOREPLACE: u32 = 1;
const EXDEV: i32 = 18;
// The kind a half-succeeded rename answers; ui/js/Errors.js words it and ui/PaneWire.qml refreshes on it.
pub(crate) const KEPT: &str = "rename-kept";

extern "C" {
    fn renameat2(
        olddirfd: i32,
        oldpath: *const c_char,
        newdirfd: i32,
        newpath: *const c_char,
        flags: u32,
    ) -> i32;
}

// Unsupported flags permit a checked retry, which can overwrite a destination created after the check.
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let c_from = path_c(from)?;
    let c_to = path_c(to)?;
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
        return Ok(());
    }
    let error = io::Error::last_os_error();
    crate::backend::checkedrename::fallback(from, to, error)
}

pub(crate) fn rename_path(from: &Path, to: &Path) -> Result<(), FleaError> {
    finish_rename(from, to, rename_noreplace(from, to))
}

fn finish_rename(from: &Path, to: &Path, result: io::Result<()>) -> Result<(), FleaError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(EXDEV) => copy_then_remove(from, to),
        Err(error) => Err(from_io("rename", &to.to_string_lossy(), &error)),
    }
}

// The target is built through the exclusive copy primitives, so an existing destination is refused rather than replaced.
pub(crate) fn copy_then_remove(from: &Path, to: &Path) -> Result<(), FleaError> {
    let cancel = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut progress = Progress { cancel: &cancel, on_bytes: &mut sink, partial: None, tree: None };
    if let Err(error) = copy_any(from, to, &mut progress) {
        if progress.partial.as_deref() == Some(to) {
            if let Err(cleanup) = remove_any(to) {
                return Err(FleaError {
                    where_: "rename".to_string(),
                    path: to.to_string_lossy().to_string(),
                    msg: format!("{}; partial target could not be removed: {}", error.msg, cleanup.msg),
                });
            }
        }
        return Err(rename_error(error));
    }
    match remove_any(from) {
        Ok(()) => Ok(()),
        Err(error) => Err(after_failed_removal(from, to, error)),
    }
}

// Only a directory's removal can stop partway, so a source that still stats as any other kind is proof it survived whole.
fn after_failed_removal(from: &Path, to: &Path, error: FleaError) -> FleaError {
    match from.symlink_metadata() {
        Ok(meta) if !meta.is_dir() => undo_the_copy(to, error),
        _ => kept_error(from, error),
    }
}

// The source is not provably whole here, so the target may hold the only complete copy and stays under its own kind.
fn kept_error(from: &Path, error: FleaError) -> FleaError {
    FleaError {
        where_: KEPT.to_string(),
        path: from.to_string_lossy().to_string(),
        msg: error.msg,
    }
}

// The source still stats as a kind remove_any unlinks, so it is whole and the copy is a duplicate this operation takes back.
fn undo_the_copy(to: &Path, error: FleaError) -> FleaError {
    match remove_any(to) {
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

    const ENOENT: i32 = 2;

    #[test]
    fn only_cross_device_failure_copies_after_the_native_attempt() {
        let sandbox = TestDir::new("renamecopypolicy");
        let source = sandbox.file("source", "body");
        let target = sandbox.join("target");
        for errno in [1, 2, 5, 13, 17, 20, 22, 28, 30, 38, 95] {
            let error = finish_rename(&source, &target, Err(io::Error::from_raw_os_error(errno)))
                .expect_err("native failures must not start a copy");
            assert_eq!(error.where_, "rename");
            assert_eq!(std::fs::read_to_string(&source).unwrap(), "body");
            assert!(!target.exists());
        }
        finish_rename(&source, &target, Err(io::Error::from_raw_os_error(EXDEV))).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "body");
    }
    #[test]
    fn cross_device_copy_fallback_refuses_to_remove_an_existing_destination() {
        let d = TestDir::new("webdavrenameclobber");
        let from = d.file("source.txt", "source body");
        let to = d.file("target.txt", "target body");
        d.assert_contains(&from);
        d.assert_contains(&to);
        let error = finish_rename(&from, &to, Err(io::Error::from_raw_os_error(EXDEV)))
            .expect_err("the fallback must refuse an existing destination");
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
    // corner: runs as a plain user, where a directory without its write bit cannot remove its child.
    #[test]
    fn copied_directory_rename_keeps_the_complete_copy_when_source_removal_fails() {
        let d = TestDir::new("copyrenameremove");
        let source = d.dir("source");
        std::fs::write(source.join("inside.txt"), "body").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o555)).unwrap();
        let target = d.join("target");
        d.assert_contains(&source);
        d.assert_contains(&target);
        let error = copy_then_remove(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, KEPT, "the half-succeeded rename gets its own kind");
        assert_eq!(std::fs::read_to_string(source.join("inside.txt")).unwrap(), "body");
        assert_eq!(std::fs::read_to_string(target.join("inside.txt")).unwrap(), "body");
        assert!(
            !error.msg.contains(&target.to_string_lossy().to_string()),
            "the target stays out of the message ui/js/Errors.js pattern matches for a collision"
        );
    }

    // corner: runs as a plain user, where a directory with no write bit cannot lose the child it holds.
    #[test]
    fn a_kept_copy_can_leave_a_remnant_at_the_name_it_came_from() {
        let d = TestDir::new("copyrenameremnant");
        let hold = d.dir("hold");
        let source = hold.join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("alpha.txt"), "alpha").unwrap();
        std::fs::write(source.join("beta.txt"), "beta").unwrap();
        let target = d.join("target");
        // A parent with no write bit fails the unlink of the source itself, once its children have gone.
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o555)).unwrap();
        d.assert_contains(&source);
        d.assert_contains(&target);
        let error = copy_then_remove(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, KEPT, "the half-succeeded rename keeps its own kind");
        assert!(!source.join("alpha.txt").exists(), "the source lost a child, so it is not the whole copy");
        assert_eq!(std::fs::read_to_string(target.join("alpha.txt")).unwrap(), "alpha");
        assert_eq!(std::fs::read_to_string(target.join("beta.txt")).unwrap(), "beta");
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
    // A removal answering ENOENT after it took effect leaves the copy as the only whole name.
    #[test]
    fn a_source_that_no_longer_stats_keeps_the_copy_rather_than_taking_it_back() {
        let d = TestDir::new("copyrenamevanished");
        let to = d.file("target.txt", "body");
        let from = d.join("source.txt");
        let removal = from_io("move", &from.to_string_lossy(), &io::Error::from_raw_os_error(ENOENT));
        let error = after_failed_removal(&from, &to, removal);
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "body", "the only complete copy stays on disk");
        assert_eq!(error.where_, KEPT, "a source that proves nothing keeps the copy");
    }
}
