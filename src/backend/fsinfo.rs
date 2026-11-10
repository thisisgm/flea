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
    (0xFE534D42, "smb2"),
    (0x01021997, "9p"),
    (0x00C36400, "ceph"),
    (0x2011BAB0, "exfat"),
    (0x65735546, "fuse"),
    (0x4D44, "vfat"),
    (0x5346544E, "ntfs"),
    (0x7366746E, "ntfs3"),
    (0x482B, "hfsplus"),
    (0xF2F52010, "f2fs"),
    (0x9660, "iso9660"),
    (0x15013346, "udf"),
    (0x9FA0, "proc"),
    (0x62656572, "sysfs"),
    (0x27E0EB, "cgroup"),
    (0x794C7630, "overlay"),
    (0x52654973, "reiserfs"),
    (0x1CD1, "devpts"),
];

// struct statfs on linux, byte-identical on x86-64 and aarch64 (120 bytes, f_bavail at 32): the
// fields this needs are f_type, f_bsize and f_bavail, and the rest is padding this never reads.
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

#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub name: String,
    // Bytes available to an unprivileged process, which is f_bavail and never f_bfree.
    pub free: u64,
    // Total blocks from statfs, so a zero-block report reads as unknown room rather than no room.
    pub blocks: u64,
}

pub fn read(path: &Path) -> Option<Info> {
    read_with_magic(path).0
}

// Sample input: a temp dir answers (Some, Some(local magic)); a missing path answers (None, None).
pub fn read_with_magic(path: &Path) -> (Option<Info>, Option<i64>) {
    let c = match CString::new(path.as_os_str().as_encoded_bytes()) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };
    // Every field is written by the call, so the zeroed value is never read as a result.
    let mut buf: StatFs = unsafe { std::mem::zeroed() };
    #[cfg(test)]
    STATFS_CALLS.with(|n| n.set(n.get() + 1));
    if unsafe { statfs(c.as_ptr(), &mut buf) } != 0 {
        return (None, None);
    }
    let info = Info { name: name_for(buf.f_type), free: buf.f_bavail.saturating_mul(buf.f_bsize.max(0) as u64), blocks: buf.f_blocks };
    (Some(info), Some(buf.f_type))
}

#[cfg(test)]
thread_local! {
    static STATFS_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// A test counts statfs calls on its own thread; production never reads this.
#[cfg(test)]
pub fn test_reset_statfs() {
    STATFS_CALLS.with(|n| n.set(0));
}

// The calls since the last reset on this thread, so a test pins one statfs for both halves.
#[cfg(test)]
pub fn statfs_calls() -> usize {
    STATFS_CALLS.with(|n| n.get())
}

// Raw f_type for the class decision to catch a network mount under an fstype name fstype_is_network does not know.
pub fn magic_of(path: &Path) -> Option<i64> {
    read_with_magic(path).1
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

// Sample output: {"t":"fsinfo","fs":"btrfs","free":442000000000,"path":"/home/gm","class":"","tagable":true}; path names the dir, class its extclass word, tagable the xattr probe.
// A system mountpoint holding the dir adds "system":true, sent only then so every other line reads as it always did.
pub fn fsinfo_line(info: &Option<Info>, path: &str, class: &str, tagable: bool, system: bool) -> String {
    let mut line = match info {
        // A zero-block statfs, as gvfsd-fuse answers for a share with no size, sends free -1: unknown room, never a full drive.
        Some(i) => {
            let free = if i.blocks == 0 { "-1".to_string() } else { i.free.to_string() };
            format!(r#"{{"t":"fsinfo","fs":"{}","free":{},"path":"{}","class":"{}","tagable":{}}}"#, escape(&i.name), free, escape(path), escape(class), tagable)
        }
        // An unreadable path answers an empty name, so the client draws nothing rather than a wrong number.
        None => format!(r#"{{"t":"fsinfo","fs":"","free":0,"path":"{}","class":"{}","tagable":{}}}"#, escape(path), escape(class), tagable),
    };
    if system {
        line.pop();
        line.push_str(r#","system":true}"#);
    }
    line
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
    fn network_and_removable_magics_read_back_as_names() {
        assert_eq!(name_for(0xFE534D42), "smb2");
        assert_eq!(name_for(0x01021997), "9p");
        assert_eq!(name_for(0x00C36400), "ceph");
        assert_eq!(name_for(0x2011BAB0), "exfat");
    }

    #[test]
    fn usb_and_optical_magics_read_back_as_names() {
        assert_eq!(name_for(0x7366746E), "ntfs3");
        assert_eq!(name_for(0x482B), "hfsplus");
        assert_eq!(name_for(0xF2F52010), "f2fs");
        assert_eq!(name_for(0x9660), "iso9660");
        assert_eq!(name_for(0x15013346), "udf");
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
        assert_eq!(fsinfo_line(&None, "/gone", "", true, false), r#"{"t":"fsinfo","fs":"","free":0,"path":"/gone","class":"","tagable":true}"#);
    }

    #[test]
    fn the_line_carries_the_name_the_free_bytes_and_the_directory_they_are_of() {
        let line = fsinfo_line(&Some(Info { name: "btrfs".to_string(), free: 442_000_000_000, blocks: 100 }), "/home/gm", "", true, false);
        assert_eq!(line, r#"{"t":"fsinfo","fs":"btrfs","free":442000000000,"path":"/home/gm","class":"","tagable":true}"#);
    }

    #[test]
    fn a_filesystem_with_no_blocks_sends_unknown_room_rather_than_zero() {
        let line = fsinfo_line(&Some(Info { name: "fuse".to_string(), free: 0, blocks: 0 }), "/run/user/1000/gvfs/dav:host=h", "network", false, false);
        assert_eq!(line, r#"{"t":"fsinfo","fs":"fuse","free":-1,"path":"/run/user/1000/gvfs/dav:host=h","class":"network","tagable":false}"#);
    }

    #[test]
    fn the_line_names_the_directory_class_beside_its_figures() {
        let line = fsinfo_line(&Some(Info { name: "cifs".to_string(), free: 7, blocks: 100 }), "/media/nas", "network", false, false);
        assert_eq!(line, r#"{"t":"fsinfo","fs":"cifs","free":7,"path":"/media/nas","class":"network","tagable":false}"#);
        let line = fsinfo_line(&None, "/gone", "usb", true, false);
        assert_eq!(line, r#"{"t":"fsinfo","fs":"","free":0,"path":"/gone","class":"usb","tagable":true}"#);
    }

    #[test]
    fn a_system_mountpoint_adds_its_flag_and_no_other_line_does() {
        let info = Some(Info { name: "btrfs".to_string(), free: 7, blocks: 100 });
        assert_eq!(fsinfo_line(&info, "/home/gm", "", false, true), r#"{"t":"fsinfo","fs":"btrfs","free":7,"path":"/home/gm","class":"","tagable":false,"system":true}"#);
        assert_eq!(fsinfo_line(&None, "/gone", "", false, true), r#"{"t":"fsinfo","fs":"","free":0,"path":"/gone","class":"","tagable":false,"system":true}"#);
        assert!(!fsinfo_line(&info, "/mnt/data", "", false, false).contains("system"), "the flag rides only when true");
    }

    // The base is a real path, so a quote or a backslash in it has to leave as JSON and not as a
    // second key: every other line in this protocol escapes its own strings for the same reason.
    #[test]
    fn a_path_with_a_quote_in_it_leaves_as_one_escaped_string() {
        let line = fsinfo_line(&None, "/tmp/a\"b", "", true, false);
        assert_eq!(line, r#"{"t":"fsinfo","fs":"","free":0,"path":"/tmp/a\"b","class":"","tagable":true}"#);
    }

    #[test]
    fn one_statfs_answers_both_the_figures_and_the_magic() {
        let d = TestDir::new("fsinfoonestatfs");
        test_reset_statfs();
        let (info, magic) = read_with_magic(d.path());
        assert!(info.is_some_and(|i| i.free > 0), "a temp directory is on a mounted filesystem");
        assert!(magic.is_some(), "the same call carries f_type");
        assert_eq!(statfs_calls(), 1, "figures and magic share one statfs, not two");
        test_reset_statfs();
        assert_eq!(read_with_magic(&d.join("never-existed")), (None, None));
        assert_eq!(statfs_calls(), 1, "even a failure is one call");
    }
}
