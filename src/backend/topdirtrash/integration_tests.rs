use super::*;
use crate::backend::{
    testdir::TestDir,
    trashdelete::Reviewed,
    trashmanifest::{Cancellation, Manifest},
};
use std::os::unix::fs::{symlink, PermissionsExt};

struct Sandbox(TestDir);
impl Sandbox {
    fn new() -> Self {
        let d = TestDir::new("topdir-integration");
        d.dir("vol");
        test_set_mount_body(Some(format!(
            "28 22 259:1 /sub {} rw - btrfs /dev/test rw\n",
            d.join("vol").display()
        )));
        Self(d)
    }
    fn trash(&self, name: &str) -> Entry {
        let path = self.0.file(&format!("vol/{name}"), "mount bytes");
        rename_to_topdir(&mut Batch::read(), &path, &mut Default::default())
            .unwrap()
            .unwrap()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        test_set_mount_body(None);
    }
}

#[test]
fn a_symlinked_or_unsticky_admin_trash_uses_the_private_fallback() {
    let s = Sandbox::new();
    let admin = s.0.dir("elsewhere");
    std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o1777)).unwrap();
    symlink(&admin, s.0.join("vol/.Trash")).unwrap();
    let first = s.trash("one");
    let record = resolve(&first.uri).unwrap();
    assert!(record.path.starts_with(s.0.join("vol").join(format!(
        ".Trash-{}",
        crate::backend::manifestdir::current_uid()
    ))));
    assert!(std::fs::read_dir(admin).unwrap().next().is_none());
}

#[test]
fn an_unsafe_shared_layout_falls_back_and_never_writes_through_a_link() {
    let s = Sandbox::new();
    let admin = s.0.dir("vol/.Trash");
    std::fs::set_permissions(admin, std::fs::Permissions::from_mode(0o1777)).unwrap();
    let shared =
        s.0.join("vol/.Trash")
            .join(crate::backend::manifestdir::current_uid().to_string());
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&shared)
        .unwrap();
    let elsewhere = s.0.dir("elsewhere");
    symlink(&elsewhere, shared.join("files")).unwrap();
    let entry = s.trash("one");
    assert!(!resolve(&entry.uri).unwrap().path.starts_with(shared));
    assert!(std::fs::read_dir(elsewhere).unwrap().next().is_none());
}

#[test]
fn relative_and_absolute_records_restore_but_escaping_records_never_list() {
    let s = Sandbox::new();
    for (name, value) in [
        ("relative", "relative".to_string()),
        ("absolute", s.0.join("vol/absolute").display().to_string()),
    ] {
        let entry = s.trash(name);
        let location = locations::known().pop().unwrap();
        let info = location.root.join("info").join(info_name(OsStr::new(name)));
        std::fs::write(
            &info,
            format!("[Trash Info]\nPath={value}\nDeletionDate=2026-11-10T00:00:00\n"),
        )
        .unwrap();
        assert_eq!(resolve(&entry.uri).unwrap().entry.original, entry.original);
        assert_eq!(restore_from_topdir(&entry), Some(Ok(())));
        assert_eq!(
            std::fs::read_to_string(entry.original).unwrap(),
            "mount bytes"
        );
    }
    let entry = s.trash("escape");
    let location = locations::known().pop().unwrap();
    let info = location.root.join("info/escape.trashinfo");
    for value in ["../outside", "/outside"] {
        std::fs::write(&info, format!("[Trash Info]\nPath={value}\n")).unwrap();
        assert!(resolve(&entry.uri).is_none());
        assert!(list().is_empty());
    }
    assert!(location.root.join("files/escape").is_file());
}

#[test]
fn both_trash_directories_and_two_volumes_keep_distinct_identities() {
    let s = Sandbox::new();
    let private = s.trash("same.txt");
    let admin = s.0.dir("vol/.Trash");
    std::fs::set_permissions(admin, std::fs::Permissions::from_mode(0o1777)).unwrap();
    let shared = s.trash("same.txt");
    s.0.dir("other");
    test_set_mount_body(Some(format!(
        "1 0 1:1 /sub {} rw - btrfs /dev/a rw\n2 0 2:1 /sub {} rw - btrfs /dev/b rw\n",
        s.0.join("vol").display(),
        s.0.join("other").display()
    )));
    let file = s.0.file("other/same.txt", "other bytes");
    let other = rename_to_topdir(&mut Batch::read(), &file, &mut Default::default())
        .unwrap()
        .unwrap();
    let entries = list();
    assert_eq!(entries.len(), 3);
    let uris: std::collections::BTreeSet<_> =
        entries.iter().map(|record| &record.entry.uri).collect();
    assert_eq!(uris.len(), 3);
    for entry in [private, shared, other] {
        assert_eq!(restore_from_topdir(&entry), Some(Ok(())));
        // The next entry with the same original may only restore after the first is moved aside.
        std::fs::rename(
            &entry.original,
            s.0.join(&format!(
                "saved-{}",
                uris.iter().position(|uri| **uri == entry.uri).unwrap()
            )),
        )
        .unwrap();
    }
}

#[test]
fn empty_uses_the_existing_reviewed_deletion_and_preserves_new_arrivals() {
    let s = Sandbox::new();
    let entry = s.trash("one");
    let record = resolve(&entry.uri).unwrap();
    let meta = record.path.symlink_metadata().unwrap();
    let mut review = Reviewed::inspect(
        record.path.clone(),
        &format!("l{}:{}", meta.dev(), meta.ino()),
    )
    .unwrap();
    let mut manifest = Manifest::new(s.0.path()).unwrap();
    review
        .snapshot(&mut manifest, s.0.path(), &Cancellation::default())
        .unwrap();
    let later = s.trash("two");
    review.delete(&s.0.dir("recovery")).unwrap();
    assert!(resolve(&entry.uri).is_none());
    assert!(resolve(&later.uri).is_some());
    assert!(record
        .path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("info/one.trashinfo")
        .symlink_metadata()
        .is_err());
}

#[test]
fn no_safe_trash_refuses_without_copying_or_changing_the_source() {
    let s = Sandbox::new();
    let target = s.0.dir("elsewhere");
    let own = s.0.join("vol").join(format!(
        ".Trash-{}",
        crate::backend::manifestdir::current_uid()
    ));
    symlink(&target, own).unwrap();
    let file = s.0.file("vol/kept", "kept bytes");
    assert!(rename_to_topdir(&mut Batch::read(), &file, &mut Default::default()).is_none());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "kept bytes");
    assert!(std::fs::read_dir(target).unwrap().next().is_none());
}

#[test]
fn a_failed_home_rename_on_the_same_device_still_tries_the_mount_trash() {
    let s = Sandbox::new();
    let home = s.0.dir("home-trash");
    s.0.file("home-trash/files", "blocks the home layout");
    crate::backend::hometrash::set_test_trash_root(Some(home));
    let source = s.0.file("vol/home-fallback", "kept bytes");
    assert!(crate::backend::hometrash::renames_to_home(&source));
    let (entries, failed, reason) = crate::backend::trash::trash(std::slice::from_ref(&source));
    assert_eq!((entries.len(), failed, reason), (1, 0, String::new()));
    assert!(!source.exists());
    assert!(resolve(&entries[0].uri).is_some());
    crate::backend::hometrash::set_test_trash_root(None);
}
