// Linux's atomic no-clobber rename, plus the two measured mounts that need a safe caller-owned copy fallback.
use crate::backend::copyfile::{copy_any, remove_any, Progress};
use crate::backend::mountinfo::mount_type_in;
use crate::error::{from_io, FleaError};
use std::ffi::{c_char, CString};
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

// Both paths passed here are absolute, so renameat2 never consults AT_FDCWD.
const AT_FDCWD: i32 = -100;
const RENAME_NOREPLACE: u32 = 1;
const EINVAL: i32 = 22;
// GVFS answers a WebDAV rename with EIO instead of refusing it outright.
const EIO: i32 = 5;
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

// rclone rejects directory RENAME_NOREPLACE; callers that can safely copy and remove handle that case themselves.
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
    Err(io::Error::last_os_error())
}

// Rename uses the atomic syscall everywhere except a measured fallback, which copies exclusively before removing the source.
pub(crate) fn rename_path(from: &Path, to: &Path) -> Result<(), FleaError> {
    match rename_noreplace(from, to) {
        Ok(()) => Ok(()),
        Err(error) if needs_copy_fallback(from, &error) => copy_then_remove(from, to),
        Err(error) => Err(from_io("rename", &to.to_string_lossy(), &error)),
    }
}

// WebDAV is decided from the path and errno alone, so an rclone check never reads mountinfo for it.
fn needs_copy_fallback(from: &Path, error: &io::Error) -> bool {
    if needs_gvfs_webdav_fallback(from, error) {
        return true;
    }
    // The errno answers first, so an ordinary collision never reads and parses the whole mount table.
    if error.raw_os_error() != Some(EINVAL) {
        return false;
    }
    std::fs::read_to_string("/proc/self/mountinfo")
        .ok()
        .map(|body| needs_rclone_fallback_in(from, error, &body))
        .unwrap_or(false)
}

// Sample input, from: "/run/user/1000/gvfs/dav:host=slot,ssl=true/notes.txt"
fn needs_gvfs_webdav_fallback(from: &Path, error: &io::Error) -> bool {
    let text = from.to_string_lossy();
    error.raw_os_error() == Some(EIO) && text.starts_with("/run/user/") && text.contains("/gvfs/dav:")
}

fn needs_rclone_fallback_in(from: &Path, error: &io::Error, mountinfo: &str) -> bool {
    error.raw_os_error() == Some(EINVAL)
        && from
            .symlink_metadata()
            .map(|meta| meta.file_type().is_dir())
            .unwrap_or(false)
        && mount_type_in(from, mountinfo).as_deref() == Some("fuse.rclone")
}

// The target is built through the exclusive copy primitives, so an existing destination is refused rather than replaced.
pub(crate) fn copy_then_remove(from: &Path, to: &Path) -> Result<(), FleaError> {
    let cancel = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut progress = Progress { cancel: &cancel, on_bytes: &mut sink, partial: None };
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

// Taking the copy back needs proof the source survived whole, and only a regular file that still stats is that proof.
fn after_failed_removal(from: &Path, to: &Path, error: FleaError) -> FleaError {
    match from.symlink_metadata() {
        Ok(meta) if meta.file_type().is_file() => undo_the_copy(to, error),
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

// The source still stats as a regular file, so it is whole and the copy is a duplicate this operation takes back.
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

    const EEXIST: i32 = 17;
    const EACCES: i32 = 13;
    const ENOENT: i32 = 2;

    #[test]
    fn copy_fallback_scope_is_only_an_einval_directory_on_rclone() {
        let d = TestDir::new("rclonerenamescope");
        let file = d.file("file", "body");
        let directory = d.dir("directory");
        let rclone = format!(
            "1 0 0:1 / {} rw - fuse.rclone remote: rw\n",
            d.path().display()
        );
        let ext4 = format!("1 0 8:1 / {} rw - ext4 /dev/a rw\n", d.path().display());
        let invalid = io::Error::from_raw_os_error(EINVAL);
        let exists = io::Error::from_raw_os_error(EEXIST);
        assert!(needs_rclone_fallback_in(&directory, &invalid, &rclone));
        assert!(!needs_rclone_fallback_in(&file, &invalid, &rclone));
        assert!(!needs_rclone_fallback_in(&directory, &invalid, &ext4));
        assert!(!needs_rclone_fallback_in(&directory, &exists, &rclone));
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
        copy_then_remove(&from, &to).expect_err("the fallback must refuse an existing destination");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "target body");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "source body");
    }
    #[test]
    fn copied_directory_rename_refuses_an_existing_empty_directory() {
        let d = TestDir::new("copyrenameclobber");
        let source = d.dir("source");
        std::fs::write(source.join("source.txt"), "source body").unwrap();
        let target = d.dir("target");
        let error = copy_then_remove(&source, &target).expect_err("must refuse");
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
        copy_then_remove(&source, &target).expect("rename by exclusive copy");
        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(target.join("nested/inside.txt")).unwrap(), "body");

        copy_then_remove(&target, &source).expect("undo by exclusive copy");
        assert!(!target.exists());
        assert_eq!(std::fs::read_to_string(source.join("nested/inside.txt")).unwrap(), "body");
    }
    #[test]
    fn copied_directory_rename_removes_its_partial_target_after_copy_failure() {
        let d = TestDir::new("copyrenamepartial");
        let source = d.dir("source");
        let _socket = std::os::unix::net::UnixListener::bind(source.join("socket")).unwrap();
        let target = d.join("target");
        copy_then_remove(&source, &target).expect_err("a socket cannot be copied");
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
        let error = copy_then_remove(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, "rename", "remove_file is atomic, so the source is provably whole");
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
        let error = copy_then_remove(&source, &target).expect_err("source removal must fail");
        std::fs::set_permissions(&hold, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(error.where_, "rename", "one unlink removes a symlink too, so the source is provably whole");
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
    // The rclone arm reads the real mountinfo, so a unit test drives only the WebDAV arm; the live rclone battery drives the other.
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
