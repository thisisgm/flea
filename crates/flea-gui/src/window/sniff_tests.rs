use super::*;

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
    let dir = std::env::temp_dir().join(format!("flea-sniff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
    std::fs::write(at("notes"), "plain words\n").unwrap();
    std::fs::write(at("blob"), [0u8, 1, 2, 3]).unwrap();
    std::fs::write(at("empty"), "").unwrap();
    let fifo = std::process::Command::new("mkfifo")
        .arg(at("pipe"))
        .status()
        .is_ok_and(|status| status.success());
    assert_eq!(looks_text(&at("notes")), Some(true));
    assert_eq!(looks_text(&at("blob")), Some(false));
    assert_eq!(looks_text(&at("empty")), Some(false));
    // A fifo with no writer returns at once, refused, rather than holding the reader.
    if fifo {
        assert_eq!(looks_text(&at("pipe")), Some(false));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn one_read_runs_and_a_newer_ask_replaces_the_one_waiting(cx: &mut gpui::TestAppContext) {
    let dir = std::env::temp_dir().join(format!("flea-sniff-queue-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = |name: &str| {
        let path = dir.join(name).to_string_lossy().into_owned();
        std::fs::write(&path, "words\n").unwrap();
        (path, 6u64, 0i64)
    };
    let (a, b, c) = (file("a"), file("b"), file("c"));
    let heard = std::rc::Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
        for key in [&a, &b, &c] {
            let (heard, name) = (heard.clone(), key.0.clone());
            ask(key.clone(), cx, move |_| heard.borrow_mut().push(name));
        }
    });
    cx.run_until_parked();
    // a ran at once; b waited and c took its place, so b was never read and never heard.
    assert_eq!(*heard.borrow(), [a.0.clone(), c.0.clone()]);
    assert_eq!(
        (answer(&a), answer(&b), answer(&c)),
        (Some(true), None, Some(true))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_cannot_be_read_yet_keeps_no_answer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("flea-sniff-locked-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes");
    std::fs::write(&path, "plain words\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let at = path.to_string_lossy().into_owned();
    // Root reads it anyway; anyone else gets no answer, and the same file reads as text once it may be read.
    if std::fs::File::open(&path).is_err() {
        assert_eq!(looks_text(&at), None);
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(looks_text(&at), Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}
