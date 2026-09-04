// The filesystem line the status bar draws: its name and how much room is left on it. One statfs
// call per directory change, which is why this is a syscall rather than a df subprocess.
use crate::json::escape;

// The directory's own filesystem id, which the listed line carries so a client can tell a move within
// one volume from a copy across two without stat'ing anything itself. 0 when it cannot be read, and a
// client reads 0 as unknown and copies: copying where Finder would move is an annoyance, and moving
// where Finder would copy loses the original.
pub fn dev_of(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|m| m.dev()).unwrap_or(0)
}
use std::ffi::{c_char, CString};
use std::path::Path;

// The f_type values for the filesystems this box can actually mount; anything else reports its hex.
const MAGIC: &[(i64, &str)] = &[
    (0x9123683E, "btrfs"),
    (0xEF53, "ext4"),
    (0x58465342, "xfs"),
    (0x2FC12FC1, "zfs"),
    (0x01021994, "tmpfs"),
    (0x6969, "nfs"),
    (0xFF534D42, "cifs"),
    (0x65735546, "fuse"),
    (0x4D44, "vfat"),
    (0x5346544E, "ntfs"),
    (0x9FA0, "proc"),
    (0x62656572, "sysfs"),
    (0x27E0EB, "cgroup"),
    (0x794C7630, "overlay"),
    (0x52654973, "reiserfs"),
    (0x1CD1, "devpts"),
];

// struct statfs on linux x86-64: the fields this needs are f_type, f_bsize and f_bavail, and the
// rest is padding this never reads. Sizes are from man 2 statfs.
#[repr(C)]
struct StatFs {
    f_type: i64,
    f_bsize: i64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_fsid: [i32; 2],
    f_namelen: i64,
    f_frsize: i64,
    f_flags: i64,
    f_spare: [i64; 4],
}

extern "C" {
    fn statfs(path: *const c_char, buf: *mut StatFs) -> i32;
}

pub struct Info {
    pub name: String,
    // Bytes available to an unprivileged process, which is f_bavail and never f_bfree.
    pub free: u64,
}

pub fn read(path: &Path) -> Option<Info> {
    let c = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // Every field is written by the call, so the zeroed value is never read as a result.
    let mut buf: StatFs = unsafe { std::mem::zeroed() };
    if unsafe { statfs(c.as_ptr() as *const i8, &mut buf) } != 0 {
        return None;
    }
    Some(Info { name: name_for(buf.f_type), free: buf.f_bavail.saturating_mul(buf.f_bsize.max(0) as u64) })
}

// An unknown filesystem reports its own magic rather than a wrong name or an empty string.
pub fn name_for(f_type: i64) -> String {
    for (magic, name) in MAGIC {
        if *magic == f_type {
            return name.to_string();
        }
    }
    format!("0x{:x}", f_type)
}

// Sample output: {"t":"fsinfo","fs":"btrfs","free":442000000000}
pub fn fsinfo_line(info: &Option<Info>) -> String {
    match info {
        Some(i) => format!(r#"{{"t":"fsinfo","fs":"{}","free":{}}}"#, escape(&i.name), i.free),
        // An unreadable path answers an empty name, so the client draws nothing rather than a wrong number.
        None => r#"{"t":"fsinfo","fs":"","free":0}"#.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn a_known_magic_reads_back_as_its_name_and_an_unknown_one_as_hex() {
        assert_eq!(name_for(0x9123683E), "btrfs");
        assert_eq!(name_for(0xEF53), "ext4");
        assert_eq!(name_for(0x01021994), "tmpfs");
        assert_eq!(name_for(0x1234), "0x1234", "a wrong name would be worse than the number");
    }

    #[test]
    fn a_real_directory_answers_a_name_and_a_nonzero_free() {
        let d = TestDir::new("fsinfo");
        let info = read(d.path()).expect("a temp directory is on a mounted filesystem");
        assert!(!info.name.is_empty());
        assert!(info.free > 0, "a filesystem with no room left could not have held this sandbox");
    }

    #[test]
    fn a_path_that_does_not_exist_is_none_rather_than_a_wrong_number() {
        let d = TestDir::new("fsinfomissing");
        assert!(read(&d.join("never-existed")).is_none());
        assert_eq!(fsinfo_line(&None), r#"{"t":"fsinfo","fs":"","free":0}"#);
    }

    #[test]
    fn the_line_carries_the_name_and_the_free_bytes() {
        let line = fsinfo_line(&Some(Info { name: "btrfs".to_string(), free: 442_000_000_000 }));
        assert_eq!(line, r#"{"t":"fsinfo","fs":"btrfs","free":442000000000}"#);
    }
}
