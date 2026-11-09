use super::*;
use crate::backend::inotifyburst::EVENT_HEADER;

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
    assert_eq!(descriptors(&event(3, b"a.txt\0\0\0")), vec![3]);
}

#[test]
fn a_burst_names_each_watch_once() {
    let mut buf = event(3, b"a.txt\0\0\0");
    buf.extend(event(3, b""));
    buf.extend(event(4, b"b.txt\0\0\0"));
    assert_eq!(descriptors(&buf), vec![3, 4]);
}

// Not a shape inotify produces: it pins the bound, so a length running past the slice cannot panic.
#[test]
fn a_truncated_tail_ends_the_walk() {
    let mut buf = event(3, b"a.txt\0\0\0");
    buf.extend(event(4, b"this name did not fit"));
    buf.truncate(buf.len() - 4);
    assert_eq!(descriptors(&buf), vec![3, 4]);
}

#[test]
fn a_short_buffer_names_nothing() {
    assert_eq!(descriptors(&[0u8; 8]), Vec::<i32>::new());
}

// Sample input: wd 7 mask IN_UNMOUNT answers unmounted [7] and changed [].
fn unmount_event(wd: i32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&wd.to_ne_bytes());
    out.extend_from_slice(&IN_UNMOUNT.to_ne_bytes());
    out.extend_from_slice(&0u32.to_ne_bytes());
    out.extend_from_slice(&0u32.to_ne_bytes());
    out
}

#[test]
fn an_unmount_never_becomes_a_changed_relist() {
    assert_eq!(descriptors(&unmount_event(7)), Vec::<i32>::new());
    assert_eq!(classify_burst(&unmount_event(7)), (Vec::new(), vec![7], Vec::new()));
}

#[test]
fn a_removed_watch_is_its_folder_vanishing() {
    let mut out = Vec::new();
    out.extend_from_slice(&7i32.to_ne_bytes());
    out.extend_from_slice(&IN_IGNORED.to_ne_bytes());
    out.extend_from_slice(&0u32.to_ne_bytes());
    out.extend_from_slice(&0u32.to_ne_bytes());
    assert_eq!(classify_burst(&out), (Vec::new(), Vec::new(), vec![7]));
}

// S3-7: a deleted or moved open folder was a content change, so the pane re-listed a path that was no longer there.
#[test]
fn a_folder_deleted_or_moved_is_never_a_changed_relist() {
    use crate::backend::inotifyburst::event as raw;
    let burst = |events: &[(i32, u32)]| events.iter().flat_map(|&(wd, mask)| raw(wd, mask, &[])).collect::<Vec<u8>>();
    assert_eq!(classify_burst(&burst(&[(7, IN_DELETE)])), (vec![7], Vec::new(), Vec::new()), "a child's delete is a change");
    assert_eq!(classify_burst(&burst(&[(7, IN_DELETE), (7, IN_DELETE_SELF), (7, IN_IGNORED)])), (Vec::new(), Vec::new(), vec![7]));
    assert_eq!(classify_burst(&burst(&[(3, IN_CREATE), (7, IN_MOVE_SELF), (7, IN_CREATE)])), (vec![3], Vec::new(), vec![7]));
}

#[test]
fn an_ejected_folder_lands_on_its_nearest_existing_parent() {
    use std::collections::HashSet;
    let live: HashSet<&str> = ["/media", "/"].into_iter().collect();
    let at = nearest_parent(Path::new("/media/stick/photos"), |p| live.contains(p.to_str().unwrap_or("")));
    assert_eq!(at, Path::new("/media"));
    assert_eq!(unmounted_line(Path::new("/media/stick"), &at), r#"{"t":"unmounted","path":"/media/stick","parent":"/media"}"#);
}

#[test]
fn nothing_is_current_before_a_directory_is_followed() {
    let w = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    assert!(!w.is_current(-1));
    assert!(!w.is_current(1));
}

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

// An abandoned re-list of the open folder aliases onto its descriptor; a real fd, since at -1 drop_one makes no syscall.
#[test]
fn an_abandoned_re_list_of_the_same_folder_keeps_its_watch() {
    let sandbox = crate::backend::testdir::TestDir::new("watch-abandon");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.begin(sandbox.path());
    w.commit(Vec::new());
    let live = w.wd;
    assert!(live >= 0, "the sandbox could not be watched");

    w.begin(sandbox.path());
    assert_eq!(w.incoming, live, "a re-list of one inode aliases onto the descriptor it has");
    w.abandon();

    sandbox.file("after-an-abandoned-relist.txt", "x");
    assert!(saw_a_create(fd, live), "the abandoned re-list took the open folder's watch with it");
}

// A second begin before any commit answers the pending descriptor again, which it must not then remove.
#[test]
fn a_second_begin_of_one_folder_keeps_the_watch_it_arms() {
    let sandbox = crate::backend::testdir::TestDir::new("watch-rebegin");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.begin(sandbox.path());
    w.begin(sandbox.path());
    w.commit(Vec::new());
    assert!(w.wd >= 0, "the sandbox could not be watched");
    sandbox.file("after-a-second-begin.txt", "x");
    assert!(saw_a_create(fd, w.wd), "the second begin removed the watch it had just armed");
}

// A removal queues IN_IGNORED synchronously, so one nonblocking read observes it with no wait.
const IN_IGNORED: u32 = 0x0000_8000;
// Sample input: wd 1, mask 0x00000100, cookie 0, len 16, then "NEWFILE.txt\0\0\0\0\0".
fn ignored_wds(fd: c_int) -> Vec<c_int> {
    let mut buf = [0u8; BUF];
    let mut out = Vec::new();
    loop {
        let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, BUF) };
        if n <= 0 {
            break;
        }
        let mut at = 0;
        while at + EVENT_HEADER <= n as usize {
            let wd = i32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
            let mask = u32::from_ne_bytes([buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]]);
            let len = u32::from_ne_bytes([buf[at + 12], buf[at + 13], buf[at + 14], buf[at + 15]]) as usize;
            if mask & IN_IGNORED != 0 && !out.contains(&wd) {
                out.push(wd);
            }
            at += EVENT_HEADER + len;
        }
    }
    out
}

// A late worker hands its descriptor back instead of removing it, so a re-list of the open folder keeps its watch.
#[test]
fn a_late_re_list_of_the_open_folder_keeps_its_watch() {
    let sandbox = crate::backend::testdir::TestDir::new("watch-late");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.begin(sandbox.path());
    w.commit(Vec::new());
    let live = w.wd;
    assert!(live >= 0, "the sandbox could not be watched");
    let late = Watch::add_raw(fd, sandbox.path());
    assert_eq!(late, live, "a re-list of one inode aliases onto the descriptor it has");
    w.abandon_wd(late);
    sandbox.file("after-a-late-relist.txt", "x");
    assert!(saw_a_create(fd, live), "the late re-list took the open folder's watch with it");
}

// A failed scan's descriptor goes, but a failed re-list aliases onto the live one, which stays.
#[test]
fn abandon_wd_removes_only_a_watch_that_is_not_live() {
    let a = crate::backend::testdir::TestDir::new("abandon-a");
    let b = crate::backend::testdir::TestDir::new("abandon-b");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.begin(a.path());
    w.commit(Vec::new());
    let live = w.wd;
    assert!(live >= 0, "the sandbox could not be watched");
    let other = Watch::add_raw(fd, b.path());
    assert!(other >= 0 && other != live, "the second sandbox arms its own descriptor");
    assert!(ignored_wds(fd).is_empty(), "arming queues no removal");
    w.abandon_wd(other);
    assert_eq!(ignored_wds(fd), vec![other], "the failed scan's descriptor is removed");
    assert!(w.is_current(live), "the open folder is still followed");
    w.abandon_wd(live);
    assert!(ignored_wds(fd).is_empty(), "a failed re-list aliases onto the live watch, which stays");
}

// No descriptor in these two, so they pin the bookkeeping alone; the one above pins the syscall.
#[test]
fn an_abandoned_scan_leaves_the_current_watch_alone() {
    let mut w = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.begin(Path::new("/tmp"));
    w.abandon();
    assert!(w.is_current(7));
}

// And one that succeeds hands the listing over to the descriptor the scan was armed with.
#[test]
fn a_committed_scan_takes_over_from_the_old_watch() {
    let mut w = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: 9, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.commit(Vec::new());
    assert!(w.is_current(9));
    assert!(!w.is_current(7));
}

// Production arms through set_incoming plus commit, with the mount the list worker computed.
#[test]
fn set_incoming_captures_the_mount_commit_takes() {
    let mut w = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    w.set_incoming(7, Path::new("/media/stick").to_path_buf());
    w.commit(Vec::new());
    assert!(w.is_current(7));
    assert_eq!(w.mount, Path::new("/media/stick"), "the watch reports against a dead mountpoint");
}

#[test]
fn an_unmount_burst_never_answers_changed_for_a_vanished_path() {
    // Sample input: IN_UNMOUNT plus IN_IGNORED on wd 7 answers changed [] and unmounted [7].
    fn burst(wd: i32, masks: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        for mask in masks {
            out.extend_from_slice(&wd.to_ne_bytes());
            out.extend_from_slice(&mask.to_ne_bytes());
            out.extend_from_slice(&0u32.to_ne_bytes());
            out.extend_from_slice(&0u32.to_ne_bytes());
        }
        out
    }
    assert_eq!(classify_burst(&burst(7, &[IN_UNMOUNT, IN_IGNORED])), (Vec::new(), vec![7], Vec::new()));
    assert_eq!(classify_burst(&burst(7, &[IN_CREATE, IN_UNMOUNT])), (Vec::new(), vec![7], Vec::new()));
}

#[test]
fn an_ejected_mountpoint_lands_outside_the_volume() {
    // Sample input: open "/media/stick/photos" with mount "/media/stick" starts "/media".
    use std::collections::HashSet;
    let w = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: -1, mount: Path::new("/media/stick").to_path_buf(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    assert_eq!(w.unmount_start(Path::new("/media/stick/photos")), Path::new("/media"));
    assert_eq!(w.unmount_start(Path::new("/media/stick")), Path::new("/media"));
    let live: HashSet<&str> = ["/media", "/"].into_iter().collect();
    let start = w.unmount_start(Path::new("/media/stick/photos"));
    let at = nearest_parent(&start, |p| live.contains(p.to_str().unwrap_or("")));
    assert_eq!(at, Path::new("/media"), "a surviving mountpoint is never the parent");
    let bare = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry: Default::default() };
    assert_eq!(bare.unmount_start(Path::new("/media/stick")), Path::new("/media"));
}

#[test]
fn the_changed_line_names_its_directory() {
    assert_eq!(
        changed_line(Path::new("/tmp/a \"b\"")),
        r#"{"t":"changed","path":"/tmp/a \"b\""}"#
    );
}

#[path = "watch_outline_tests.rs"]
mod outline;
