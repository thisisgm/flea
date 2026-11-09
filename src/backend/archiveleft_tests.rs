// Issue 370: on a drive that will not let the stage go, a whole result is a success and the kept work folder is named.
use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::time::{Duration, Instant};

// A folder whose mode refuses the unlink of what it holds, the way rclone refuses the rmdir of an uploading one.
fn stuck(dir: &Path) {
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/f.txt"), b"whole\n").unwrap();
    std::fs::set_permissions(dir.join("sub"), std::fs::Permissions::from_mode(0o555)).unwrap();
}

fn unstick(dir: &Path) {
    let _ = std::fs::set_permissions(dir.join("sub"), std::fs::Permissions::from_mode(0o755));
}

// Root unlinks through any mode, so the refusal these tests stand on cannot be built.
fn as_root() -> bool {
    std::fs::metadata("/proc/self")
        .map(|m| m.uid() == 0)
        .unwrap_or(true)
}

#[test]
fn a_copied_stage_that_will_not_go_publishes_whole_and_its_folder_is_named() {
    if as_root() {
        return;
    }
    let d = TestDir::new("archleft");
    let work = Work::new(d.path(), "ext").unwrap();
    let staged = work.dir.join("out");
    stuck(&staged);
    let dest = d.join("dest");
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_force_copy_fallback(true);
    let published = publish(&staged, &dest, &AtomicBool::new(false));
    crate::backend::renamecompat::test_force_copy_fallback(false);
    crate::backend::renamecompat::test_force_fallback(false);
    published.expect("the copy is whole, so the publish succeeds");
    assert_eq!(std::fs::read(dest.join("sub/f.txt")).unwrap(), b"whole\n");
    let started = Instant::now();
    let left = clear(work);
    assert!(
        started.elapsed() >= Duration::from_secs(3),
        "every pause was tried before the folder was named"
    );
    let left = left.expect("the work folder is named");
    assert!(
        Path::new(&left)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".flea-work-ext-"),
        "{left}"
    );
    unstick(&Path::new(&left).join("out"));
    unstick(&dest);
}

#[test]
fn a_work_folder_the_drive_lets_go_on_a_later_try_leaves_nothing() {
    if as_root() {
        return;
    }
    let d = TestDir::new("archleftlater");
    let work = Work::new(d.path(), "ext").unwrap();
    let dir = work.dir.clone();
    stuck(&dir);
    let later = dir.clone();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        unstick(&later);
    });
    assert_eq!(clear(work), None, "a later try removed it");
    release.join().unwrap();
    assert!(dir.symlink_metadata().is_err());
}
