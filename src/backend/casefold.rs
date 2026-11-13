// Which folders fold the case of names, so two names that differ only in case land on one entry there.
use std::collections::HashSet;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// FS_IOC_GETFLAGS and FS_CASEFOLD_FL from linux/fs.h, the same on x86-64 and aarch64.
const FS_IOC_GETFLAGS: usize = 0x80086601;
const FS_CASEFOLD_FL: i32 = 0x40000000;
extern "C" {
    fn ioctl(fd: i32, request: usize, ...) -> i32;
}

// The FAT family folds case on the whole drive; ext4 and f2fs fold only inside a folder carrying the casefold flag.
fn folds_by_type(fstype: &str) -> Option<bool> {
    match fstype.to_ascii_lowercase().as_str() {
        "vfat" | "msdos" | "exfat" | "fat" | "fat12" | "fat16" | "fat32" => Some(true),
        "ext4" | "f2fs" => None,
        _ => Some(false),
    }
}

// Sample input: a folder on a vfat stick trues, a +F folder on ext4 trues, a plain ext4 or tmpfs folder falses.
pub fn folds(dir: &Path) -> bool {
    #[cfg(test)]
    if STOOD
        .lock()
        .unwrap()
        .iter()
        .any(|root| dir.starts_with(root))
    {
        return true;
    }
    match super::fs_type_of(dir).as_deref().map(folds_by_type) {
        Some(Some(known)) => known,
        Some(None) => casefold_flag(dir),
        None => false,
    }
}

// A folder that cannot be opened or asked answers false, which is how every other filesystem reads.
fn casefold_flag(dir: &Path) -> bool {
    let Ok(file) = std::fs::File::open(dir) else {
        return false;
    };
    let mut flags: i32 = 0;
    // The kernel writes one int through the pointer; the descriptor stays open for the call.
    (unsafe { ioctl(file.as_raw_fd(), FS_IOC_GETFLAGS, &mut flags as *mut i32) }) == 0
        && flags & FS_CASEFOLD_FL != 0
}

// The key two names share in a folder: the name itself, or its lower case where the folder folds case.
pub fn key(name: &str, folds: bool) -> String {
    if folds {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

// The names a batch has already claimed in one folder, under that folder's case rule.
pub(crate) struct Claimed {
    folds: bool,
    names: HashSet<String>,
}

impl Claimed {
    pub(crate) fn new(dir: &Path) -> Claimed {
        Claimed {
            folds: folds(dir),
            names: HashSet::new(),
        }
    }
    // True when an earlier name of the batch already took this one.
    pub(crate) fn taken(&mut self, name: &str) -> bool {
        !self.names.insert(key(name, self.folds))
    }
}

// The path of the entry the kernel finds at path, spelled as the folder holds it: a.txt for a lookup of A.txt on a FAT stick.
pub fn spelled(path: &Path) -> PathBuf {
    let (Some(parent), Some(leaf)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
    else {
        return path.to_path_buf();
    };
    let Ok(meta) = path.symlink_metadata() else {
        return path.to_path_buf();
    };
    if !folds(parent) {
        return path.to_path_buf();
    }
    let Ok(entries) = std::fs::read_dir(parent) else {
        return path.to_path_buf();
    };
    let mut found = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_str() == Some(leaf) {
            return path.to_path_buf();
        }
        // readdir's inode on FAT is made up for an uncached entry, so each case twin is asked with its own lstat.
        if found.is_none()
            && name
                .to_str()
                .is_some_and(|name| key(name, true) == key(leaf, true))
        {
            let candidate = parent.join(&name);
            if candidate
                .symlink_metadata()
                .is_ok_and(|other| other.dev() == meta.dev() && other.ino() == meta.ino())
            {
                found = Some(candidate);
            }
        }
    }
    found.unwrap_or_else(|| path.to_path_buf())
}

// Roots read as case-folding folders on every thread, because the question runs on a thread of its own.
#[cfg(test)]
static STOOD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

// Sample input: test_stand_folding(root, true) makes every folder under root read as folding case; false ends it.
#[cfg(test)]
pub fn test_stand_folding(root: &Path, on: bool) {
    let mut roots = STOOD.lock().unwrap();
    roots.retain(|stood| stood != root);
    if on {
        roots.push(root.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn the_flag_request_matches_the_kernel_headers() {
        assert_eq!(FS_IOC_GETFLAGS as u64, libc::FS_IOC_GETFLAGS);
    }

    #[test]
    fn fat_folds_ext4_asks_the_folder_and_the_rest_do_not() {
        for fs in ["vfat", "exfat", "msdos", "FAT32"] {
            assert_eq!(folds_by_type(fs), Some(true), "{fs}");
        }
        assert_eq!(folds_by_type("ext4"), None);
        assert_eq!(folds_by_type("f2fs"), None);
        for fs in ["btrfs", "tmpfs", "ntfs3", "xfs"] {
            assert_eq!(folds_by_type(fs), Some(false), "{fs}");
        }
    }

    #[test]
    fn a_plain_folder_does_not_fold_and_a_stood_one_does() {
        let sandbox = TestDir::new("casefold-stood");
        assert!(!folds(sandbox.path()), "a test sandbox is case-sensitive");
        test_stand_folding(sandbox.path(), true);
        assert!(folds(&sandbox.join("inner")));
        test_stand_folding(sandbox.path(), false);
        assert!(!folds(sandbox.path()));
    }

    #[test]
    fn claimed_names_collide_by_the_folders_rule() {
        let mut exact = Claimed {
            folds: false,
            names: HashSet::new(),
        };
        assert!(!exact.taken("a.txt"));
        assert!(!exact.taken("A.txt"), "a case-sensitive folder holds both");
        assert!(exact.taken("a.txt"));
        let mut folding = Claimed {
            folds: true,
            names: HashSet::new(),
        };
        assert!(!folding.taken("a.txt"));
        assert!(folding.taken("A.TXT"), "a folding folder holds one of them");
    }

    #[test]
    fn spelled_keeps_a_name_the_folder_holds_exactly() {
        let sandbox = TestDir::new("casefold-spelled");
        let file = sandbox.file("a.txt", "x");
        test_stand_folding(sandbox.path(), true);
        assert_eq!(spelled(&file), file);
        // A missing name has no entry to spell, so it stays as asked.
        assert_eq!(spelled(&sandbox.join("A.txt")), sandbox.join("A.txt"));
        test_stand_folding(sandbox.path(), false);
    }
}

#[cfg(test)]
#[path = "casefold_unicode_tests.rs"]
mod casefold_unicode_tests;
