use super::*;
use crate::backend::homerestore::RESTORE_TAKEN;
use crate::backend::hometrash::set_test_trash_root;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;

// The mount body naming one subvolume mount inside the sandbox, so every probe and rename stays in it.
fn subvol_body(d: &TestDir) -> String {
    format!(
        "28 22 259:1 /@data {} rw - btrfs /dev/nvme0n1p1 rw",
        d.join("vol").display()
    )
}

// The sandbox's own Trash directory name, the uid the whole module names its directories by.
fn own_trash(d: &TestDir, under: &str) -> PathBuf {
    d.join(under).join(format!(
        ".Trash-{}",
        crate::backend::manifestdir::current_uid()
    ))
}

#[test]
fn mount_roots_read_the_root_field_unescape_it_and_serve_only_block_backed_subvolumes() {
    let body = "28 22 259:1 /@data /mnt/data rw - btrfs /dev/nvme0n1p1 rw\n29 22 259:1 / /mnt/whole rw - ext4 /dev/sdb1 rw\n40 22 0:50 /x /mnt/data/net rw - nfs4 srv:/x rw\n50 22 0:60 /sub /mnt/win rw - fuseblk /dev/sda1 rw\n";
    let mounts = mountinfo::mount_roots_in(body);
    let at = |path: &str| mountinfo::deepest(Path::new(path), &mounts).cloned();
    assert_eq!(
        at("/mnt/win/f"),
        Some((PathBuf::from("/mnt/win"), PathBuf::from("/sub"), false)),
        "a FUSE mount of a block device stays with gio"
    );
    assert_eq!(
        at("/mnt/data/f.txt"),
        Some((PathBuf::from("/mnt/data"), PathBuf::from("/@data"), true))
    );
    assert_eq!(
        at("/mnt/whole/f"),
        Some((PathBuf::from("/mnt/whole"), PathBuf::from("/"), false))
    );
    assert_eq!(
        at("/mnt/data/net/f"),
        Some((PathBuf::from("/mnt/data/net"), PathBuf::from("/x"), false))
    );
    assert_eq!(at("/elsewhere/f"), None);
    let escaped =
        mountinfo::mount_roots_in("30 22 0:9 /my\\040root /media/d rw - ext4 /dev/sdc1 rw\n");
    assert_eq!(
        mountinfo::deepest(Path::new("/media/d/f"), &escaped).map(|(_, root, _)| root.clone()),
        Some(PathBuf::from("/my root"))
    );
}

#[test]
fn a_root_that_is_not_slash_is_taken_and_a_whole_mount_is_left_to_gio() {
    let d = TestDir::new("topdir-eligible");
    d.dir("vol");
    let file = d.file("vol/a.txt", "x");
    assert!(
        !renames_to_topdir(&mut Batch::read(), &file),
        "no mount table names the sandbox, so gio keeps it"
    );
    test_set_mount_body(Some(subvol_body(&d)));
    assert!(
        renames_to_topdir(&mut Batch::read(), &file),
        "a subvolume mount renames into its own topdir Trash"
    );
    assert!(
        !own_trash(&d, "vol").exists(),
        "classification creates no directories"
    );
    test_set_mount_body(Some(
        "29 22 259:1 / /mnt/whole rw - ext4 /dev/sdb1 rw\n".to_string(),
    ));
    assert!(
        !renames_to_topdir(&mut Batch::read(), Path::new("/mnt/whole/f.txt")),
        "a whole-filesystem mount is gio's own"
    );
    test_set_mount_body(None);
}

#[test]
fn the_home_filesystem_stays_the_home_renames_own() {
    let d = TestDir::new("topdir-home-wins");
    d.dir("vol");
    let trash = d.dir("Trash");
    d.dir("Trash/files");
    d.dir("Trash/info");
    let file = d.file("vol/a.txt", "x");
    set_test_trash_root(Some(trash));
    test_set_mount_body(Some(subvol_body(&d)));
    assert!(
        !renames_to_topdir(&mut Batch::read(), &file),
        "the home rename wins the partition first"
    );
    set_test_trash_root(None);
    test_set_mount_body(None);
}

#[test]
fn a_rename_moves_the_item_and_writes_its_record() {
    let d = TestDir::new("topdir-rename");
    d.dir("vol");
    let file = d.file("vol/kit.zip", "body");
    test_set_mount_body(Some(subvol_body(&d)));
    let mut unsynced = std::collections::BTreeSet::new();
    let entry = match rename_to_topdir(&mut Batch::read(), &file, &mut unsynced) {
        Some(Ok(entry)) => entry,
        other => panic!("the rename ran: {other:?}"),
    };
    let trash = own_trash(&d, "vol");
    assert!(!file.exists(), "the item left its place");
    assert_eq!(
        std::fs::read_to_string(trash.join("files/kit.zip")).unwrap(),
        "body"
    );
    let record = std::fs::read_to_string(trash.join("info/kit.zip.trashinfo")).unwrap();
    assert!(record.starts_with("[Trash Info]\nPath="), "{record}");
    assert!(
        record.contains("Path=kit.zip\n"),
        "the record is relative to the mount: {record}"
    );
    assert!(record.contains("\nDeletionDate="), "{record}");
    assert_eq!(entry.original, file);
    assert_eq!(entry.uri, records::uri(&trash.join("files/kit.zip")));
    test_set_mount_body(None);
}

#[test]
fn a_taken_trash_name_takes_the_next_suffix() {
    let d = TestDir::new("topdir-collision");
    d.dir("vol");
    let file = d.file("vol/kit.zip", "new");
    test_set_mount_body(Some(subvol_body(&d)));
    let trash = own_trash(&d, "vol");
    locations::at(&d.join("vol"), true);
    std::fs::write(trash.join("files/kit.zip"), "old").unwrap();
    let mut unsynced = std::collections::BTreeSet::new();
    let entry = match rename_to_topdir(&mut Batch::read(), &file, &mut unsynced) {
        Some(Ok(entry)) => entry,
        other => panic!("the rename ran: {other:?}"),
    };
    assert_eq!(
        std::fs::read_to_string(trash.join("files/kit.zip")).unwrap(),
        "old"
    );
    assert_eq!(
        std::fs::read_to_string(trash.join("files/kit.2.zip")).unwrap(),
        "new"
    );
    assert_eq!(entry.uri, records::uri(&trash.join("files/kit.2.zip")));
    test_set_mount_body(None);
}

#[test]
fn an_administrators_sticky_trash_wins_and_an_unsticky_one_does_not() {
    let d = TestDir::new("topdir-admin");
    let admin = d.dir("vol/.Trash");
    std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o1777)).unwrap();
    test_set_mount_body(Some(subvol_body(&d)));
    let mut unsynced = std::collections::BTreeSet::new();
    let first = d.file("vol/a.txt", "x");
    assert!(matches!(
        rename_to_topdir(&mut Batch::read(), &first, &mut unsynced),
        Some(Ok(_))
    ));
    let shared = d
        .join("vol/.Trash")
        .join(crate::backend::manifestdir::current_uid().to_string());
    assert!(
        shared.join("files/a.txt").is_file(),
        "the sticky administrator Trash holds it"
    );
    std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let second = d.file("vol/b.txt", "y");
    assert!(matches!(
        rename_to_topdir(&mut Batch::read(), &second, &mut unsynced),
        Some(Ok(_))
    ));
    assert!(
        own_trash(&d, "vol").join("files/b.txt").is_file(),
        "an unsticky .Trash is not trusted"
    );
    test_set_mount_body(None);
}

#[test]
fn a_trash_directory_that_is_a_link_is_never_followed() {
    let d = TestDir::new("topdir-link");
    d.dir("elsewhere");
    d.dir("vol");
    std::os::unix::fs::symlink(d.join("elsewhere"), own_trash(&d, "vol")).unwrap();
    let file = d.file("vol/a.txt", "x");
    test_set_mount_body(Some(subvol_body(&d)));
    assert!(
        rename_to_topdir(&mut Batch::read(), &file, &mut Default::default()).is_none(),
        "a linked Trash is never followed"
    );
    test_set_mount_body(None);
}

#[test]
fn undo_puts_the_item_back_on_the_records_proof() {
    let d = TestDir::new("topdir-restore");
    d.dir("vol");
    let file = d.file("vol/kit.zip", "body");
    test_set_mount_body(Some(subvol_body(&d)));
    let mut unsynced = std::collections::BTreeSet::new();
    let entry = match rename_to_topdir(&mut Batch::read(), &file, &mut unsynced) {
        Some(Ok(entry)) => entry,
        other => panic!("the rename ran: {other:?}"),
    };
    assert!(
        matches!(restore_from_topdir(&entry), Some(Ok(()))),
        "the record proves the entry"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "body");
    let trash = own_trash(&d, "vol");
    assert!(
        !trash.join("files/kit.zip").exists() && !trash.join("info/kit.zip.trashinfo").exists(),
        "the item and its record left the Trash"
    );
    // A record naming another path proves nothing, so the entry stays gio's.
    let mut unsynced = std::collections::BTreeSet::new();
    let entry = match rename_to_topdir(&mut Batch::read(), &file, &mut unsynced) {
        Some(Ok(entry)) => entry,
        other => panic!("the rename ran: {other:?}"),
    };
    let elsewhere = Entry {
        original: d.join("vol/other.zip"),
        ..entry
    };
    assert!(
        matches!(restore_from_topdir(&elsewhere), Some(Err(_))),
        "a record for another path is not proof"
    );
    assert!(!file.exists(), "the trashed item stayed in the Trash");
    test_set_mount_body(None);
}

#[test]
fn a_taken_original_refuses_and_keeps_the_item() {
    let d = TestDir::new("topdir-restore-taken");
    d.dir("vol");
    let file = d.file("vol/kit.zip", "body");
    test_set_mount_body(Some(subvol_body(&d)));
    let mut unsynced = std::collections::BTreeSet::new();
    let entry = match rename_to_topdir(&mut Batch::read(), &file, &mut unsynced) {
        Some(Ok(entry)) => entry,
        other => panic!("the rename ran: {other:?}"),
    };
    std::fs::write(&file, "back again").unwrap();
    assert_eq!(
        restore_from_topdir(&entry),
        Some(Err(RESTORE_TAKEN.to_string()))
    );
    assert_eq!(
        std::fs::read_to_string(own_trash(&d, "vol").join("files/kit.zip")).unwrap(),
        "body",
        "undo left it in the Trash"
    );
    test_set_mount_body(None);
}
