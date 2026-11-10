use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn missing_relative_and_special_sources_are_not_text() {
    let d = TestDir::new("sniff-absent");
    assert_eq!(read_head("relative"), Some(Head::Binary));
    assert_eq!(read_head(&d.join("missing").to_string_lossy()), None);
    assert_eq!(read_head(&d.path().to_string_lossy()), Some(Head::Binary));
    assert!(remote("", d.path()), "missing mount evidence fails closed");
    assert!(slow("autofs") && slow("nfs4") && slow("fuse.gvfsd-fuse"));
}

#[test]
fn symlinks_recheck_mounts_and_stop_at_the_hop_limit() {
    let d = TestDir::new("sniff-links");
    let local = "/dev/root / ext4 rw 0 0\n";
    std::fs::write(d.join("notes"), "words").unwrap();
    std::os::unix::fs::symlink("notes", d.join("link")).unwrap();
    assert_eq!(resolve_local(local, &d.join("link")), Some(d.join("notes")));
    let far = d.dir("far");
    std::fs::write(far.join("notes"), "words").unwrap();
    std::os::unix::fs::symlink(far.join("notes"), d.join("remote")).unwrap();
    // The target exists, so only the per-hop mount check can refuse it.
    let slow_far = format!("{local}rc: {} fuse.rclone rw 0 0\n", far.display());
    assert_eq!(resolve_local(&slow_far, &d.join("remote")), None);
    std::os::unix::fs::symlink(d.join("notes"), far.join("back")).unwrap();
    std::os::unix::fs::symlink(far.join("back"), d.join("through")).unwrap();
    // A hop through the slow mount is refused even when the final target is local.
    assert_eq!(resolve_local(&slow_far, &d.join("through")), None);
    std::os::unix::fs::symlink("loop", d.join("loop")).unwrap();
    assert_eq!(resolve_local(local, &d.join("loop")), None);
}

#[test]
fn only_the_bounded_head_is_classified() {
    let d = TestDir::new("sniff-bound");
    let mut text = vec![b'a'; HEAD_BYTES];
    text.push(0);
    std::fs::write(d.join("large"), text).unwrap();
    assert_eq!(
        read_head(&d.join("large").to_string_lossy()),
        Some(Head::Text)
    );
}

const TABLE: &str =
    "/dev/root / btrfs rw 0 0\nrc: /mnt/My\\040Drive fuse.rclone rw 0 0\nauto /net autofs rw 0 0\n";

#[test]
fn mount_points_with_escapes_and_automounts_count_as_remote() {
    assert!(remote(TABLE, Path::new("/mnt/My Drive/notes")));
    assert!(remote(TABLE, Path::new("/net/host/notes")));
    assert!(!remote(TABLE, Path::new("/home/me/notes")));
    assert!(!remote(TABLE, Path::new("/mnt/My Drivers/notes")));
    assert_eq!(unescape("a\\011b\\134c"), "a\tb\\c");
}

#[test]
fn the_descriptor_mount_is_found_by_its_id() {
    let info =
        "21 1 0:20 / / rw - btrfs /dev/root rw\n57 21 0:50 / /mnt/d rw - fuse.rclone rc: rw\n";
    assert_eq!(mount_kind(info, "57").as_deref(), Some("fuse.rclone"));
    assert_eq!(mount_kind(info, "99"), None);
    // This process's own open file resolves to some mount kind through fdinfo.
    let file = std::fs::File::open("/proc/self/mountinfo").unwrap();
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(descriptor_kind(file.as_raw_fd(), &mountinfo).is_some());
}

#[test]
fn only_a_local_regular_file_with_a_text_head_reads_as_text() {
    let dir = TestDir::new("sniff");
    let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
    std::fs::write(at("notes"), "plain words\n").unwrap();
    std::fs::write(at("blob"), [0u8, 1, 2, 3]).unwrap();
    std::fs::write(at("empty"), "").unwrap();
    std::fs::write(at("myscript"), "#!/bin/bash\necho hi\n").unwrap();
    std::fs::write(at("latin1"), b"caf\xe9 na\xefve\n").unwrap();
    std::fs::write(at("utf16"), b"\xff\xfeh\0i\0\n\0").unwrap();
    let fifo = std::process::Command::new("mkfifo")
        .arg(at("pipe"))
        .status()
        .is_ok_and(|status| status.success());
    assert_eq!(read_head(&at("notes")), Some(Head::Text));
    assert_eq!(read_head(&at("blob")), Some(Head::Binary));
    assert_eq!(read_head(&at("empty")), Some(Head::Binary));
    assert_eq!(read_head(&at("myscript")), Some(Head::Script));
    assert_eq!(read_head(&at("latin1")), Some(Head::Text));
    assert_eq!(read_head(&at("utf16")), Some(Head::Text));
    // A fifo is refused by its own stat before any open, so it never holds the reader.
    if fifo {
        assert_eq!(read_head(&at("pipe")), Some(Head::Binary));
    }
}

#[test]
fn a_file_that_cannot_be_read_yet_keeps_no_answer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new("sniff-locked");
    let path = dir.join("notes");
    std::fs::write(&path, "plain words\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let at = path.to_string_lossy().into_owned();
    // Root reads it anyway; anyone else gets no answer, and the same file reads as text once it may be read.
    if std::fs::File::open(&path).is_err() {
        assert_eq!(read_head(&at), None);
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(read_head(&at), Some(Head::Text));
}
