use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::MetadataExt;

#[test]
fn gh458_statx_uses_no_follow_no_automount_and_no_remote_sync() {
    assert_eq!(O_PATH, libc::O_PATH);
    assert_eq!(STATX_ATTR_AUTOMOUNT, libc::STATX_ATTR_AUTOMOUNT as u64);
    for (fd, path, extra) in [
        (AT_FDCWD, c"sleeping", AT_STATX_DONT_SYNC),
        (77, c"", AT_EMPTY_PATH | AT_STATX_DONT_SYNC),
        (AT_FDCWD, c"explicit-count", 0),
    ] {
        let result = query_with(fd, path, extra, |got_fd, got_path, flags, mask, out| {
            assert_eq!(got_fd, fd);
            assert_eq!(unsafe { std::ffi::CStr::from_ptr(got_path) }, path);
            assert_eq!(
                flags,
                libc::AT_SYMLINK_NOFOLLOW | libc::AT_NO_AUTOMOUNT | extra
            );
            assert_eq!(mask, libc::STATX_BASIC_STATS | libc::STATX_MNT_ID);
            let mut native = unsafe { std::mem::zeroed::<libc::statx>() };
            native.stx_mask = mask;
            native.stx_mode = libc::S_IFDIR as u16;
            native.stx_attributes = libc::STATX_ATTR_AUTOMOUNT as u64;
            native.stx_attributes_mask = libc::STATX_ATTR_AUTOMOUNT as u64;
            native.stx_nlink = 2;
            native.stx_dev_major = 8;
            native.stx_dev_minor = 257;
            native.stx_ino = 123;
            native.stx_size = 4096;
            native.stx_mnt_id = 99;
            assert_eq!(
                std::mem::size_of::<Statx>(),
                std::mem::size_of::<libc::statx>()
            );
            unsafe { std::ptr::write(out.cast::<libc::statx>(), native) };
            0
        })
        .unwrap();
        assert_eq!(result.identity(), (libc::makedev(8, 257), 123, Some(99)));
        assert_eq!((result.size, result.nlink), (4096, 2));
        assert!(result.is_dir() && result.automount);
    }
}

#[test]
fn gh458_statx_failure_and_missing_required_fields_fail_closed() {
    assert!(query_with(AT_FDCWD, c"x", 0, |_, _, _, _, _| -1).is_none());
    assert!(decode(&[0; 256]).is_none());
    let mut bytes = [0; 256];
    bytes[..4].copy_from_slice(&STATX_BASIC_STATS.to_ne_bytes());
    bytes[8..16].copy_from_slice(&STATX_ATTR_AUTOMOUNT.to_ne_bytes());
    let entry = decode(&bytes).unwrap();
    assert!(
        !entry.automount,
        "an unsupported attribute is not authoritative"
    );
    assert_eq!(
        entry.mount, None,
        "older kernels fall back to the device boundary"
    );
}

#[test]
fn gh458_path_and_descriptor_probes_name_the_same_inode_without_following_links() {
    let d = TestDir::new("gh458-statx");
    let root = d.dir("root");
    let meta = root.symlink_metadata().unwrap();
    let entry = at(&root).unwrap();
    assert_eq!(
        (entry.dev, entry.ino, entry.size),
        (meta.dev(), meta.ino(), meta.size())
    );
    let pinned = open_same(&root, entry.identity()).unwrap();
    assert_eq!(of(&pinned).unwrap().identity(), entry.identity());
    let link = d.join("link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    assert!(!at(&link).unwrap().is_dir());
    assert!(open_same(&link, entry.identity()).is_none());
    let parent = std::fs::File::open(d.path()).unwrap();
    assert_eq!(
        child(&parent, OsStr::new("root"), false)
            .unwrap()
            .identity(),
        entry.identity()
    );
    assert!(!child(&parent, OsStr::new("link"), false).unwrap().is_dir());
}

#[test]
fn gh458_a_mount_changed_after_the_first_probe_is_not_reopened_for_reading() {
    let d = TestDir::new("gh458-open-mount");
    let root = d.dir("root");
    let expected = at(&root).unwrap().identity();
    {
        let _fake = fake::mount(&root, true, None);
        assert!(
            open_same(&root, expected).is_none(),
            "a newly attached automount must not be triggered by a read open"
        );
    }
    let _fake = fake::mount(&root, false, Some(expected.2.unwrap() + 1));
    assert!(
        open_same(&root, expected).is_none(),
        "a new bind mount with the same inode and device must not be read"
    );
}
