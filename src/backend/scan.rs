use crate::backend::listing::Listing;
use crate::error::{from_io, FleaError};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::time::Instant;

// Phase 1 never stats: d_type is free, see AGENTS.md "Two-phase listing".
// hidden:false is the shell's own dotfile convention, matched here rather than left to the client.
pub fn scan(path: &str, hidden: bool) -> Result<(Listing, f64), FleaError> {
    let t = Instant::now();
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) => return Err(from_io("scan", path, &e)),
    };
    let mut l = Listing::new();
    // corner: unreadable entries skip, typeless ones become files, see AGENTS.md.
    // corner: a non-UTF8 name goes lossy here and then cannot be stat'd, see AGENTS.md.
    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // corner: a dot-prefixed name is dropped before any stat, so a hidden directory costs nothing when hidden is false.
        if !hidden && name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().ok();
        let index = l.len();
        l.push(&name, file_type.is_some_and(|kind| kind.is_dir()));
        l.spans[index].is_symlink = file_type.is_some_and(|kind| kind.is_symlink());
    }
    Ok((l, t.elapsed().as_secs_f64() * 1000.0))
}

// The st_mode of a path whose listing failed. The stat outlives the denial: /root answers mode
// 0o40750 to anyone while opendir on it is refused, which is what lets a denied pane draw the
// directory's own permission string instead of nothing. Zero when the stat failed too, and a real
// st_mode always carries its file-type bits, so zero can only mean "I could not look".
pub fn mode_of(path: &str) -> u32 {
    match fs::metadata(path) {
        Ok(m) => m.mode(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn reads_files_and_marks_directories() {
        let d = TestDir::new("scan-basic");
        d.file("a.txt", "");
        d.dir("sub");

        let (l, _) = scan(d.path().to_str().unwrap(), false).unwrap();
        assert_eq!(l.len(), 2);

        let mut seen_file = false;
        let mut seen_dir = false;
        for i in 0..l.len() {
            if l.name(i) == "a.txt" && !l.is_dir(i) {
                seen_file = true;
            }
            if l.name(i) == "sub" && l.is_dir(i) {
                seen_dir = true;
            }
        }
        assert!(seen_file, "expected a.txt as a file");
        assert!(seen_dir, "expected sub as a directory");
    }

    #[test]
    fn an_empty_directory_yields_an_empty_listing() {
        let d = TestDir::new("scan-empty");
        let empty = d.dir("empty");
        let (l, _) = scan(empty.to_str().unwrap(), false).unwrap();
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn a_missing_directory_is_an_error_naming_the_path() {
        let d = TestDir::new("scan-missing");
        let missing = d.join("missing");
        let path = missing.to_str().unwrap();
        let e = scan(path, false).unwrap_err();
        assert_eq!(e.where_, "scan");
        assert_eq!(e.path, path);
        assert!(!e.msg.is_empty());
    }

    #[test]
    fn a_directory_that_cannot_be_listed_still_answers_its_own_mode() {
        let d = TestDir::new("scan-mode");
        let locked = d.dir("locked");
        // Write and enter, never read: opendir is refused while stat still answers.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o300)).unwrap();

        let listed = scan(locked.to_str().unwrap(), false);
        let mode = mode_of(locked.to_str().unwrap());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(listed.is_err(), "a directory with no read bit cannot be listed");
        assert_eq!(mode & 0o777, 0o300, "the stat answered the permission bits, got {:o}", mode);
        assert_eq!(mode & 0o170000, 0o040000, "and the file-type bits say directory, got {:o}", mode);
    }

    #[test]
    fn a_path_that_cannot_be_stat_at_all_answers_zero() {
        let d = TestDir::new("scan-missing-mode");
        assert_eq!(mode_of(d.join("missing").to_str().unwrap()), 0);
    }

    #[test]
    fn a_dotfile_is_skipped_by_default_and_listed_when_hidden_is_true() {
        let d = TestDir::new("scan-hidden");
        let listing = d.dir("listing");
        d.file("listing/.dotfile", "");
        d.dir("listing/.dotdir");
        d.file("listing/plain.txt", "");

        let (visible, _) = scan(listing.to_str().unwrap(), false).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible.name(0), "plain.txt");

        let (all, _) = scan(listing.to_str().unwrap(), true).unwrap();
        assert_eq!(all.len(), 3);
        let mut names: Vec<&str> = (0..all.len()).map(|i| all.name(i)).collect();
        names.sort();
        assert_eq!(names, [".dotdir", ".dotfile", "plain.txt"]);
    }
}
