// The /proc/self/mountinfo parser: the filesystem type of the mount that owns a path, from a body the caller has read.
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

// Sample: `2 1 0:9 / /home/pi/My\040Drive rw - fuse.rclone remote: rw`; longest enclosing mount wins after octal unescaping.
pub(crate) fn mount_type_in(path: &Path, body: &str) -> Option<String> {
    mount_entry_in(path, body).map(|entry| entry.fstype)
}

// FAT and NTFS report fixed modes; CIFS and SMB3 keep per-file modes with POSIX, Unix, ACL or SID mode options.
pub(crate) fn modes_are_real(path: &Path, body: &str) -> bool {
    #[cfg(test)]
    LEXICAL_LOOKUPS.with(|lookups| lookups.set(lookups.get() + 1));
    if let Some(real) = super::gvfsroot::gvfs_modes_are_real(path) {
        return real;
    }
    mount_type_in(path, body).is_none_or(|fstype| modes_for_mount(&fstype, &mount_options_in(path, body).unwrap_or_default()))
}

// Sample input: dev 0x811 finds a "8:17 ... - cifs ... rw,posix" entry and answers Some(true), with no path syscall.
pub(crate) fn modes_are_real_for_dev(dev: u64, body: &str) -> Option<bool> {
    #[cfg(test)]
    DEV_LOOKUPS.with(|lookups| lookups.set(lookups.get() + 1));
    if dev == 0 {
        return None;
    }
    let want = dev_majmin(dev);
    body.lines().find_map(|line| {
        let (_, fstype, majmin, _) = parse_line(line)?;
        (majmin == want).then(|| modes_for_mount(&fstype, &super_options(line)))
    })
}

// Sample input: cifs with "rw,cifsacl" or "rw,modefromsid" keeps modes, while "rw,cifsacl=0,modefromsid=0" does not.
fn modes_for_mount(fstype: &str, options: &str) -> bool {
    match fstype {
        "cifs" | "smb3" => options.split(',').any(|option| matches!(option, "posix" | "unix" | "cifsacl" | "modefromsid")),
        // One proxy device serves every gvfs backend, so it cannot vouch for execute bits; gvfs-spelled paths decide by backend first.
        "fuse.gvfsd-fuse" => false,
        "vfat" | "exfat" | "msdos" | "fat" | "fat12" | "fat16" | "fat32" | "ntfs" | "ntfs3" | "fuseblk" | "smbfs" => false,
        _ => true,
    }
}

// Sample dev: makedev(8,17) is 0x811 and answers "8:17"; the glibc major/minor packing, see sys/sysmacros.h.
pub(crate) fn dev_majmin(dev: u64) -> String {
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & 0xffff_f000);
    let minor = (dev & 0xff) | ((dev >> 12) & 0xffff_ff00);
    format!("{major}:{minor}")
}

// Sample input: dev 0x811 against a line carrying "8:17 ... - vfat ..." answers Some("vfat").
pub(crate) fn fstype_for_dev(dev: u64, body: &str) -> Option<String> {
    let want = dev_majmin(dev);
    body.lines().filter_map(parse_line).find(|(_, _, majmin, _)| *majmin == want).map(|(_, fstype, _, _)| fstype)
}

// The mountpoints ui/js/Devices.js isSystemPath names exactly; a test reads that script's list, so the two cannot drift.
const SYSTEM_MOUNTS: &[&str] = &[
    "/bin", "/boot", "/compat/linux/proc", "/compat/linux/sys", "/etc", "/home", "/lib", "/lib64", "/libexec",
    "/live/cow", "/live/image", "/media", "/mnt", "/net", "/opt", "/rescue", "/root", "/run", "/sbin", "/srv",
    "/tmp", "/usr", "/usr/X11R6", "/usr/local", "/usr/obj", "/usr/ports", "/usr/src", "/usr/xobj", "/var",
    "/var/crash", "/var/local", "/var/log", "/var/log/audit", "/var/mail", "/var/run", "/var/tmp",
];

// Sample: "/" and "/home" answer true, "/home/gm/Data" and "/mnt/data" false; ui/js/Devices.js isSystemPath, rule for rule.
fn is_system_mount(mount: &Path) -> bool {
    let p = mount.to_string_lossy();
    let under = |dir: &str| p == dir || p.strip_prefix(dir).is_some_and(|rest| rest.starts_with('/'));
    p == "/" || p.contains("/.") || under("/dev") || under("/proc") || under("/sys") || SYSTEM_MOUNTS.contains(&&*p)
}

// Sample: against `1 0 0:29 /@ / rw - btrfs /dev/mapper/root rw` an entry on 0:29 answers true, as a mount of its device sits on "/".
pub(crate) fn on_system_volume(entry: &MountEntry, body: &str) -> bool {
    body.lines().filter_map(parse_line).any(|(mount, _, majmin, _)| majmin == entry.majmin && is_system_mount(&mount))
}

// The mount that owns a path: its mount point, its filesystem type, its device numbers and its source.
pub(crate) struct MountEntry {
    pub mount: PathBuf,
    pub fstype: String,
    pub majmin: String,
    pub source: String,
}

// Sample: the line above answers mount "/home/pi/My Drive", fstype "fuse.rclone" and majmin "0:9".
pub(crate) fn mount_entry_in(path: &Path, body: &str) -> Option<MountEntry> {
    let mut best: Option<(usize, MountEntry)> = None;
    for (mount, fstype, majmin, source) in body.lines().filter_map(parse_line) {
        if !path.starts_with(&mount) {
            continue;
        }
        let depth = mount.components().count();
        if best.as_ref().map(|(old, _)| depth >= *old).unwrap_or(true) {
            best = Some((depth, MountEntry { mount, fstype, majmin, source }));
        }
    }
    best.map(|(_, entry)| entry)
}

// Every mount point with whether its filesystem can wedge a syscall, a later line winning a point mounted twice as mount_entry_in does.
pub(crate) fn wedge_points_in(body: &str) -> std::collections::HashMap<PathBuf, bool> {
    body.lines().filter_map(parse_line).map(|(mount, fstype, _, _)| (mount, super::netfs::can_wedge(&fstype))).collect()
}

// The super options of the deepest mount owning path, so a caller can read windows_names off ntfs3.
pub(crate) fn mount_options_in(path: &Path, body: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in body.lines() {
        let Some((mount, _, _, _)) = parse_line(line) else { continue };
        if !path.starts_with(&mount) {
            continue;
        }
        let depth = mount.components().count();
        if best.as_ref().map(|(old, _)| depth >= *old).unwrap_or(true) {
            best = Some((depth, super_options(line)));
        }
    }
    best.map(|(_, opts)| opts)
}

// The raw mount options (field 5) of the deepest mount owning path, for the run card's noexec rule: "2 1 0:9 / /mnt/stick rw,noexec - vfat /dev/sdb1 rw" answers "rw,noexec".
pub(crate) fn raw_mount_options_in(path: &Path, body: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in body.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if !fields.contains(&"-") {
            continue;
        }
        if fields.len() < 6 {
            continue;
        }
        let mount = PathBuf::from(OsString::from_vec(unescape(fields[4])));
        if !path.starts_with(&mount) {
            continue;
        }
        let depth = mount.components().count();
        if best.as_ref().map(|(old, _)| depth >= *old).unwrap_or(true) {
            best = Some((depth, fields[5].to_string()));
        }
    }
    best.map(|(_, options)| options)
}

mod topdir;
pub(crate) use topdir::{deepest, mount_roots_in, mounts_in};

// Sample line as above: the fields after the source are the filesystem's super options.
fn super_options(line: &str) -> String {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let Some(split) = fields.iter().position(|field| *field == "-") else { return String::new() };
    fields.iter().skip(split + 3).copied().collect::<Vec<_>>().join(" ")
}

// The deepest mount holding path, the later line winning a tie as the kernel's own stacking order does.
pub(crate) fn enclosing<'a>(path: &Path, mounts: &'a [(PathBuf, String)]) -> Option<&'a (PathBuf, String)> {
    let mut best: Option<(usize, &(PathBuf, String))> = None;
    for mount in mounts {
        if !path.starts_with(&mount.0) {
            continue;
        }
        let depth = mount.0.components().count();
        if best.map(|(old, _)| depth >= old).unwrap_or(true) {
            best = Some((depth, mount));
        }
    }
    best.map(|(_, mount)| mount)
}

// One counted read of the mount table, so a walk pays one read and passes the body down.
pub(crate) fn read_body() -> String {
    #[cfg(test)]
    READS.with(|reads| reads.set(reads.get() + 1));
    std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default()
}

// Thread-local, so parallel suites never see each other's reads; tests reset it with test_reset.
#[cfg(test)]
thread_local! {
    static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static DEV_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(crate) static LEXICAL_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// Counted reads since the last reset, so a session walk asserts at most one over a known walk.
#[cfg(test)]
pub(crate) fn test_reset() {
    READS.with(|reads| reads.set(0));
    DEV_LOOKUPS.with(|lookups| lookups.set(0));
    LEXICAL_LOOKUPS.with(|lookups| lookups.set(0));
}

// The counted reads since the last reset.
#[cfg(test)]
pub(crate) fn test_reads() -> usize {
    READS.with(|reads| reads.get())
}

// Device lookups on this thread, so a window proves it decides each distinct device only once.
#[cfg(test)]
pub(crate) fn test_device_lookups() -> usize {
    DEV_LOOKUPS.with(|lookups| lookups.get())
}

// Sample line as above: mount point, fstype, major:minor and source, or None for a line with no "-" or too few fields.
fn parse_line(line: &str) -> Option<(PathBuf, String, String, String)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let split = fields.iter().position(|field| *field == "-")?;
    if fields.len() <= split + 2 || fields.len() < 5 {
        return None;
    }
    Some((PathBuf::from(OsString::from_vec(unescape(fields[4]))), fields[split + 1].to_string(), fields[2].to_string(), fields[split + 2].to_string()))
}

fn unescape(field: &str) -> Vec<u8> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match octal_byte(bytes, i) {
            Some(byte) => {
                out.push(byte);
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    out
}

// Sample: `\040` is a space. Summed wide and refused above 255, because `\400` overflows a u8 mid-sum.
fn octal_byte(bytes: &[u8], i: usize) -> Option<u8> {
    if bytes[i] != b'\\' || i + 3 >= bytes.len() {
        return None;
    }
    let digits = &bytes[i + 1..=i + 3];
    if !digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
        return None;
    }
    u8::try_from(digits.iter().fold(0u32, |value, b| value * 8 + u32::from(b - b'0'))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_system(path: &str, body: &str) -> bool {
        mount_entry_in(Path::new(path), body).is_some_and(|entry| on_system_volume(&entry, body))
    }

    #[test]
    fn deepest_mount_identifies_rclone_and_decodes_its_path() {
        let info = "1 0 8:1 / / rw - ext4 /dev/a rw\n\
                    2 1 0:9 / /home/pi/My\\040Drive rw - fuse.rclone remote: rw\n\
                    3 2 0:10 / /home/pi/My\\040Drive/nested rw - tmpfs tmpfs rw\n";
        assert_eq!(
            mount_type_in(Path::new("/home/pi/My Drive/file"), info).as_deref(),
            Some("fuse.rclone")
        );
        assert_eq!(
            mount_type_in(Path::new("/home/pi/My Drive/nested/file"), info).as_deref(),
            Some("tmpfs")
        );
        assert_eq!(
            mount_type_in(Path::new("/elsewhere"), info).as_deref(),
            Some("ext4")
        );
    }

    #[test]
    fn a_device_lookup_names_the_fstype_for_the_slow_pass_gate() {
        let info = "1 0 8:1 / / rw - ext4 /dev/a rw\n\
                    30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw\n\
                    31 1 0:45 / /media/nas rw - nfs nas:/share rw\n\
                    32 1 0:46 / /media/cloud rw - fuse.rclone remote: rw\n";
        // Sample dev numbers: makedev(8,17) is 0x811, makedev(0,45) is 45.
        assert_eq!(fstype_for_dev(0x811, info).as_deref(), Some("vfat"));
        assert_eq!(fstype_for_dev(45, info).as_deref(), Some("nfs"));
        assert_eq!(fstype_for_dev(46, info).as_deref(), Some("fuse.rclone"));
        assert_eq!(fstype_for_dev(0x812, info), None);
    }

    #[test]
    fn the_system_mountpoints_are_the_set_the_rail_hides() {
        let script = include_str!("../../ui/js/Devices.js");
        let list = script.split("var exact = [").nth(1).and_then(|rest| rest.split(']').next()).expect("isSystemPath keeps its exact list");
        let named: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
        assert_eq!(named, SYSTEM_MOUNTS, "ui/js/Devices.js and this list name the same mountpoints");
        let rules = script.split("function isSystemPath(p) {").nth(1).and_then(|body| body.split("var exact").next()).expect("isSystemPath keeps its rules above its list");
        let literals: Vec<&str> = rules.split('"').skip(1).step_by(2).collect();
        assert_eq!(literals, ["/", "/.", "/dev", "/proc", "/sys", "/dev/", "/proc/", "/sys/"], "isSystemPath's other rules are the ones is_system_mount mirrors");
        for path in ["/", "/home", "/boot", "/var", "/dev", "/dev/shm", "/proc/1", "/sys/fs", "/home/gm/.mnt/x"] {
            assert!(is_system_mount(Path::new(path)), "{path} is a system mountpoint");
        }
        for path in ["/mnt/data", "/home/gm/Data", "/home/gm/gdrive", "/run/media/gm/stick", "/devices", "/boot/efi"] {
            assert!(!is_system_mount(Path::new(path)), "{path} is not");
        }
    }

    #[test]
    fn a_volume_is_the_system_volume_when_a_mount_of_its_device_sits_on_a_system_path() {
        // Sample: sdb1 (8:17) is mounted twice.
        let info = "1 0 8:1 / / rw - ext4 /dev/sda1 rw\n\
                    30 1 8:17 / /mnt/data rw - ext4 /dev/sdb1 rw\n\
                    31 1 8:17 /sub /home/gm/Data rw - ext4 /dev/sdb1 rw\n\
                    32 1 0:50 / /home/gm/gdrive rw - fuse.rclone remote: rw\n";
        assert!(on_system("/etc", info), "the device holding / is the system volume");
        assert!(!on_system("/mnt/data/x", info), "a data volume mounted twice, on no system path");
        assert!(!on_system("/home/gm/gdrive/x", info), "a FUSE mount under home is no system volume");
        assert!(!on_system("/elsewhere", "30 1 8:17 / /mnt/data rw - ext4 /dev/sdb1 rw\n"), "a path no mount owns is no claim");
        assert!(on_system("/mnt/data/x", &format!("{info}33 1 8:17 / /srv rw - ext4 /dev/sdb1 rw\n")), "any one mount of the device on a system path decides");
    }

    #[test]
    fn every_btrfs_subvolume_of_the_root_filesystem_is_on_the_system_volume() {
        // Sample from a btrfs root: stat gives / 0:31 and /home/flea-sandbox 0:73, while every line of that one filesystem names 0:29.
        let info = "1 0 0:29 /@ / rw - btrfs /dev/mapper/root rw\n\
                    2 1 0:29 /@home /home rw - btrfs /dev/mapper/root rw\n\
                    3 1 0:29 /@log /var/log rw - btrfs /dev/mapper/root rw\n\
                    4 2 0:29 /@home/flea-sandbox /home/flea-sandbox rw - btrfs /dev/mapper/root rw\n\
                    5 1 0:52 / /mnt/backup rw - btrfs /dev/sdc1 rw\n";
        assert!(on_system("/etc", info), "the root subvolume");
        assert!(on_system("/home/flea-sandbox/cap-sysrefusal/listing", info), "a subvolume mounted under home shares the filesystem's number with /");
        assert!(!on_system("/mnt/backup/photos", info), "a second btrfs filesystem has its own number and is judged alone");
    }

    #[test]
    fn malformed_mountinfo_is_ignored() {
        assert_eq!(mount_type_in(Path::new("/home/pi"), "junk\n"), None);
    }

    #[test]
    fn the_entry_names_the_fstype_the_device_and_the_source() {
        let info = "1 0 8:1 / / rw - ext4 /dev/a rw\n\
                    30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw\n";
        let entry = mount_entry_in(Path::new("/media/stick/photo.jpg"), info).expect("the stick owns its files");
        assert_eq!(entry.fstype, "vfat");
        assert_eq!(entry.majmin, "8:17");
        assert_eq!(entry.source, "/dev/sdb1", "the source rides the entry for the batch gate");
        let root = mount_entry_in(Path::new("/elsewhere"), info).expect("the root owns the rest");
        assert_eq!((root.fstype.as_str(), root.majmin.as_str()), ("ext4", "8:1"));
        assert!(mount_entry_in(Path::new("/home/pi"), "junk\n").is_none());
    }

    // The kernel escapes only \040, \011, \012 and \134, but this is a parser at a trust boundary.
    #[test]
    fn an_octal_escape_above_255_is_refused_rather_than_overflowing() {
        assert_eq!(unescape("\\040"), b" ".to_vec());
        assert_eq!(unescape("\\134"), b"\\".to_vec());
        assert_eq!(unescape("\\400"), b"\\400".to_vec());
        assert_eq!(unescape("\\777"), b"\\777".to_vec());
    }

    #[test]
    fn the_deepest_mount_names_its_options_for_the_noexec_rule() {
        let info = "1 0 8:1 / / rw - ext4 /dev/a rw\n\
                    30 1 8:17 / /media/stick rw,noexec - vfat /dev/sdb1 rw\n";
        assert_eq!(
            raw_mount_options_in(Path::new("/media/stick/photo.jpg"), info).as_deref(),
            Some("rw,noexec")
        );
        assert_eq!(raw_mount_options_in(Path::new("/elsewhere"), info).as_deref(), Some("rw"));
        assert_eq!(raw_mount_options_in(Path::new("/home/pi"), "junk\n"), None);
    }

    #[test]
    fn modes_are_real_only_where_a_file_keeps_its_own_mode() {
        for fstype in ["ext4", "btrfs", "xfs", "tmpfs"] {
            let body = format!("1 0 8:1 / / rw - {fstype} /dev/a rw\n");
            assert!(modes_are_real(Path::new("/file"), &body), "{fstype} stores POSIX modes");
        }
        for fstype in ["vfat", "exfat", "msdos", "fat", "fat12", "fat16", "fat32", "ntfs", "ntfs3", "fuseblk", "cifs", "smb3", "smbfs"] {
            let body = format!("1 0 8:1 / / rw - {fstype} /dev/a rw\n");
            assert!(!modes_are_real(Path::new("/file"), &body), "{fstype} reports a mount default for every file, not the file's own mode");
        }
        assert!(modes_are_real(Path::new("/file"), "junk\n"), "an unknown mount preserves the reported mode");
    }

    #[test]
    fn smb_modes_use_only_the_owning_mounts_super_options() {
        for fstype in ["cifs", "smb3"] {
            let body = format!("1 0 0:30 / / rw - {fstype} //server/root rw,posix\n\
                                2 1 0:31 / /share rw,posix,unix - {fstype} //server/share rw\n\
                                3 2 0:32 / /share/unix rw - {fstype} //server/unix rw,unix\n");
            assert!(modes_are_real(Path::new("/file"), &body));
            assert!(!modes_are_real(Path::new("/share/file"), &body), "raw or parent options cannot enable real modes");
            assert!(modes_are_real(Path::new("/share/unix/file"), &body));
            let stacked = format!("{body}4 1 0:33 / /share/unix rw - {fstype} //server/fixed rw\n");
            assert!(!modes_are_real(Path::new("/share/unix/file"), &stacked), "the later mount wins the same depth");
        }
    }
}
