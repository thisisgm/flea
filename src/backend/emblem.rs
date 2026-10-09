use std::ffi::{c_char, CString};
use std::path::Path;

// A row's sync status, read from the extended attribute a sync tool sets on the file:
// user.sync.status = synced, syncing, partial, conflict or excluded (the Omarchy Storage Drives
// plugin writes it after each rclone bisync). A neutral name, so Flea never has to know which tool synced.
// One lgetxattr per visible row, the same viewport scope as the stat beside it, see AGENTS.md rule 1.
const ATTR: &core::ffi::CStr = c"user.sync.status";

extern "C" {
    fn lgetxattr(path: *const c_char, name: *const c_char, value: *mut c_char, size: usize) -> isize;
}

// rclone bisync renames a conflict loser with a ".conflictN" part, before or after the extension.
// Sample input: "notes.txt.conflict1" and "notes.conflict2.txt" are conflicts, "my.conflicts.txt" is not.
fn is_conflict_name(name: &str) -> bool {
    name.split('.').skip(1).any(|part| part.strip_prefix("conflict").is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
}

// Sample input: a file tagged "Synced\n" answers "synced"; an untagged file answers "".
pub fn read_emblem(base: &Path, name: &str) -> String {
    if is_conflict_name(name) {
        return "conflict".to_string();
    }
    let Ok(c_path) = CString::new(base.join(name).into_os_string().into_encoded_bytes()) else {
        return String::new();
    };
    let mut buf = [0u8; 32];
    let ret = unsafe { lgetxattr(c_path.as_ptr(), ATTR.as_ptr(), buf.as_mut_ptr() as *mut c_char, buf.len()) };
    if ret <= 0 {
        return String::new();
    }
    let value = std::str::from_utf8(&buf[..ret as usize]).unwrap_or("").trim().to_ascii_lowercase();
    // Only the words the badge can draw travel to the UI, so a stray value never reaches QML.
    match value.as_str() {
        "synced" | "syncing" | "partial" | "conflict" | "excluded" => value,
        "error" => "conflict".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    extern "C" {
        fn lsetxattr(path: *const c_char, name: *const c_char, value: *const c_char, size: usize, flags: i32) -> i32;
    }

    #[test]
    fn conflict_names_follow_rclone_and_nothing_else() {
        assert!(is_conflict_name("notes.txt.conflict1"));
        assert!(is_conflict_name("notes.conflict2.txt"));
        assert!(is_conflict_name("notes (Case Conflict).conflict"));
        assert!(!is_conflict_name("my.conflicts.txt"));
        assert!(!is_conflict_name("conflict1"));
        let d = TestDir::new("emblem-names");
        assert_eq!(read_emblem(d.path(), "notes.txt.conflict1"), "conflict");
        assert_eq!(read_emblem(d.path(), "missing.txt"), "");
    }

    #[test]
    fn the_attribute_is_read_and_only_known_words_pass() {
        let d = TestDir::new("emblem-xattr");
        let tag = |name: &str, value: &str| {
            std::fs::write(d.path().join(name), b"x").unwrap();
            let p = CString::new(d.path().join(name).into_os_string().into_encoded_bytes()).unwrap();
            unsafe { lsetxattr(p.as_ptr(), ATTR.as_ptr(), value.as_ptr() as *const c_char, value.len(), 0) }
        };
        // corner: a filesystem without user xattrs cannot hold the tag, so there is nothing to read back.
        if tag("a.txt", "Synced\n") != 0 {
            return;
        }
        tag("b.txt", "error");
        tag("c.txt", "<b>bold</b>");
        std::fs::write(d.path().join("d.txt"), b"x").unwrap();
        let read = |n: &str| read_emblem(d.path(), n);
        assert_eq!((read("a.txt"), read("b.txt"), read("c.txt"), read("d.txt")), ("synced".into(), "conflict".into(), String::new(), String::new()));
    }
}
