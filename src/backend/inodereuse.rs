// Real inode-reuse fixtures shared by the permission and tag identity tests.
use super::testdir::{allowed_root, TestDir};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const TMPFS_MAGIC: i64 = 0x0102_1994;
const OVERLAY_MAGIC: i64 = 0x794c_7630;
const REUSE_TRIES: usize = 256;

// tmpfs allocates new inode numbers, and an overlay can hide a tmpfs upper, so use a disk-backed sandbox.
pub(super) fn sandbox(tag: &str) -> TestDir {
    let candidates = [Some(std::env::temp_dir()), std::env::current_dir().ok(), Some("/var/tmp".into())];
    for root in candidates.into_iter().flatten().filter_map(|root| allowed_root(&root)) {
        if matches!(super::fsinfo::read_with_magic(&root).1, Some(magic) if magic != TMPFS_MAGIC && magic != OVERLAY_MAGIC) {
            return TestDir::new_in(tag, &root);
        }
    }
    panic!("inode reuse needs a disk-backed sandbox outside HOME; temp, current and /var/tmp roots were unsuitable");
}

// A miss becomes the next original, so another allocator taking its freed inode cannot strand every later try.
pub(super) fn recreate<T>(dir: &TestDir, path: &Path, body: &str, mut record: impl FnMut() -> T) -> T {
    dir.assert_contains(path);
    for attempt in 1..=REUSE_TRIES {
        let before = path.symlink_metadata().expect("the original file");
        let saved = record();
        dir.assert_contains(path);
        std::fs::remove_file(path).expect("unlink the recorded file inside its sandbox");
        std::fs::write(path, body).expect("recreate the recorded file inside its sandbox");
        let after = path.symlink_metadata().expect("the recreated file");
        if (before.dev(), before.ino()) == (after.dev(), after.ino()) {
            eprintln!("inode-reuse proof: inode={} attempts={attempt} filesystem={:?}", after.ino(), super::fsinfo::read(dir.path()).map(|info| info.name));
            return saved;
        }
    }
    panic!("the disk-backed filesystem did not reuse an inode in {REUSE_TRIES} fresh identity attempts");
}

#[test]
fn a_missed_inode_records_the_next_original_before_trying_again() {
    let dir = sandbox("inode-reuse-miss");
    let path = dir.file("a.txt", "original");
    let mut held = None;
    let mut records = 0;
    let old_id = recreate(&dir, &path, "replacement", || {
        records += 1;
        held = (records == 1).then(|| std::fs::File::open(&path).unwrap());
        let meta = path.symlink_metadata().unwrap();
        (meta.dev(), meta.ino())
    });
    assert!(records >= 2, "a held inode cannot be reused on the first try");
    let now = path.symlink_metadata().unwrap();
    assert_eq!(old_id, (now.dev(), now.ino()), "the returned record belongs to the inode actually reused");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    drop(held);
}
