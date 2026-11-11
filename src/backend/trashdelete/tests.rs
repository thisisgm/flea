use super::*;
use crate::backend::testdir::TestDir;
fn fixture(d: &TestDir) -> (PathBuf, Reviewed) {
    d.dir("files");
    d.dir("info");
    let path = d.file("files/item", "payload");
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    (path, reviewed)
}
fn guard(d: &TestDir, path: &Path) {
    assert!(
        path.is_absolute()
            && !path.as_os_str().is_empty()
            && path.starts_with(d.path())
            && path != d.path()
    );
}
fn restore_fixture(d: &TestDir, original: &Path) -> (PathBuf, Reviewed) {
    let (path, _) = fixture(d);
    std::fs::write(d.join("info/item.trashinfo"), format!("[Trash Info]\nPath={}\n", original.display())).unwrap();
    let meta = path.symlink_metadata().unwrap();
    let reviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    (path, reviewed)
}
#[test]
fn restore_claims_both_identities_and_never_overwrites() {
    let d = TestDir::new("trash-restore-claim");
    let original = d.file("restored", "keep");
    let (path, reviewed) = restore_fixture(&d, &original);
    guard(&d, &path);
    guard(&d, &original);
    assert!(reviewed.restore(&original, d.path()).is_err());
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "payload");
    assert!(d.join("info/item.trashinfo").exists());
    std::fs::rename(&original, d.join("existing-saved")).unwrap();
    reviewed.restore(&original, d.path()).unwrap();
    assert_eq!(std::fs::read_to_string(original).unwrap(), "payload");
    assert!(!path.exists());
    assert!(!d.join("info/item.trashinfo").exists());
}
#[test]
fn restore_replacement_between_check_and_claim_is_preserved() {
    let d = TestDir::new("trash-restore-race");
    let original = d.join("restored");
    let (path, reviewed) = restore_fixture(&d, &original);
    guard(&d, &path);
    guard(&d, &original);
    assert!(reviewed.restore_after(&original, d.path(), || {
        std::fs::rename(&path, d.join("files/old-reviewed")).unwrap();
        std::fs::write(&path, "replacement").unwrap();
    }).is_err());
    assert!(!original.exists());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    assert_eq!(std::fs::read_to_string(d.join("files/old-reviewed")).unwrap(), "payload");
    assert!(d.join("info/item.trashinfo").exists());
}
#[test]
fn restore_recreates_a_missing_parent_and_a_mismatched_original_preserves_trash() {
    let d = TestDir::new("trash-restore-path");
    let original = d.join("missing/deeper/restored");
    let (path, reviewed) = restore_fixture(&d, &original);
    guard(&d, &path);
    guard(&d, &original);
    let wrong = d.join("wrong");
    guard(&d, &wrong);
    assert!(reviewed.restore(&wrong, d.path()).is_err() && !wrong.exists());
    std::fs::write(d.join("missing"), "a file where the folder was").unwrap();
    assert!(reviewed.restore(&original, d.path()).is_err() && path.exists());
    assert!(d.join("info/item.trashinfo").exists());
    std::fs::remove_file(d.join("missing")).unwrap();
    reviewed.restore(&original, d.path()).unwrap();
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "payload");
    assert!(!path.exists() && !d.join("info/item.trashinfo").exists());
}
#[test]
fn original_paths_decode_escaping_and_volume_relative_metadata() {
    let d = TestDir::new("trash-restore-metadata");
    let encoded = d.file("metadata", "[Trash Info]\nPath=folder/photo%20one+two.jpg\n");
    let root = d.join(".Trash-1000");
    assert_eq!(original_path(&File::open(&encoded).unwrap(), &root).unwrap(), d.join("folder/photo one+two.jpg"));
    let root = d.join(".Trash/1000");
    assert_eq!(original_path(&File::open(&encoded).unwrap(), &root).unwrap(), d.join("folder/photo one+two.jpg"));
    std::fs::write(&encoded, "[Trash Info]\nPath=/x/caf%E9.txt\n").unwrap();
    assert_eq!(original_path(&File::open(&encoded).unwrap(), &root).unwrap(), Path::new(OsStr::from_bytes(b"/x/caf\xe9.txt")), "a name that is not UTF-8 keeps its bytes");
    std::fs::write(&encoded, "[Trash Info]\nPath=../escape\n").unwrap();
    assert!(original_path(&File::open(&encoded).unwrap(), &root).is_err());
    std::fs::write(&encoded, "[Trash Info]\nPath=/one\nPath=/two\n").unwrap();
    assert!(original_path(&File::open(&encoded).unwrap(), &root).is_err());
}
#[test]
fn deletes_only_reviewed_item_and_preserves_later_arrival() {
    let d = TestDir::new("trash-delete");
    let (path, reviewed) = fixture(&d);
    let later = d.file("files/later", "keep");
    guard(&d, &path);
    reviewed.delete(d.path()).unwrap();
    assert!(!path.exists());
    assert!(later.exists());
    assert!(!d.join("info/item.trashinfo").exists());
}
// corner: runs as a plain user, where a directory without its write bit refuses the claim.
#[test]
fn a_locked_directory_fails_with_the_reason_spelled_by_io_message() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("trash-delete-locked");
    d.dir("files");
    d.dir("info");
    let path = d.dir("files/locked");
    d.file("files/locked/child.txt", "survive");
    d.file("info/locked.trashinfo", "[Trash Info]\nPath=/original/locked\n");
    guard(&d, &path);
    // Locked before the review, as the ui case locks the backing before it opens the confirmation, because the mode is part of the identity.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o555)).unwrap();
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    let refused = reviewed.delete(d.path());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    // ui/TrashView.qml puts "<name> failed: " before this, so the name is the view's and the reason is io_message's for EACCES.
    assert_eq!(refused.map_err(|error| error.shown()),
        Err("Could not prepare the deletion, so nothing was deleted: permission denied.".to_string()));
    assert_eq!(std::fs::read_to_string(path.join("child.txt")).unwrap(), "survive");
}
#[test]
fn changed_identity_or_size_is_never_deleted() {
    let d = TestDir::new("trash-delete-change");
    let (path, reviewed) = fixture(&d);
    std::fs::rename(&path, d.join("files/old")).unwrap();
    std::fs::write(&path, "replacement").unwrap();
    guard(&d, &path);
    assert!(reviewed.delete(d.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
}
#[test]
fn unreviewed_contents_are_never_deleted() {
    let d = TestDir::new("trash-delete-unreviewed");
    let (path, _) = fixture(&d);
    let meta = path.symlink_metadata().unwrap();
    let unreviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    guard(&d, &path);
    assert!(unreviewed.delete(d.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}
#[test]
fn changed_metadata_is_never_deleted() {
    let d = TestDir::new("trash-delete-metadata");
    let (path, reviewed) = fixture(&d);
    let info = d.join("info/item.trashinfo");
    std::fs::rename(&info, d.join("info/old.trashinfo")).unwrap();
    std::fs::write(&info, "[Trash Info]\nPath=/new-original\n").unwrap();
    guard(&d, &path);
    assert!(reviewed.delete(d.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
    assert!(std::fs::read_to_string(info).unwrap().contains("/new-original"));
}
#[test]
fn backing_paths_must_name_a_trash_payload() {
    assert!(Reviewed::inspect(PathBuf::from("relative/files/item"), "").is_err());
    assert!(Reviewed::inspect(PathBuf::from("/tmp/item"), "").is_err());
}
#[test]
fn replacement_between_check_and_claim_is_restored_without_deletion() {
    let d = TestDir::new("trash-delete-race");
    let (path, reviewed) = fixture(&d);
    guard(&d, &path);
    let result = reviewed.delete_after(d.path(), || {
        std::fs::rename(&path, d.join("files/reviewed-old")).unwrap();
        std::fs::write(&path, "new arrival").unwrap();
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new arrival");
    assert_eq!(
        std::fs::read_to_string(d.join("files/reviewed-old")).unwrap(),
        "payload"
    );
    assert!(d.join("info/item.trashinfo").exists());
}
#[test]
fn directory_contents_are_removed_but_symlinks_never_followed() {
    let d = TestDir::new("trash-delete-directory");
    let (path, _) = fixture(&d);
    std::fs::rename(&path, d.join("files/saved-file")).unwrap();
    std::fs::create_dir(&path).unwrap();
    let target = d.file("target", "keep");
    std::os::unix::fs::symlink(&target, path.join("link")).unwrap();
    std::fs::write(path.join("child"), "delete").unwrap();
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    guard(&d, &path);
    reviewed.delete(d.path()).unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
}
#[test]
fn directory_child_changes_invalidate_review() {
    let d = TestDir::new("trash-delete-child-change");
    d.dir("files/item");
    d.dir("info");
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    let child = d.file("files/item/child", "reviewed");
    let path = d.join("files/item");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    assert_eq!(reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap(), 8);
    std::fs::write(&child, "changed size").unwrap();
    assert!(!reviewed.contents_unchanged(&Cancellation::default()).unwrap());
    guard(&d, &path);
    assert!(reviewed.delete(d.path()).is_err());
    assert_eq!(std::fs::read_to_string(child).unwrap(), "changed size");
}
#[test]
fn directory_arrival_after_claim_survives() {
    let d = TestDir::new("trash-delete-child-arrival");
    d.dir("files/item");
    d.dir("info");
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    d.file("files/item/child", "reviewed");
    let path = d.join("files/item");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed = Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    guard(&d, &path);
    let walks = match_walks();
    let result = reviewed.delete_with(d.path(), || {}, |claimed| {
        std::fs::write(claimed.join("new-arrival"), "keep").unwrap();
    });
    assert_eq!(match_walks() - walks, 1, "one delete runs exactly one matches() walk");
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(path.join("new-arrival")).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(path.join("child")).unwrap(), "reviewed");
}
#[test]
fn deleting_a_reviewed_child_does_not_remove_unreviewed_siblings() {
    let d = TestDir::new("trash-delete-member-set");
    let directory = d.dir("directory");
    let path = d.file("directory/reviewed", "delete");
    let node = Node { relative: PathBuf::new(), identity: identity(&path).unwrap(), children: 0 };
    let later = d.file("directory/later", "keep");
    let quarantine = d.dir("quarantine");
    guard(&d, &path);
    node.claim_remove(&open_dir(&directory).unwrap(), OsStr::new("reviewed"), &open_dir(&quarantine).unwrap(), 0).unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read_to_string(later).unwrap(), "keep");
}
#[test]
fn cancelled_snapshot_never_leaves_an_actionable_review() {
    let d = TestDir::new("trash-review-cancel");
    let (path, mut reviewed) = fixture(&d);
    let cancellation = Cancellation::default();
    let old = cancellation.next();
    cancellation.next();
    assert!(reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &old).is_err());
    guard(&d, &path);
    assert!(reviewed.delete(d.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "payload");
}
#[test]
fn saved_reviews_keep_original_identities_and_manifest_ranges() {
    let d = TestDir::new("trash-review-record");
    let (path, mut reviewed) = fixture(&d);
    let mut manifest = Manifest::new(d.path()).unwrap();
    reviewed.snapshot(&mut manifest, d.path(), &Cancellation::default()).unwrap();
    let restored = Reviewed::from_saved(&reviewed.saved().unwrap(), &manifest).unwrap();
    assert!(restored.contents_unchanged(&Cancellation::default()).unwrap());
    std::fs::write(&path, "different").unwrap();
    assert!(!restored.contents_unchanged(&Cancellation::default()).unwrap());
    guard(&d, &path);
    assert!(restored.delete(d.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "different");
}
#[test]
fn ordinary_path_review_refuses_a_replacement_during_claim() {
    let d = TestDir::new("path-delete-race");
    let path = d.file("selected", "reviewed");
    let mut manifest = Manifest::new(d.path()).unwrap();
    let (reviewed, bytes) = PathReview::prepare(path.clone(), &mut manifest, d.path(), &Cancellation::default()).unwrap();
    assert_eq!(bytes, 8);
    guard(&d, &path);
    assert!(reviewed.delete_after(d.path(), || {
        std::fs::rename(&path, d.join("old-selected")).unwrap();
        std::fs::write(&path, "replacement").unwrap();
    }).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
    assert_eq!(std::fs::read_to_string(d.join("old-selected")).unwrap(), "reviewed");
}
#[test]
fn iterative_review_deletes_deep_trees_without_following_symlinks() {
    let d = TestDir::new("path-delete-deep");
    let root = d.dir("selected");
    let mut deepest = root.clone();
    for _ in 0..300 {
        deepest = deepest.join("d");
        std::fs::create_dir(&deepest).unwrap();
    }
    std::fs::write(deepest.join("payload"), "reviewed").unwrap();
    let outside = d.file("outside", "keep");
    std::os::unix::fs::symlink(&outside, deepest.join("link")).unwrap();
    let mut manifest = Manifest::new(d.path()).unwrap();
    let (reviewed, _) = PathReview::prepare(root.clone(), &mut manifest, d.path(), &Cancellation::default()).unwrap();
    assert!(reviewed.unchanged(&Cancellation::default()).unwrap());
    guard(&d, &root);
    reviewed.delete(d.path()).unwrap();
    assert!(!root.exists());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep");
}
// The Trash view's Restore all holds every claim's folder syncs to one per folder at the batch's close, as undo's home restore does.
#[test]
fn a_restore_batch_syncs_each_folder_once_at_its_close() {
    let d = TestDir::new("trash-restore-syncs");
    d.dir("files");
    d.dir("info");
    let (back, recovery) = (d.dir("back"), d.dir("recovery"));
    let reviewed: Vec<_> = (0..3)
        .map(|i| {
            let path = d.file(&format!("files/item{i}"), "payload");
            d.file(
                &format!("info/item{i}.trashinfo"),
                &format!(
                    "[Trash Info]\nPath={}\n",
                    back.join(format!("item{i}")).display()
                ),
            );
            let meta = path.symlink_metadata().unwrap();
            Reviewed::inspect(path, &format!("l{}:{}", meta.dev(), meta.ino())).unwrap()
        })
        .collect();
    FolderSyncs::take_synced();
    let batch = FolderSyncs::open();
    for (i, item) in reviewed.iter().enumerate() {
        item.restore(&back.join(format!("item{i}")), &recovery)
            .unwrap();
        FolderSyncs::restored(&back);
    }
    assert!(
        FolderSyncs::take_synced().is_empty(),
        "no folder syncs before the batch closes"
    );
    drop(batch);
    let mut once = vec![
        back.clone(),
        std::fs::canonicalize(d.join("files")).unwrap(),
        recovery.clone(),
    ];
    once.sort();
    assert_eq!(
        FolderSyncs::take_synced(),
        once,
        "the destination, files and recovery folders sync once each"
    );
    assert!(
        (0..3).all(|i| back.join(format!("item{i}")).is_file())
            && !d.join("info/item0.trashinfo").exists()
    );
    // With no batch open, a lone restore's folder syncs at once.
    FolderSyncs::restored(&back);
    assert_eq!(FolderSyncs::take_synced(), [back]);
}
