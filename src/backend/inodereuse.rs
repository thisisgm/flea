// Recreated-file fixtures shared by the permission and tag identity tests.
use super::testdir::{allowed_root, TestDir};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const TMPFS_MAGIC: i64 = 0x0102_1994;
const REUSE_TRIES: usize = 256;

pub(super) struct Recreated<T> {
    pub saved: T,
    pub reused_inode: bool,
}

// Prefer a non-tmpfs root to exercise real reuse, but keep the identity proof valid on every filesystem.
pub(super) fn sandbox(tag: &str) -> TestDir {
    let candidates = [Some(std::env::temp_dir()), std::env::current_dir().ok(), Some("/var/tmp".into())];
    for root in candidates.into_iter().flatten().filter_map(|root| allowed_root(&root)) {
        if matches!(super::fsinfo::read_with_magic(&root).1, Some(magic) if magic != TMPFS_MAGIC) {
            return TestDir::new_in(tag, &root);
        }
    }
    TestDir::new(tag)
}

// Every retry records its own original; the fallback holds that original so replacement must get a different inode.
pub(super) fn recreate<T>(dir: &TestDir, path: &Path, body: &str, mut record: impl FnMut() -> T) -> Recreated<T> {
    dir.assert_contains(path);
    for attempt in 1..=REUSE_TRIES {
        let before = path.symlink_metadata().expect("the original file");
        let birth = (super::permissions::born_of(&before), super::filegen::of_path(path));
        let saved = record();
        dir.assert_contains(path);
        std::fs::remove_file(path).expect("unlink the recorded file inside its sandbox");
        std::fs::write(path, body).expect("recreate the recorded file inside its sandbox");
        let after = path.symlink_metadata().expect("the recreated file");
        let changed = super::undo::born_differs(birth.0, super::permissions::born_of(&after))
            || super::filegen::differs(birth.1, super::filegen::of_path(path));
        if (before.dev(), before.ino()) == (after.dev(), after.ino()) && changed {
            eprintln!("inode-reuse proof: arm=reused-inode inode={} attempts={attempt} filesystem={:?}", after.ino(), super::fsinfo::read(dir.path()).map(|info| info.name));
            return Recreated { saved, reused_inode: true };
        }
    }
    let saved = record();
    dir.replace_file(path, body);
    eprintln!("inode-reuse proof: arm=different-inode attempts={REUSE_TRIES} filesystem={:?}", super::fsinfo::read(dir.path()).map(|info| info.name));
    Recreated { saved, reused_inode: false }
}

#[test]
fn a_missed_inode_records_the_next_original_before_trying_again() {
    let dir = sandbox("inode-reuse-miss");
    let path = dir.file("a.txt", "original");
    let mut held = None;
    let mut records = 0;
    let recreated = recreate(&dir, &path, "replacement", || {
        records += 1;
        held = (records == 1).then(|| std::fs::File::open(&path).unwrap());
        let meta = path.symlink_metadata().unwrap();
        (meta.dev(), meta.ino())
    });
    assert!(records >= 2, "a held inode cannot be reused on the first try");
    let now = path.symlink_metadata().unwrap();
    if recreated.reused_inode {
        assert_eq!(recreated.saved, (now.dev(), now.ino()), "the returned record belongs to the inode actually reused");
    } else {
        assert_ne!(recreated.saved, (now.dev(), now.ino()), "the fallback replaces the recorded identity");
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    drop(held);
}

#[test]
fn a_forced_new_inode_returns_the_last_recorded_original() {
    let dir = sandbox("inode-reuse-new");
    let path = dir.file("a.txt", "original");
    let mut held = None;
    let mut records = 0;
    let recreated = recreate(&dir, &path, "replacement", || {
        records += 1;
        held = Some(std::fs::File::open(&path).unwrap());
        let meta = path.symlink_metadata().unwrap();
        (records, (meta.dev(), meta.ino()))
    });
    assert!(!recreated.reused_inode, "holding each original prevents reuse on every attempt");
    assert_eq!(records, REUSE_TRIES + 1, "the fallback records its own original");
    assert_eq!(recreated.saved.0, records, "the returned record is the last original, not a missed attempt");
    let now = path.symlink_metadata().unwrap();
    assert_ne!(recreated.saved.1, (now.dev(), now.ino()));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    drop(held);
}
