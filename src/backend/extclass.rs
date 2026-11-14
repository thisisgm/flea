// The directory's class for the thumbnail gate (network, phone, usb or local "") once per directory change.
// The block device checks live in blockdev.rs, which the GUI's folder preview shares.
pub use super::blockdev::link_is_usb;
use super::blockdev::usb_for;
#[cfg(test)]
use super::blockdev::{disk_of_link, is_usb};
use super::mountinfo::MountEntry;
use std::path::{Path, PathBuf};

// statfs magics that catch a kernel network mount under an fstype name fstype_is_network does not know.
const NETWORK_MAGICS: [i64; 5] = [0x6969, 0xFF534D42, 0xFE534D42, 0x01021997, 0x00C36400];

// Sample input: 0xFE534D42 trues, 0xEF53 falses.
pub fn magic_is_network(magic: i64) -> bool {
    NETWORK_MAGICS.contains(&magic)
}

// Sample input: "fuse.sshfs" trues, "ext4" falses, "NFS4" trues.
// Kept for fsinforeq.rs; extclass itself calls netfs directly.
pub fn fstype_is_network(fstype: &str) -> bool {
    super::netfs::is_network_fstype(fstype)
}

pub use super::gvfsroot::{gvfs_class, gvfs_root};

// The classes ui/js/DirSizes.js isRemoteClass refuses a folder size on; unknown never reaches the backend, classify always answers.
// Sample input: "network" and "phone" true, "usb" and "" false.
pub fn refuses_folder_sizes(class: &str) -> bool {
    matches!(class, "network" | "phone")
}

// Paths the link walk issued readlink on, so a test pins that none lands inside a mount that can wedge.
#[cfg(test)]
thread_local! {
    static LINK_READS: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

// The kernel's own bound on links followed in one path, past which the walk answers the raw path as realpath fails.
const MAX_LINK_HOPS: usize = 40;

// The one syscall the walk makes, on a path whose mount is already known not to wedge.
fn read_link(path: &Path) -> std::io::Result<PathBuf> {
    #[cfg(test)]
    LINK_READS.with(|v| v.borrow_mut().push(path.to_path_buf()));
    std::fs::read_link(path)
}

// The one link walk, stopping at the first prefix inside a mount that can wedge; sample input: a link into a cifs mount answers its edge plus the tail.
pub fn resolved_in(path: &Path, body: &str) -> PathBuf {
    use std::path::Component;
    if !path.is_absolute() || gvfs_class(path).is_some() {
        return path.to_path_buf();
    }
    let parts = |path: &Path| -> Vec<std::ffi::OsString> {
        path.components().filter(|c| matches!(c, Component::Normal(_) | Component::ParentDir)).rev().map(|c| c.as_os_str().to_owned()).collect()
    };
    let mut todo = parts(path);
    let mut at = PathBuf::from("/");
    let mut hops = 0;
    while let Some(name) = todo.pop() {
        if name == ".." {
            at.pop();
            continue;
        }
        let next = at.join(&name);
        let wedges = gvfs_class(&next).is_some()
            || super::mountinfo::mount_entry_in(&next, body).is_some_and(|e| super::netfs::can_wedge(&e.fstype));
        if wedges {
            let mut out = next;
            out.extend(todo.iter().rev());
            return out;
        }
        match read_link(&next) {
            Ok(target) => {
                hops += 1;
                if hops > MAX_LINK_HOPS {
                    return path.to_path_buf();
                }
                if target.is_absolute() {
                    at = PathBuf::from("/");
                }
                todo.extend(parts(&target));
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => at = next,
            Err(_) => return path.to_path_buf(),
        }
    }
    at
}

// The paths the link walk read since the last reset.
#[cfg(test)]
pub(crate) fn test_link_reads() -> Vec<PathBuf> {
    LINK_READS.with(|v| v.borrow().clone())
}

#[cfg(test)]
pub(crate) fn test_reset_link_reads() {
    LINK_READS.with(|v| v.borrow_mut().clear());
}

// Sample: (None, Some(0xFF534D42), false) answers "network"; (Some("ext4"), Some(0xEF53), false) answers "".
pub fn classify_parts(path: &Path, fstype: Option<&str>, magic: Option<i64>, usb: bool) -> &'static str {
    if let Some(class) = gvfs_class(path) {
        return class;
    }
    if fstype.is_some_and(super::netfs::is_network_fstype) {
        return "network";
    }
    if magic.is_some_and(magic_is_network) {
        return "network";
    }
    if usb {
        return "usb";
    }
    ""
}

// One link walk, one mountinfo read, one statfs and two sysfs reads at most, once per directory change.
pub fn classify(path: &Path) -> &'static str {
    if let Some(class) = gvfs_class(path) {
        return class;
    }
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    classify_in(path, &body)
}

// Sample input: a symlink to "/media/nas" answers "network" under a body mounting it cifs.
pub fn classify_in(path: &Path, body: &str) -> &'static str {
    early_class(path, body).unwrap_or_else(|| resolved_class(&resolved_in(path, body), body))
}

// The class a path answers before its link walk: a gvfs path, or a raw path on a kernel network mount.
pub fn early_class(path: &Path, body: &str) -> Option<&'static str> {
    if let Some(class) = gvfs_class(path) {
        return Some(class);
    }
    // The raw path's own mount first, so a kernel share whose server died costs no canonicalize.
    super::mountinfo::mount_entry_in(path, body).is_some_and(|e| super::netfs::is_network_fstype(&e.fstype)).then_some("network")
}

// The class of a path its link walk already resolved.
pub fn resolved_class(owned: &Path, body: &str) -> &'static str {
    if let Some(class) = gvfs_class(owned) {
        return class;
    }
    classify_entry(owned, super::mountinfo::mount_entry_in(owned, body).as_ref())
}

// The class from a mount entry the caller already read; a path or fstype that decides pays no statfs.
pub fn classify_entry(path: &Path, entry: Option<&MountEntry>) -> &'static str {
    if let Some(class) = gvfs_class(path) {
        return class;
    }
    if entry.is_some_and(|e| super::netfs::is_network_fstype(&e.fstype)) {
        return "network";
    }
    classify_entry_with_magic(path, entry, super::fsinfo::magic_of(path))
}

// Sample input: (Some("cifs"), None) answers "network" with no statfs of its own.
pub fn classify_entry_with_magic(path: &Path, entry: Option<&MountEntry>, magic: Option<i64>) -> &'static str {
    let decided = classify_parts(path, entry.map(|e| e.fstype.as_str()), magic, false);
    if !decided.is_empty() {
        return decided;
    }
    if usb_for(entry.map(|e| e.majmin.as_str())) {
        return "usb";
    }
    ""
}

// The wider removable-disk read durable copies take; it never changes the class this file answers.
#[path = "removable.rs"]
pub mod removable;

#[cfg(test)]
#[path = "extclass_tests.rs"]
mod link_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn network_magics_are_the_five_this_tree_mounts() {
        for magic in [0x6969, 0xFF534D42, 0xFE534D42, 0x01021997, 0x00C36400] {
            assert!(magic_is_network(magic), "0x{:x} is a network filesystem", magic);
        }
        for magic in [0xEF53, 0x9123683E, 0x2011BAB0, 0x01021994, 0x65735546] {
            assert!(!magic_is_network(magic), "0x{:x} is local or decided elsewhere", magic);
        }
    }

    #[test]
    fn only_network_and_phone_refuse_a_folder_size() {
        assert!(refuses_folder_sizes("network") && refuses_folder_sizes("phone"));
        assert!(!refuses_folder_sizes("usb") && !refuses_folder_sizes(""), "a local or removable disk still sizes");
    }

    #[test]
    fn a_gvfs_phone_scheme_is_a_phone_and_any_other_gvfs_path_is_network() {
        for path in [
            "/run/user/1000/gvfs/mtp:host=SAMSUNG/DCIM",
            "/run/user/1000/gvfs/gphoto2:host=usb%3A001%2C014/store",
            "/run/user/1000/gvfs/afc:host=abc/Documents",
        ] {
            assert_eq!(gvfs_class(Path::new(path)), Some("phone"), "{} is a phone", path);
        }
        for path in [
            "/run/user/1000/gvfs/smb-share:server=nas,share=media",
            "/run/user/1000/gvfs/sftp:host=nas/home/tom",
            "/run/user/1000/gvfs/dav:host=slot,ssl=true/notes.txt",
            "/run/user/1000/gvfs/",
        ] {
            assert_eq!(gvfs_class(Path::new(path)), Some("network"), "{} is network", path);
        }
        for path in ["/home/gm", "/run/user/1000/doc/x", "/tmp/gvfs/mtp:host=X", "/run/user/1000/gvfs"] {
            assert_eq!(gvfs_class(Path::new(path)), None, "{} is not a gvfs share", path);
        }
    }

    #[test]
    fn the_decision_prefers_phone_then_network_then_usb_then_local() {
        assert_eq!(classify_parts(Path::new("/media/nas"), Some("cifs"), Some(0xFF534D42), false), "network");
        assert_eq!(classify_parts(Path::new("/media/nas"), None, Some(0xFE534D42), false), "network");
        assert_eq!(classify_parts(Path::new("/run/user/1000/gvfs/mtp:host=X"), Some("ext4"), Some(0xEF53), true), "phone");
        assert_eq!(classify_parts(Path::new("/run/user/1000/gvfs/smb-share:server=n"), None, None, false), "network");
        assert_eq!(classify_parts(Path::new("/media/stick"), Some("vfat"), Some(0x4D44), true), "usb");
        assert_eq!(classify_parts(Path::new("/home/gm"), Some("ext4"), Some(0xEF53), false), "");
        assert_eq!(classify_parts(Path::new("/home/gm"), None, None, false), "");
    }

    #[test]
    fn a_gvfs_root_is_the_share_directory_whatever_the_depth() {
        let share = Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=media");
        assert_eq!(gvfs_root(&share.join("photos/2026")).as_deref(), Some(share));
        assert_eq!(gvfs_root(share).as_deref(), Some(share), "the share itself is its own root");
        assert_eq!(gvfs_root(Path::new("/run/user/1000/gvfs")), None, "the gvfs mount alone names no share");
        assert_eq!(gvfs_root(Path::new("/home/gm/gvfs/x")), None, "a local folder named gvfs is not a share");
    }

    #[test]
    fn only_a_usb_bus_segment_or_a_removable_disk_is_usb() {
        assert!(link_is_usb("../../devices/pci0000:00/0000:00:14.0/usb2/2-1/host0/block/sdb/sdb1"));
        assert!(link_is_usb("../../devices/usb/1-1/block/sda/sda1"));
        assert!(!link_is_usb("../../devices/pci0000:00/0000:00:1f.2/ata1/host0/block/sda/sda1"));
        assert!(!link_is_usb("../../devices/virtual/block/dm-0"));
        assert!(is_usb(None, Some("1\n")));
        assert!(is_usb(None, Some("1")));
        assert!(!is_usb(None, Some("0\n")));
        assert!(!is_usb(None, None));
        assert!(is_usb(Some("../../devices/usb/x/block/sdb/sdb1"), Some("0")));
    }

    #[test]
    fn the_disk_is_the_name_below_block() {
        assert_eq!(disk_of_link("../../devices/pci/host0/block/sdb/sdb1"), Some("sdb"));
        assert_eq!(disk_of_link("../../devices/pci/host0/block/nvme0n1/nvme0n1p2"), Some("nvme0n1"));
        assert_eq!(disk_of_link("../../devices/virtual/block/dm-0"), Some("dm-0"));
        assert_eq!(disk_of_link("/no/block/here"), Some("here"));
        assert_eq!(disk_of_link("plain/path"), None);
        assert_eq!(disk_of_link("../../devices/pci/block/"), None);
    }

    #[test]
    fn resolved_follows_a_symlink_and_keeps_a_gvfs_path_and_a_failure() {
        let d = crate::backend::testdir::TestDir::new("extclass-resolved");
        let target = d.dir("real");
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let local = "1 0 0:30 / / rw - ext4 /dev/a rw\n";
        assert_eq!(resolved_in(&link, local), target, "a symlink answers its target");
        assert_eq!(resolved_in(&d.join("never-existed"), local), d.join("never-existed"), "a failure keeps today's path");
    }

    #[test]
    fn a_symlink_into_a_network_mount_classifies_by_its_target() {
        let d = crate::backend::testdir::TestDir::new("extclass-link");
        let target = d.dir("real");
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let body = format!("1 0 0:30 / / rw - ext4 /dev/a rw\n30 1 0:9 / {} rw - cifs //nas/media rw\n", target.display());
        assert_eq!(classify_in(&link, &body), "network", "a symlink into a cifs mount is network, not local");
        let plain = "1 0 0:30 / / rw - ext4 /dev/a rw\n".to_string();
        assert_eq!(classify_in(&link, &plain), "", "the same link stays local when its target is local");
    }

    // A link into a share is followed no further than the share's edge: no readlink lands inside it, even when the target is gone.
    #[test]
    fn a_link_into_a_share_classifies_network_with_no_syscall_inside_the_share() {
        test_reset_link_reads();
        let d = crate::backend::testdir::TestDir::new("extclass-link-wedge");
        let share = d.dir("share").canonicalize().unwrap();
        let (link, gone) = (d.path().join("link"), d.path().join("gone"));
        std::os::unix::fs::symlink(&share, &link).unwrap();
        std::os::unix::fs::symlink(share.join("missing").join("deep"), &gone).unwrap();
        let body = format!("1 0 0:30 / / rw - ext4 /dev/a rw\n30 1 0:9 / {} rw - cifs //nas/media rw\n", share.display());
        assert_eq!(classify_in(&link.join("sub"), &body), "network", "a link into a share is network");
        assert_eq!(classify_in(&gone, &body), "network", "a link to a path the share cannot show is network too");
        let reads = test_link_reads();
        assert!(reads.contains(&link) && reads.contains(&gone), "the links themselves are read, so the hook sees the walk: {:?}", reads);
        assert!(reads.iter().all(|p| !p.starts_with(&share)), "no syscall reaches the share: {:?}", reads);
        test_reset_link_reads();
    }

    #[test]
    fn a_network_entry_classifies_without_a_statfs() {
        crate::backend::fsinfo::test_reset_statfs();
        let entry = MountEntry { mount: PathBuf::from("/media/nas"), fstype: "smb3".to_string(), majmin: "0:27".to_string(), source: "//nas/media".to_string() };
        assert_eq!(classify_entry(Path::new("/media/nas/photos"), Some(&entry)), "network");
        assert_eq!(crate::backend::fsinfo::statfs_calls(), 0, "classify_entry itself decides on the fstype");
    }

    // Quick Look reads a Markdown file ahead and inline only on class "": every kernel and FUSE share below must not get it.
    #[test]
    fn kernel_cifs_nfs_and_fuse_shares_take_the_network_class_that_keeps_quick_look_from_reading_them() {
        let body = "1 0 0:30 / / rw - ext4 /dev/a rw\n\
            31 1 0:27 / /mnt/cifs rw - cifs //nas/media rw\n\
            32 1 0:28 / /mnt/nfs rw - nfs4 nas:/export rw\n\
            33 1 0:29 / /mnt/smb rw - smb3 //nas/media rw\n\
            34 1 0:40 / /mnt/sshfs rw - fuse.sshfs me@nas:/ rw\n\
            35 1 0:41 / /mnt/rclone rw - fuse.rclone remote: rw\n\
            36 1 0:42 / /mnt/s3 rw - fuse.s3fs bucket rw\n\
            37 1 0:43 / /mnt/local-fuse rw - fuse.mergerfs a:b rw\n";
        for mount in ["cifs", "nfs", "smb", "sshfs", "rclone", "s3"] {
            let path = format!("/mnt/{}/notes/README.md", mount);
            assert_eq!(classify_in(Path::new(&path), body), "network", "{} reaches the class Quick Look refuses", mount);
        }
        assert_eq!(classify_in(Path::new("/mnt/local-fuse/README.md"), body), "", "an unlisted FUSE type is local, as netfs.rs pins");
    }

    #[test]
    fn a_network_fstype_classifies_without_a_statfs() {
        crate::backend::fsinfo::test_reset_statfs();
        let body = "31 23 0:27 / /media/nas rw - cifs //nas/media rw\n";
        assert_eq!(classify_in(Path::new("/media/nas/photos"), body), "network");
        assert_eq!(crate::backend::fsinfo::statfs_calls(), 0, "fstype already decided; no statfs for nothing");
    }
}
