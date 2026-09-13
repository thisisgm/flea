use super::*;
use super::super::Reviewed;
use crate::backend::testdir::TestDir;
use crate::backend::trashmanifest::Cancellation;
use std::os::unix::fs::MetadataExt;

fn guard(d: &TestDir, path: &Path) {
    assert!(path.is_absolute() && !path.as_os_str().is_empty()
        && path.starts_with(d.path()) && path != d.path());
}
fn move_path(d: &TestDir, from: &Path, to: &Path) {
    guard(d, from);
    guard(d, to);
    std::fs::rename(from, to).unwrap();
}
fn remove_file(d: &TestDir, path: &Path) {
    guard(d, path);
    std::fs::remove_file(path).unwrap();
}
fn fixture(d: &TestDir, child: Option<&str>, destination: Option<&Path>) -> (Reviewed, PathBuf, Recovery) {
    d.dir("files");
    d.dir("info");
    let source = if let Some(child) = child {
        let source = d.dir("files/item");
        let child = source.join(child);
        guard(d, &child);
        std::fs::create_dir_all(child.parent().unwrap()).unwrap();
        std::fs::write(child, "payload").unwrap();
        source
    } else { d.file("files/item", "payload") };
    d.file("info/item.trashinfo", "[Trash Info]\nPath=/original\n");
    let metadata = source.symlink_metadata().unwrap();
    let mut reviewed = Reviewed::inspect(source.clone(), &format!("l{}:{}", metadata.dev(), metadata.ino())).unwrap();
    reviewed.snapshot(&mut Manifest::new(d.path()).unwrap(), d.path(), &Cancellation::default()).unwrap();
    let quarantine = d.dir("files/.flea-delete-test");
    let recovery_root = d.dir("recovery");
    for path in [&source, &reviewed.info, &quarantine, &recovery_root] { guard(d, path); }
    if let Some(destination) = destination { guard(d, destination); }
    let journal = Recovery::begin(&recovery_root, &source, &open_dir(&d.join("files")).unwrap(),
        &reviewed.payload, Some((&reviewed.info, &reviewed.metadata, &open_dir(&d.join("info")).unwrap())), &quarantine,
        &open_dir(&quarantine).unwrap(), if destination.is_none() { reviewed.tree.as_ref() } else { None }, destination).unwrap();
    (reviewed, quarantine, journal)
}
fn claim(d: &TestDir, reviewed: &Reviewed, quarantine: &Path, metadata: bool) {
    move_path(d, &reviewed.path, &quarantine.join("payload"));
    if metadata { move_path(d, &reviewed.info, &quarantine.join("metadata")); }
}
fn claim_node(d: &TestDir, reviewed: &Reviewed, quarantine: &Path, relative: &Path) -> PathBuf {
    let tree = reviewed.tree.as_ref().unwrap();
    let mut cursor = tree.start();
    loop {
        let offset = cursor - tree.start();
        let node = Node::decode(&tree.next(&mut cursor).unwrap().expect("reviewed child")).unwrap();
        if node.relative == relative {
            let claimed = quarantine.join(format!("entry-{}", offset));
            let payload = quarantine.join("payload");
            let source = if relative.as_os_str().is_empty() { payload } else { payload.join(relative) };
            move_path(d, &source, &claimed);
            return claimed;
        }
    }
}
fn replay(d: &TestDir) -> RecoveryReport {
    let root = d.join("recovery");
    guard(d, &root);
    guard(d, &d.join("files"));
    guard(d, &d.join("info"));
    recover(&root).unwrap()
}
fn clean(d: &TestDir, quarantine: &Path, journal: &Path) {
    let report = replay(d);
    assert_eq!(report.failures, Vec::<String>::new());
    assert_eq!(report.recovered, 1);
    assert!(!quarantine.exists());
    assert!(!journal.exists());
    let repeated = replay(d);
    assert_eq!(repeated.recovered, 0);
    assert!(repeated.failures.is_empty());
}

#[test]
fn recovery_skips_a_live_record_lock() {
    let d = TestDir::new("recovery-active");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    let report = replay(&d);
    assert_eq!(report.recovered, 0);
    assert!(report.failures.is_empty());
    assert!(!reviewed.path.exists());
    assert!(quarantine.join("payload").exists());
    let record = journal.path.clone();
    drop(journal);
    clean(&d, &quarantine, &record);
    assert_eq!(std::fs::read_to_string(&reviewed.path).unwrap(), "payload");
    assert!(reviewed.info.exists());
}

#[test]
fn recovery_returns_payload_only_and_paired_root_claims() {
    for metadata in [false, true] {
        let d = TestDir::new("recovery-roots");
        let (reviewed, quarantine, journal) = fixture(&d, None, None);
        claim(&d, &reviewed, &quarantine, metadata);
        let record = journal.path.clone();
        drop(journal);
        clean(&d, &quarantine, &record);
        assert_eq!(std::fs::read_to_string(&reviewed.path).unwrap(), "payload");
        assert_eq!(identity(&reviewed.info).unwrap(), reviewed.metadata);
    }
}

#[test]
fn recovery_returns_interrupted_child_and_entry_zero_claims() {
    for relative in [Path::new("child"), Path::new("")] {
        let d = TestDir::new("recovery-child");
        let (reviewed, quarantine, journal) = fixture(&d, (!relative.as_os_str().is_empty()).then_some("child"), None);
        claim(&d, &reviewed, &quarantine, true);
        let interrupted = claim_node(&d, &reviewed, &quarantine, relative);
        let record = journal.path.clone();
        drop(journal);
        clean(&d, &quarantine, &record);
        assert!(!interrupted.exists());
        let source = if relative.as_os_str().is_empty() { reviewed.path.clone() } else { reviewed.path.join(relative) };
        assert_eq!(std::fs::read_to_string(source).unwrap(), "payload");
        assert!(reviewed.info.exists());
    }
}

#[test]
fn recovery_never_overwrites_an_original_name_collision() {
    let d = TestDir::new("recovery-collision");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    d.file("files/item", "later arrival");
    let record = journal.path.clone();
    drop(journal);
    let report = replay(&d);
    assert_eq!(report.recovered, 0);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(std::fs::read_to_string(&reviewed.path).unwrap(), "later arrival");
    assert_eq!(std::fs::read_to_string(quarantine.join("payload")).unwrap(), "payload");
    assert!(quarantine.join("metadata").exists());
    assert!(record.exists());
}

#[test]
fn recovery_never_overwrites_an_interrupted_child_collision() {
    let d = TestDir::new("recovery-child-collision");
    let (reviewed, quarantine, journal) = fixture(&d, Some("child"), None);
    claim(&d, &reviewed, &quarantine, true);
    let interrupted = claim_node(&d, &reviewed, &quarantine, Path::new("child"));
    std::fs::write(quarantine.join("payload/child"), "later arrival").unwrap();
    drop(journal);
    let report = replay(&d);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(std::fs::read_to_string(interrupted).unwrap(), "payload");
    assert_eq!(std::fs::read_to_string(quarantine.join("payload/child")).unwrap(), "later arrival");
}

#[test]
fn recovery_refuses_a_replaced_quarantine() {
    let d = TestDir::new("recovery-quarantine-replacement");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    let saved = d.join("files/saved-quarantine");
    move_path(&d, &quarantine, &saved);
    std::fs::create_dir(&quarantine).unwrap();
    std::fs::write(quarantine.join("payload"), "replacement").unwrap();
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert_eq!(std::fs::read_to_string(saved.join("payload")).unwrap(), "payload");
    assert_eq!(std::fs::read_to_string(quarantine.join("payload")).unwrap(), "replacement");
    assert!(!reviewed.path.exists());
}

#[test]
fn recovery_validates_payload_before_returning_a_child() {
    let d = TestDir::new("recovery-payload-replacement");
    let (reviewed, quarantine, journal) = fixture(&d, Some("child"), None);
    claim(&d, &reviewed, &quarantine, true);
    let interrupted = claim_node(&d, &reviewed, &quarantine, Path::new("child"));
    move_path(&d, &quarantine.join("payload"), &d.join("files/saved-payload"));
    std::fs::create_dir(quarantine.join("payload")).unwrap();
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert_eq!(std::fs::read_to_string(interrupted).unwrap(), "payload");
    assert!(!quarantine.join("payload/child").exists());
    assert!(!reviewed.path.exists());
}

#[test]
fn recovery_cleans_a_completed_deletion_and_its_record() {
    let d = TestDir::new("recovery-completed-delete");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    remove_file(&d, &quarantine.join("payload"));
    let record = journal.path.clone();
    drop(journal);
    clean(&d, &quarantine, &record);
    assert!(!reviewed.path.exists());
    assert!(!reviewed.info.exists());
}

#[test]
fn recovery_cleans_restore_metadata_only_for_its_verified_destination() {
    for replace_destination in [false, true] {
        let d = TestDir::new("recovery-completed-restore");
        let destination = d.join("restored");
        let (reviewed, quarantine, journal) = fixture(&d, None, Some(&destination));
        claim(&d, &reviewed, &quarantine, true);
        move_path(&d, &quarantine.join("payload"), &destination);
        if replace_destination {
            move_path(&d, &destination, &d.join("saved-destination"));
            std::fs::write(&destination, "replacement").unwrap();
        }
        let record = journal.path.clone();
        drop(journal);
        if replace_destination {
            assert_eq!(replay(&d).failures.len(), 1);
            assert!(quarantine.join("metadata").exists());
            assert!(record.exists());
            assert_eq!(std::fs::read_to_string(destination).unwrap(), "replacement");
        } else {
            clean(&d, &quarantine, &record);
            assert_eq!(std::fs::read_to_string(destination).unwrap(), "payload");
        }
    }
}

#[test]
fn recovery_keeps_malformed_and_nonregular_records() {
    let d = TestDir::new("recovery-malformed");
    d.dir("recovery");
    d.file("recovery/.flea-delete-truncated.review", "short");
    let malformed = d.join("recovery/.flea-delete-malformed.review");
    let mut record = Manifest::create(&malformed).unwrap();
    record.append(b"{}").unwrap();
    drop(record);
    d.dir("recovery/.flea-delete-directory.review");
    let target = d.file("link-target", "keep");
    std::os::unix::fs::symlink(&target, d.join("recovery/.flea-delete-link.review")).unwrap();
    assert!(std::process::Command::new("mkfifo").arg(d.join("recovery/.flea-delete-pipe.review")).status().unwrap().success());
    let report = replay(&d);
    assert_eq!(report.recovered, 0);
    assert_eq!(report.failures.len(), 5);
    assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 5);
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
}

#[test]
fn recovery_retains_unknown_quarantine_contents() {
    let d = TestDir::new("recovery-unknown");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    std::fs::write(quarantine.join("unrecognized"), "keep").unwrap();
    let record = journal.path.clone();
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert!(record.exists());
    assert_eq!(std::fs::read_to_string(quarantine.join("unrecognized")).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(quarantine.join("payload")).unwrap(), "payload");
    assert!(quarantine.join("metadata").exists());
    assert!(!reviewed.path.exists());
}

#[test]
fn recovery_refuses_same_inode_payload_edits_before_moving_metadata() {
    for returned in [false, true] {
        let d = TestDir::new("recovery-payload-edit");
        let (reviewed, quarantine, journal) = fixture(&d, None, None);
        claim(&d, &reviewed, &quarantine, true);
        let payload = if returned {
            move_path(&d, &quarantine.join("payload"), &reviewed.path);
            reviewed.path.clone()
        } else { quarantine.join("payload") };
        std::fs::write(&payload, "changed payload").unwrap();
        assert_eq!(identity(&payload).unwrap().ino, reviewed.payload.ino);
        drop(journal);
        assert_eq!(replay(&d).failures.len(), 1);
        assert_eq!(std::fs::read_to_string(payload).unwrap(), "changed payload");
        assert!(quarantine.join("metadata").exists());
        assert!(!reviewed.info.exists());
    }
}

#[test]
fn recovery_preserves_edited_metadata_before_return_and_after_deletion() {
    for completed in [false, true] {
        let d = TestDir::new("recovery-metadata-edit");
        let (reviewed, quarantine, journal) = fixture(&d, None, None);
        claim(&d, &reviewed, &quarantine, true);
        if completed { remove_file(&d, &quarantine.join("payload")); }
        let metadata = quarantine.join("metadata");
        std::fs::write(&metadata, "changed metadata").unwrap();
        assert_eq!(identity(&metadata).unwrap().ino, reviewed.metadata.ino);
        drop(journal);
        assert_eq!(replay(&d).failures.len(), 1);
        assert_eq!(std::fs::read_to_string(metadata).unwrap(), "changed metadata");
        assert!(!reviewed.path.exists());
        assert!(!reviewed.info.exists());
    }
}

#[test]
fn recovery_never_returns_metadata_into_a_replaced_info_directory() {
    let d = TestDir::new("recovery-info-parent");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    move_path(&d, &d.join("info"), &d.join("saved-info"));
    d.dir("info");
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert!(quarantine.join("metadata").exists());
    assert!(!reviewed.info.exists());
    assert_eq!(std::fs::read_to_string(reviewed.path).unwrap(), "payload");
}

#[test]
fn recovery_keeps_metadata_when_unknown_data_remains_after_deletion() {
    let d = TestDir::new("recovery-unknown-completed");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    remove_file(&d, &quarantine.join("payload"));
    std::fs::write(quarantine.join("unrecognized"), "keep").unwrap();
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert!(quarantine.join("metadata").exists());
    assert_eq!(std::fs::read_to_string(quarantine.join("unrecognized")).unwrap(), "keep");
}

#[test]
fn recovery_never_returns_a_child_into_a_replaced_ancestor() {
    let d = TestDir::new("recovery-child-parent");
    let (reviewed, quarantine, journal) = fixture(&d, Some("branch/leaf"), None);
    claim(&d, &reviewed, &quarantine, true);
    let interrupted = claim_node(&d, &reviewed, &quarantine, Path::new("branch/leaf"));
    move_path(&d, &quarantine.join("payload/branch"), &d.join("saved-branch"));
    std::fs::create_dir(quarantine.join("payload/branch")).unwrap();
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert_eq!(std::fs::read_to_string(interrupted).unwrap(), "payload");
    assert!(!quarantine.join("payload/branch/leaf").exists());
    assert!(quarantine.join("metadata").exists());
}

#[test]
fn recovery_keeps_its_record_when_the_source_parent_was_replaced() {
    let d = TestDir::new("recovery-source-parent");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    move_path(&d, &d.join("files"), &d.join("saved-files"));
    d.dir("files");
    let record = journal.path.clone();
    drop(journal);
    assert_eq!(replay(&d).failures.len(), 1);
    assert!(record.exists());
    assert_eq!(std::fs::read_to_string(d.join("saved-files/.flea-delete-test/payload")).unwrap(), "payload");
    assert!(d.join("saved-files/.flea-delete-test/metadata").exists());
}

#[test]
fn recovery_preserves_an_unmarked_record_when_the_quarantine_is_missing() {
    let d = TestDir::new("recovery-missing-unmarked");
    let (reviewed, quarantine, journal) = fixture(&d, None, None);
    claim(&d, &reviewed, &quarantine, true);
    let moved = d.join("files/moved-quarantine");
    move_path(&d, &quarantine, &moved);
    let record = journal.path.clone();
    drop(journal);
    let report = replay(&d);
    assert_eq!(report.recovered, 0);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("without a completion record"));
    assert!(record.exists());
    assert_eq!(std::fs::read_to_string(moved.join("payload")).unwrap(), "payload");
    assert!(moved.join("metadata").exists());
}

#[test]
fn recovery_finishes_marked_cleanup_before_and_after_quarantine_removal() {
    for removed in [false, true] {
        let d = TestDir::new("recovery-missing-completed");
        let (reviewed, quarantine, mut journal) = fixture(&d, None, None);
        claim(&d, &reviewed, &quarantine, true);
        remove_file(&d, &quarantine.join("payload"));
        remove_file(&d, &quarantine.join("metadata"));
        guard(&d, &quarantine);
        if removed {
            journal.remove_empty(&quarantine, &open_dir(&quarantine).unwrap(), &open_dir(&d.join("files")).unwrap()).unwrap();
        } else {
            journal.record.append(FINISHED).unwrap();
            journal.record.sync().unwrap();
        }
        let record = journal.path.clone();
        drop(journal);
        clean(&d, &quarantine, &record);
        assert!(!reviewed.path.exists());
        assert!(!reviewed.info.exists());
    }
}
