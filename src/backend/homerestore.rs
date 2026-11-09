// Undo puts a home Trash entry back in-process, the way gvfsd-trash restores it, when the entry's own record proves it.
use crate::backend::hometrash::{home_trash_root, info_name, sync_dir};
use crate::backend::trash::Entry;
use crate::paths::percent_decode_bytes;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

// The whole sentence undo refuses with when an item already has the trashed item's name.
pub(crate) const RESTORE_TAKEN: &str = "an item with that name is already there, so undo left it in the Trash";

// None means this entry is not one the home Trash proves, so gio restores it exactly as before.
pub(crate) fn restore_from_home(entry: &Entry) -> Option<Result<(), String>> {
    let root = home_trash_root()?;
    let raw = percent_decode_bytes(entry.uri.strip_prefix("trash:///")?);
    // Only a top-level entry; a name inside a trashed folder, or a dot name, stays gio's.
    if raw.is_empty() || raw == b"." || raw == b".." || raw.contains(&b'/') {
        return None;
    }
    let name = OsStr::from_bytes(&raw);
    let (files, info) = (root.join("files"), root.join("info"));
    let (kept, record) = (files.join(name), info.join(info_name(name)));
    kept.symlink_metadata().ok()?;
    if recorded_path(&std::fs::read(&record).ok()?)? != entry.original.as_os_str().as_bytes() {
        return None;
    }
    let parent = entry.original.parent()?;
    // gio recreates a missing original folder before it moves the item back.
    std::fs::create_dir_all(parent).ok()?;
    match crate::backend::renamecompat::rename_noreplace(&kept, &entry.original) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Some(Err(RESTORE_TAKEN.to_string())),
        // EXDEV or any other failure leaves the item in the Trash for gio.
        Err(_) => return None,
    }
    let _ = std::fs::remove_file(&record);
    // The item is back already, so a folder that will not sync is not a failure.
    for dir in [parent, files.as_path(), info.as_path()] {
        let _ = sync_dir(dir);
    }
    Some(Ok(()))
}

// The raw bytes a .trashinfo record's Path= names; its value is ASCII once encoded.
// Sample input: "[Trash Info]\nPath=/home/u/caf%E9.txt\nDeletionDate=2026-10-09T22:41:03\n"
pub(crate) fn recorded_path(body: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(body).ok()?;
    text.lines().find_map(|line| line.strip_prefix("Path=")).map(percent_decode_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::hometrash::{percent_encode, set_test_trash_root};
    use crate::backend::testdir::TestDir;
    use std::path::Path;

    // Sample input: uri "trash:///sub%2Ff.txt" names sub/f.txt under files/, which only gio's own restore handles.
    #[test]
    fn a_name_inside_a_trashed_folder_or_a_dot_name_is_left_to_gio() {
        let d = TestDir::new("homerestore-nested");
        let trash = d.dir("Trash");
        d.dir("Trash/files/sub");
        d.dir("Trash/info/sub");
        let record = |name: &str, original: &Path| {
            let body = format!("[Trash Info]\nPath={}\nDeletionDate=2026-10-09T00:00:00\n", percent_encode(original.as_os_str().as_bytes()));
            d.file(&format!("Trash/info/{name}"), &body)
        };
        let entry = |uri: &str, original: &Path| Entry { original: original.to_path_buf(), uri: uri.to_string(), before: Vec::new(), trashed_at: String::new() };
        set_test_trash_root(Some(trash));
        // A top-level entry restores, so the refusals below come from the guard and not from a lookup that finds nothing.
        let home = d.join("to/g.txt");
        d.file("Trash/files/g.txt", "top");
        let top = record("g.txt.trashinfo", &home);
        assert!(matches!(restore_from_home(&entry("trash:///g.txt", &home)), Some(Ok(()))), "a top-level entry restores in-process");
        assert_eq!(std::fs::read_to_string(&home).unwrap_or_default(), "top");
        assert!(!top.exists() && !d.join("Trash/files/g.txt").exists(), "the item and its record left the Trash");
        // Every refused uri has a file and a record that would match, so only the guard stops it.
        let original = d.join("refused/f.txt");
        let kept = d.file("Trash/files/sub/f.txt", "inside");
        let records = ["sub/f.txt.trashinfo", "..trashinfo", "...trashinfo", ".trashinfo"].map(|name| record(name, &original));
        for uri in ["trash:///sub/f.txt", "trash:///sub%2Ff.txt", "trash:///.", "trash:///..", "trash:///"] {
            assert!(restore_from_home(&entry(uri, &original)).is_none(), "{uri} is gio's to restore");
        }
        assert!(kept.is_file() && records.iter().all(|r| r.is_file()), "nothing moved and no record went");
        assert!(d.join("Trash/files").is_dir() && d.join("Trash/info").is_dir(), "the Trash folders stay");
        assert!(!d.join("refused").exists(), "nothing was created for a refused uri");
        set_test_trash_root(None);
    }
}
