use super::*;

fn change(wd: i32, visible: bool) -> Change {
    Change { visible, hidden: !visible, ..Change::new(wd) }
}

// Sample input: two events on watch 3, one carrying a 16 byte name and one carrying none.
fn event(wd: i32, name: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&wd.to_ne_bytes());
    out.extend_from_slice(&IN_CREATE.to_ne_bytes());
    out.extend_from_slice(&0u32.to_ne_bytes());
    out.extend_from_slice(&(name.len() as u32).to_ne_bytes());
    out.extend_from_slice(name);
    out
}

#[test]
fn one_event_names_its_watch() {
    assert_eq!(descriptors(&event(3, b"a.txt\0\0\0")), vec![change(3, true)]);
}

#[test]
fn a_burst_names_each_watch_once() {
    let mut buf = event(3, b"a.txt\0\0\0");
    buf.extend(event(3, b""));
    buf.extend(event(4, b"b.txt\0\0\0"));
    assert_eq!(descriptors(&buf), vec![change(3, true), change(4, true)]);
}

// Not a shape inotify produces: it pins the bound, so a length running past the slice cannot panic.
#[test]
fn a_truncated_tail_ends_the_walk() {
    let mut buf = event(3, b"a.txt\0\0\0");
    buf.extend(event(4, b"this name did not fit"));
    buf.truncate(buf.len() - 4);
    assert_eq!(descriptors(&buf), vec![change(3, true), change(4, true)]);
}

#[test]
fn a_short_buffer_names_nothing() {
    assert_eq!(descriptors(&[0u8; 8]), Vec::<Change>::new());
}

#[test]
fn nothing_is_current_before_a_directory_is_followed() {
    let w = Watch::new(-1);
    assert!(!w.is_current(-1));
    assert!(!w.is_current(1));
}

// O_NONBLOCK, so a watch this test killed fails it by answering nothing rather than by hanging.
const IN_NONBLOCK: c_int = 0x800;

// Sample input: wd 1, mask 0x00000100, cookie 0, len 16, then "NEWFILE.txt\0\0\0\0\0".
fn carries_a_create(buf: &[u8], wd: c_int) -> bool {
    let mut at = 0;
    while at + EVENT_HEADER <= buf.len() {
        let this = i32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
        let mask = u32::from_ne_bytes([buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]]);
        let len = u32::from_ne_bytes([buf[at + 12], buf[at + 13], buf[at + 14], buf[at + 15]]) as usize;
        // The mask and not the descriptor, because a removed watch's own IN_IGNORED carries it too.
        if this == wd && (mask & IN_CREATE) != 0 {
            return true;
        }
        at += EVENT_HEADER + len;
    }
    false
}

// Two seconds all told, which is the kernel queueing being slow rather than the watch being gone.
const TRIES: usize = 200;
const BETWEEN_TRIES: Duration = Duration::from_millis(10);

// The kernel queues on its own schedule, so this reads until the create lands or the tries run out.
fn saw_a_create(fd: c_int, wd: c_int) -> bool {
    let mut buf = [0u8; BUF];
    for _ in 0..TRIES {
        let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, BUF) };
        if n > 0 && carries_a_create(&buf[..n as usize], wd) {
            return true;
        }
        thread::sleep(BETWEEN_TRIES);
    }
    false
}

// inotify_add_watch answers the descriptor the folder already holds, so an abandoned re-list of the
// directory on screen must not remove it. This one carries a real descriptor because the two below
// run at fd -1, where drop_one makes no syscall and the guard therefore has nothing to show.
#[test]
fn an_abandoned_re_list_of_the_same_folder_keeps_its_watch() {
    let sandbox = crate::backend::testdir::TestDir::new("watch-abandon");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch::new(fd);
    w.begin(sandbox.path());
    w.commit(false);
    let live = w.wd;
    assert!(live >= 0, "the sandbox could not be watched");

    w.begin(sandbox.path());
    assert_eq!(w.incoming, live, "a re-list of one inode aliases onto the descriptor it has");
    w.abandon();

    sandbox.file("after-an-abandoned-relist.txt", "x");
    assert!(saw_a_create(fd, live), "the abandoned re-list took the open folder's watch with it");
}

// No descriptor in these two, so they pin the bookkeeping alone; the one above pins the syscall.
#[test]
fn an_abandoned_scan_leaves_the_current_watch_alone() {
    let mut w = Watch { wd: 7, incoming: -1, hidden: false, ..Watch::new(-1) };
    w.begin(Path::new("/tmp"));
    w.abandon();
    assert!(w.is_current(7));
}

// And one that succeeds hands the listing over to the descriptor the scan was armed with.
#[test]
fn a_committed_scan_takes_over_from_the_old_watch() {
    let mut w = Watch { wd: 7, incoming: 9, hidden: false, ..Watch::new(-1) };
    w.commit(false);
    assert!(w.is_current(9));
    assert!(!w.is_current(7));
}

#[test]
fn the_changed_line_names_its_directory() {
    assert_eq!(
        changed_line(Path::new("/tmp/a \"b\"")),
        r#"{"t":"changed","path":"/tmp/a \"b\""}"#
    );
}

#[test]
fn hidden_only_bursts_do_not_refresh_a_listing_that_hides_them() {
    let mut w = Watch { wd: 3, incoming: 3, hidden: false, ..Watch::new(-1) };
    let hidden_change = descriptors(&event(3, b".local\0\0")).remove(0);
    assert!(!w.should_refresh(&hidden_change, Path::new("/tmp")));
    w.commit(true);
    assert!(w.should_refresh(&hidden_change, Path::new("/tmp")));
    w.incoming = 3;
    w.commit(false);
    assert!(!w.should_refresh(&hidden_change, Path::new("/tmp")));
    assert!(!w.should_refresh(&change(4, true), Path::new("/tmp")));
}

#[test]
fn mixed_bursts_and_directory_self_events_still_refresh() {
    let mut w = Watch { wd: 3, incoming: -1, hidden: false, ..Watch::new(-1) };
    for visible in [b"name\0".as_slice(), b"".as_slice()] {
        for hidden_first in [true, false] {
            let mut buf = event(3, if hidden_first { b".local\0" } else { visible });
            buf.extend(event(3, if hidden_first { visible } else { b".local\0" }));
            let changes = descriptors(&buf);
            assert_eq!(changes.len(), 1);
            assert!(w.should_refresh(&changes[0], Path::new("/tmp")));
        }
    }
}

#[test]
fn a_failed_navigation_keeps_the_current_hidden_policy() {
    let mut w = Watch { wd: 3, incoming: -1, hidden: true, ..Watch::new(-1) };
    w.begin(Path::new("/definitely/not/here"));
    w.abandon();
    assert!(w.should_refresh(&change(3, false), Path::new("/tmp")));
}

#[test]
fn repeated_attribute_events_stay_quiet_after_a_relist_with_hidden_files_shown() {
    let d = crate::backend::testdir::TestDir::new("watch-relist-attributes");
    d.dir(".local");
    let mut w = Watch { wd: 3, hidden: true, ..Watch::new(-1) };
    for name in [b".local\0".as_slice(), b"".as_slice()] {
        let mut bytes = event(3, name);
        bytes[4..8].copy_from_slice(&IN_ATTRIB.to_ne_bytes());
        let change = descriptors(&bytes).remove(0);
        assert!(w.should_refresh(&change, d.path()));
        w.incoming = 3;
        w.commit(true);
        assert!(!w.should_refresh(&change, d.path()));
        w.begin(Path::new("/definitely/not/here"));
        w.abandon();
        assert!(!w.should_refresh(&change, d.path()));
    }
    w.incoming = 4;
    w.commit(true);
    let mut bytes = event(4, b".local\0");
    bytes[4..8].copy_from_slice(&IN_ATTRIB.to_ne_bytes());
    assert!(w.should_refresh(&descriptors(&bytes).remove(0), d.path()));
}
