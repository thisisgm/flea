// The topdir Trash on mounts gio refuses: GLib marks any mount whose mountinfo root is not "/" system-internal (btrfs subvolumes and bind mounts included, GLib issue 1271) and g_file_trash refuses it outright, so flea performs the freedesktop topdir move itself, the same rename gio would run on a mount it accepts.
use crate::backend::homerestore::{recorded_path, RESTORE_TAKEN};
use crate::backend::hometrash::{info_name, percent_encode, reserve, sync_dir, trash_dirs};
use crate::backend::mountinfo;
use crate::backend::trash::Entry;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

// The topdir Trash directory on a mount gio refuses: an administrator's sticky .Trash with .Trash/<uid> beneath it wins, else this user's own .Trash-<uid>, the order gio itself tries; None leaves the item to gio.
fn trash_dir_at(topdir: &Path) -> Option<PathBuf> {
    let uid = crate::backend::manifestdir::current_uid();
    let admin = topdir.join(".Trash");
    // The spec's administrator Trash: a directory, never a link, carrying the sticky bit.
    let sticky = admin.symlink_metadata().is_ok_and(|meta| {
        meta.is_dir() && !meta.file_type().is_symlink() && meta.permissions().mode() & 0o1000 != 0
    });
    if sticky {
        let shared = admin.join(uid.to_string());
        if let Ok(meta) = shared.symlink_metadata() {
            if meta.is_dir() && !meta.file_type().is_symlink() && meta.uid() == uid {
                return Some(shared);
            }
        } else if std::fs::DirBuilder::new().mode(0o700).create(&shared).is_ok() {
            return Some(shared);
        }
    }
    let own = topdir.join(format!(".Trash-{uid}"));
    match own.symlink_metadata() {
        // A Trash this user owns and that is a plain directory is the spec's own fallback; a link is never followed.
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() && meta.uid() == uid => Some(own),
        Ok(_) => None,
        // Absent: this user may create it beside the mount root, as gio does; a topdir without room for it stays gio's.
        Err(_) => std::fs::DirBuilder::new().mode(0o700).create(&own).ok().map(|()| own),
    }
}

// The mount point of a path gio's own rule refuses (a mountinfo root that is not "/"), with the home filesystem's items left to hometrash's rename; None means gio.
fn topdir_root_for(original: &Path) -> Option<PathBuf> {
    let name = original.file_name()?;
    if !original.is_absolute() || matches!(name.as_bytes(), b"." | b"..") {
        return None;
    }
    if crate::backend::hometrash::renames_to_home(original) {
        return None;
    }
    let body = mount_body();
    let (mount, root) = mountinfo::mount_point_and_root_in(original, &body)?;
    (root != Path::new("/")).then_some(mount)
}

// A mount table the tests decide, so a sandbox names its own mount without touching the process's.
#[cfg(test)]
thread_local! {
    static MOUNT_BODY: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn test_set_mount_body(body: Option<String>) {
    MOUNT_BODY.with(|slot| *slot.borrow_mut() = body);
}
fn mount_body() -> String {
    #[cfg(test)]
    if let Some(body) = MOUNT_BODY.with(|slot| slot.borrow().clone()) {
        return body;
    }
    mountinfo::read_body()
}

// Whether rename_to_topdir would try this item, so the batch knows which paths still need gio. The probe creates the Trash directory gio would create on the mount gio refuses.
pub(crate) fn renames_to_topdir(original: &Path) -> bool {
    topdir_root_for(original).is_some_and(|top| trash_dir_at(&top).is_some())
}

// Moves one item into the topdir Trash of its own mount, the same rename gio performs on a mount it accepts; None means nothing moved and gio still runs.
pub(crate) fn rename_to_topdir(original: &Path, unsynced: &mut std::collections::BTreeSet<PathBuf>, durable: &mut Option<bool>) -> Option<Result<Entry, String>> {
    let root = trash_dir_at(&topdir_root_for(original)?)?;
    let (files, info) = trash_dirs(&root).ok()?;
    let name = original.file_name()?;
    // A drive Flea confirms syncs each record before its item moves, the same rule the home rename follows.
    let confirm = *durable.get_or_insert_with(|| original.parent().is_some_and(crate::backend::durable::dest_is_durable));
    let (candidate, info_path) = reserve(&files, &info, original, name, confirm).ok()?;
    if let Err(e) = crate::backend::renamecompat::rename_noreplace(original, &files.join(&candidate)) {
        let _ = std::fs::remove_file(&info_path);
        // An item gone before its move is the Issue 88 refusal; any other failure keeps gio.
        let gone = e.kind() == std::io::ErrorKind::NotFound && original.symlink_metadata().is_err();
        return gone.then(|| Err(crate::backend::collide::TRASH_REFUSED.to_string()));
    }
    unsynced.extend([files, info].into_iter().chain(original.parent().map(Path::to_path_buf)));
    Some(Ok(Entry { original: original.to_path_buf(), uri: format!("trash:///{}", percent_encode(candidate.as_bytes())), before: Vec::new(), trashed_at: String::new() }))
}

// Undo puts a topdir Trash entry back in-process the way homerestore does, when the entry's own record proves it; None leaves the item for gio.
pub(crate) fn restore_from_topdir(entry: &Entry) -> Option<Result<(), String>> {
    let raw = crate::paths::percent_decode_bytes(entry.uri.strip_prefix("trash:///")?);
    // Only a top-level entry; a name inside a trashed folder, or a dot name, stays gio's.
    if raw.is_empty() || raw == b"." || raw == b".." || raw.contains(&b'/') {
        return None;
    }
    let root = trash_dir_at(&topdir_root_for(&entry.original)?)?;
    let name = OsStr::from_bytes(&raw);
    let (files, info) = (root.join("files"), root.join("info"));
    let (kept, record) = (files.join(name), info.join(info_name(name)));
    kept.symlink_metadata().ok()?;
    if recorded_path(&std::fs::read(&record).ok()?)? != entry.original.as_os_str().as_bytes() {
        return None;
    }
    let parent = entry.original.parent()?;
    std::fs::create_dir_all(parent).ok()?;
    match crate::backend::renamecompat::rename_noreplace(&kept, &entry.original) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Some(Err(RESTORE_TAKEN.to_string())),
        Err(_) => return None,
    }
    let _ = std::fs::remove_file(&record);
    // The item is back already, so a folder that will not sync is not a failure.
    for dir in [parent, files.as_path(), info.as_path()] {
        let _ = sync_dir(dir);
    }
    Some(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::hometrash::set_test_trash_root;
    use crate::backend::testdir::TestDir;

    // The mount body naming one subvolume mount inside the sandbox, so every probe and rename stays in it.
    fn subvol_body(d: &TestDir) -> String {
        format!("28 22 259:1 /@data {} rw - btrfs /dev/nvme0n1p1 rw", d.join("vol").display())
    }

    // The sandbox's own Trash directory name, the uid the whole module names its directories by.
    fn own_trash(d: &TestDir, under: &str) -> PathBuf {
        d.join(under).join(format!(".Trash-{}", crate::backend::manifestdir::current_uid()))
    }

    #[test]
    fn mount_point_and_root_reads_the_root_field_and_unescapes_it() {
        let body = "28 22 259:1 /@data /mnt/data rw - btrfs /dev/nvme0n1p1 rw\n29 22 259:1 / /mnt/whole rw - ext4 /dev/sdb1 rw\n";
        assert_eq!(mountinfo::mount_point_and_root_in(Path::new("/mnt/data/f.txt"), body), Some((PathBuf::from("/mnt/data"), PathBuf::from("/@data"))));
        assert_eq!(mountinfo::mount_point_and_root_in(Path::new("/mnt/whole/f"), body), Some((PathBuf::from("/mnt/whole"), PathBuf::from("/"))));
        assert_eq!(mountinfo::mount_point_and_root_in(Path::new("/elsewhere/f"), body), None);
        let escaped = "30 22 0:9 /my\\040root /media/d rw - ext4 /dev/sdc1 rw\n";
        assert_eq!(mountinfo::mount_point_and_root_in(Path::new("/media/d/f"), escaped).map(|(_, root)| root), Some(PathBuf::from("/my root")));
    }

    #[test]
    fn a_root_that_is_not_slash_is_taken_and_a_whole_mount_is_left_to_gio() {
        let d = TestDir::new("topdir-eligible");
        d.dir("vol");
        let file = d.file("vol/a.txt", "x");
        assert!(!renames_to_topdir(&file), "no mount table names the sandbox, so gio keeps it");
        test_set_mount_body(Some(subvol_body(&d)));
        assert!(renames_to_topdir(&file), "a subvolume mount renames into its own topdir Trash");
        assert!(own_trash(&d, "vol").is_dir(), "the probe created the Trash gio would have");
        test_set_mount_body(Some("29 22 259:1 / /mnt/whole rw - ext4 /dev/sdb1 rw\n".to_string()));
        assert!(!renames_to_topdir(Path::new("/mnt/whole/f.txt")), "a whole-filesystem mount is gio's own");
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
        assert!(!renames_to_topdir(&file), "the home rename wins the partition first");
        set_test_trash_root(None);
        test_set_mount_body(None);
    }

    #[test]
    fn a_rename_moves_the_item_and_writes_its_record() {
        let d = TestDir::new("topdir-rename");
        d.dir("vol");
        let file = d.file("vol/kit.zip", "body");
        test_set_mount_body(Some(subvol_body(&d)));
        let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
        let entry = match rename_to_topdir(&file, &mut unsynced, &mut durable) {
            Some(Ok(entry)) => entry,
            other => panic!("the rename ran: {other:?}"),
        };
        let trash = own_trash(&d, "vol");
        assert!(!file.exists(), "the item left its place");
        assert_eq!(std::fs::read_to_string(trash.join("files/kit.zip")).unwrap(), "body");
        let record = std::fs::read_to_string(trash.join("info/kit.zip.trashinfo")).unwrap();
        assert!(record.starts_with("[Trash Info]\nPath="), "{record}");
        assert!(record.contains(&percent_encode(file.as_os_str().as_bytes())), "the record names the original: {record}");
        assert!(record.contains("\nDeletionDate="), "{record}");
        assert_eq!((entry.original, entry.uri.as_str()), (file, "trash:///kit.zip"));
        test_set_mount_body(None);
    }

    #[test]
    fn a_taken_trash_name_takes_the_next_suffix() {
        let d = TestDir::new("topdir-collision");
        d.dir("vol");
        let file = d.file("vol/kit.zip", "new");
        test_set_mount_body(Some(subvol_body(&d)));
        let trash = own_trash(&d, "vol");
        std::fs::create_dir_all(trash.join("files")).unwrap();
        std::fs::write(trash.join("files/kit.zip"), "old").unwrap();
        let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
        let entry = match rename_to_topdir(&file, &mut unsynced, &mut durable) {
            Some(Ok(entry)) => entry,
            other => panic!("the rename ran: {other:?}"),
        };
        assert_eq!(std::fs::read_to_string(trash.join("files/kit.zip")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(trash.join("files/kit.2.zip")).unwrap(), "new");
        assert_eq!(entry.uri, "trash:///kit.2.zip");
        test_set_mount_body(None);
    }

    #[test]
    fn an_administrators_sticky_trash_wins_and_an_unsticky_one_does_not() {
        let d = TestDir::new("topdir-admin");
        let admin = d.dir("vol/.Trash");
        std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o1777)).unwrap();
        test_set_mount_body(Some(subvol_body(&d)));
        let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
        let first = d.file("vol/a.txt", "x");
        assert!(matches!(rename_to_topdir(&first, &mut unsynced, &mut durable), Some(Ok(_))));
        let shared = d.join("vol/.Trash").join(crate::backend::manifestdir::current_uid().to_string());
        assert!(shared.join("files/a.txt").is_file(), "the sticky administrator Trash holds it");
        std::fs::set_permissions(&admin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let second = d.file("vol/b.txt", "y");
        assert!(matches!(rename_to_topdir(&second, &mut unsynced, &mut durable), Some(Ok(_))));
        assert!(own_trash(&d, "vol").join("files/b.txt").is_file(), "an unsticky .Trash is not trusted");
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
        assert!(!renames_to_topdir(&file), "a linked Trash is gio's problem");
        test_set_mount_body(None);
    }

    #[test]
    fn undo_puts_the_item_back_on_the_records_proof() {
        let d = TestDir::new("topdir-restore");
        d.dir("vol");
        let file = d.file("vol/kit.zip", "body");
        test_set_mount_body(Some(subvol_body(&d)));
        let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
        let entry = match rename_to_topdir(&file, &mut unsynced, &mut durable) {
            Some(Ok(entry)) => entry,
            other => panic!("the rename ran: {other:?}"),
        };
        assert!(matches!(restore_from_topdir(&entry), Some(Ok(()))), "the record proves the entry");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "body");
        let trash = own_trash(&d, "vol");
        assert!(!trash.join("files/kit.zip").exists() && !trash.join("info/kit.zip.trashinfo").exists(), "the item and its record left the Trash");
        // A record naming another path proves nothing, so the entry stays gio's.
        let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
        let entry = match rename_to_topdir(&file, &mut unsynced, &mut durable) {
            Some(Ok(entry)) => entry,
            other => panic!("the rename ran: {other:?}"),
        };
        let elsewhere = Entry { original: d.join("vol/other.zip"), ..entry };
        assert!(restore_from_topdir(&elsewhere).is_none(), "a record for another path is not proof");
        assert!(!file.exists(), "the trashed item stayed in the Trash");
        test_set_mount_body(None);
    }

    #[test]
    fn a_taken_original_refuses_and_keeps_the_item() {
        let d = TestDir::new("topdir-restore-taken");
        d.dir("vol");
        let file = d.file("vol/kit.zip", "body");
        test_set_mount_body(Some(subvol_body(&d)));
        let (mut unsynced, mut durable) = (std::collections::BTreeSet::new(), None);
        let entry = match rename_to_topdir(&file, &mut unsynced, &mut durable) {
            Some(Ok(entry)) => entry,
            other => panic!("the rename ran: {other:?}"),
        };
        std::fs::write(&file, "back again").unwrap();
        assert_eq!(restore_from_topdir(&entry), Some(Err(RESTORE_TAKEN.to_string())));
        assert_eq!(std::fs::read_to_string(own_trash(&d, "vol").join("files/kit.zip")).unwrap(), "body", "undo left it in the Trash");
        test_set_mount_body(None);
    }
}
