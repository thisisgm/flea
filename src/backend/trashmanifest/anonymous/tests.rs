use super::*;
use crate::backend::testdir::TestDir;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::{symlink, PermissionsExt};

fn private_root(tag: &str) -> TestDir {
    let root = TestDir::new(tag);
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}

#[test]
fn opr_reds_unsupported_tmpfile_keeps_private_anonymous_fd_semantics() {
    let root = private_root("anonymous-fallback");
    for errno in [95, 21, 22] {
        let mut file = open_with(root.path(), None, None, |_| {
            Err(io::Error::from_raw_os_error(errno))
        })
        .expect("unsupported O_TMPFILE must retain review functionality");
        let metadata = file.metadata().unwrap();
        assert_eq!(metadata.nlink(), 0, "nothing named survives the open");
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.dev(), root.path().metadata().unwrap().dev());
        unsafe extern "C" {
            fn fcntl(fd: i32, command: i32, ...) -> i32;
        }
        assert_eq!(
            unsafe { fcntl(file.as_raw_fd(), 1) } & 1,
            1,
            "FD_CLOEXEC is atomic"
        );
        file.write_all(b"private review").unwrap();
        let mut held = file.try_clone().unwrap();
        drop(file);
        held.seek(SeekFrom::Start(0)).unwrap();
        let mut content = String::new();
        held.read_to_string(&mut content).unwrap();
        assert_eq!(content, "private review");
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            1,
            "only fixture marker remains"
        );
    }
}

#[test]
fn opr_reds_other_tmpfile_errors_remain_errors_without_named_fallback() {
    let root = private_root("anonymous-errors");
    for errno in [1, 2, 13, 24, 28, 30] {
        let error = open_with(root.path(), None, None, |_| {
            Err(io::Error::from_raw_os_error(errno))
        })
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(errno));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}

#[test]
fn opr_reds_private_runtime_is_preferred_and_untrusted_runtime_is_ignored() {
    let root = private_root("anonymous-runtime");
    let runtime = root.dir("runtime");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let missing = root.join("missing");
    let file = open_with(&missing, Some(&runtime), Some(&missing), |_| {
        Err(io::Error::from_raw_os_error(95))
    })
    .unwrap();
    assert_eq!(file.metadata().unwrap().nlink(), 0);
    assert_eq!(std::fs::read_dir(&runtime).unwrap().count(), 0);
    let link = root.join("runtime-link");
    symlink(&runtime, &link).unwrap();
    assert!(open_with(&missing, Some(&link), Some(&missing), |_| Err(
        io::Error::from_raw_os_error(95)
    ))
    .is_err());
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(open_with(&missing, Some(&runtime), Some(&missing), |_| Err(
        io::Error::from_raw_os_error(95)
    ))
    .is_err());
    assert_eq!(std::fs::read_dir(&runtime).unwrap().count(), 0);
}

#[test]
fn opr_reds_successful_primary_is_returned_without_fallback() {
    let root = private_root("anonymous-primary");
    let path = root.file("held", "held review");
    let original = File::open(&path).unwrap();
    let inode = original.metadata().unwrap().ino();
    std::fs::remove_file(path).unwrap();
    let mut original = Some(original);
    let file = open_with(&root.join("missing"), None, None, |_| {
        Ok(original.take().unwrap())
    })
    .unwrap();
    assert_eq!(file.metadata().unwrap().ino(), inode);
    assert_eq!(
        file.metadata().unwrap().nlink(),
        0,
        "the injected successful fd stays untouched"
    );
}

#[test]
fn opr_reds_exclusive_open_failure_never_unlinks_an_existing_review() {
    let root = private_root("anonymous-exclusive");
    let private = PrivateDirectory::create(root.path()).unwrap();
    let path = format!("/proc/self/fd/{}/review", private.directory.as_raw_fd());
    let witness = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    assert_eq!(
        create_review(private).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        witness.metadata().unwrap().nlink(),
        1,
        "failed exclusive open must not delete another file"
    );
}

#[test]
fn opr_reds_nonsticky_writable_parent_is_refused_before_creation() {
    let root = private_root("anonymous-parent");
    let parent = root.dir("unsafe");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    let result = named_then_unlinked(&parent);
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(std::fs::read_dir(parent).unwrap().count(), 0);
}

fn unlinked_fixture(root: &Path) -> io::Result<File> {
    let path = root.join("injected-primary");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    std::fs::remove_file(path)?;
    Ok(file)
}

#[test]
fn opr_reds_delete_candidates_follow_configured_runtime_local_order() {
    let root = private_root("anonymous-order");
    let runtime = root.dir("runtime");
    let local = root.dir("local");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let expected = [root.path().to_path_buf(), runtime.clone(), local.clone()];
    for success_at in 0..3 {
        let mut seen = Vec::new();
        let file = open_with(root.path(), Some(&runtime), Some(&local), |candidate| {
            seen.push(candidate.to_path_buf());
            if candidate == expected[success_at] {
                return unlinked_fixture(candidate);
            }
            Err(io::Error::from_raw_os_error(if candidate == root.path() {
                95
            } else {
                13
            }))
        })
        .unwrap();
        assert_eq!(seen, expected[..=success_at]);
        assert_eq!(file.metadata().unwrap().nlink(), 0);
    }
    let mut seen = Vec::new();
    let error = open_with(root.path(), Some(&runtime), Some(&local), |candidate| {
        seen.push(candidate.to_path_buf());
        Err(io::Error::from_raw_os_error(if candidate == root.path() {
            95
        } else if candidate == runtime {
            13
        } else {
            28
        }))
    })
    .unwrap_err();
    assert_eq!(seen, expected);
    assert_eq!(error.raw_os_error(), Some(28));
}

#[test]
fn opr_reds_named_runtime_and_local_candidates_require_anonymous_fds() {
    let root = private_root("anonymous-named-order");
    let runtime = root.dir("runtime");
    let local = root.dir("local");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    for runtime_error in [95, 13] {
        std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut seen = Vec::new();
        let file = open_with(root.path(), Some(&runtime), Some(&local), |candidate| {
            seen.push(candidate.to_path_buf());
            Err(io::Error::from_raw_os_error(if candidate == runtime {
                runtime_error
            } else {
                95
            }))
        })
        .unwrap();
        assert_eq!(file.metadata().unwrap().nlink(), 0);
        assert_eq!(seen.len(), if runtime_error == 95 { 2 } else { 3 });
        assert_eq!(std::fs::read_dir(&runtime).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&local).unwrap().count(), 0);
    }
}

#[test]
fn opr_reds_primary_errors_and_copy_placement_do_not_try_other_candidates() {
    let root = private_root("anonymous-placement");
    let runtime = root.dir("runtime");
    let local = root.dir("local");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut seen = Vec::new();
    let error = open_with(root.path(), Some(&runtime), Some(&local), |candidate| {
        seen.push(candidate.to_path_buf());
        Err(io::Error::from_raw_os_error(13))
    })
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(13));
    assert_eq!(seen, [root.path()]);
    seen.clear();
    let file = open_with(root.path(), Some(&runtime), None, |candidate| {
        seen.push(candidate.to_path_buf());
        Err(io::Error::from_raw_os_error(95))
    })
    .unwrap();
    assert_eq!(seen, [root.path()]);
    assert_eq!(
        file.metadata().unwrap().dev(),
        root.path().metadata().unwrap().dev()
    );
}

#[test]
fn opr_reds_retained_links_are_refused_and_rejected_directories_are_cleaned() {
    let root = private_root("anonymous-refusals");
    let path = root.file("still-named", "unchanged");
    let file = File::open(&path).unwrap();
    assert_eq!(
        anonymous(file).unwrap_err().raw_os_error(),
        Some(95),
        "an NFS-style retained link is not anonymous"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"unchanged");
    let before = std::fs::read_dir(root.path()).unwrap().count();
    let error = PrivateDirectory::create_checked(root.path(), |_| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected metadata rejection",
        ))
    })
    .err()
    .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), before);
}
