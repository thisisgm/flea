// Expanded children share descriptors with the open folder, so every drop path is pinned against a real inotify fd.
use super::*;

// Going up to a parent that shows the old folder expanded reuses its descriptor, so the commit keeps it.
#[test]
fn going_up_keeps_the_watch_an_expanded_child_still_uses() {
    let parent = crate::backend::testdir::TestDir::new("watch-up");
    let child = parent.dir("child");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    w.begin(&child);
    w.commit(Vec::new());
    let live = w.wd;
    assert!(live >= 0, "the sandbox could not be watched");
    w.begin(parent.path());
    let expanded = Watch::add_raw(fd, &child);
    assert_eq!(expanded, live, "the expanded child aliases onto the folder it was");
    w.commit(vec![expanded]);
    assert!(ignored_wds(fd).is_empty(), "going up removed a watch");
    assert!(w.is_current(live) && !w.is_open_folder(live));
    parent.file("child/after-going-up.txt", "x");
    assert!(saw_a_create(fd, live), "going up removed the watch the expanded child still uses");
}

// Going into an expanded child reuses its descriptor as the open folder's, so only the parent's goes.
#[test]
fn going_into_an_expanded_child_keeps_its_watch_and_drops_the_parent() {
    let parent = crate::backend::testdir::TestDir::new("watch-in");
    let child = parent.dir("child");
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    w.begin(parent.path());
    let expanded = Watch::add_raw(fd, &child);
    w.commit(vec![expanded]);
    let root = w.wd;
    assert!(root >= 0 && expanded >= 0 && root != expanded, "the sandbox could not be watched");
    w.begin(&child);
    assert_eq!(w.incoming, expanded, "the child aliases onto its expanded descriptor");
    w.commit(Vec::new());
    assert_eq!(ignored_wds(fd), vec![root], "only the parent's watch goes");
    assert!(w.is_open_folder(expanded));
    parent.file("child/after-going-in.txt", "x");
    assert!(saw_a_create(fd, expanded), "going in removed the watch the child already had");
}

// A collapse is a re-list without the child, so only its descriptor goes; a failed scan of a shown child takes none.
#[test]
fn a_collapse_drops_only_its_child_and_a_stop_drops_the_rest() {
    let parent = crate::backend::testdir::TestDir::new("watch-collapse");
    let (a, b) = (parent.dir("a"), parent.dir("b"));
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    w.begin(parent.path());
    let (wa, wb) = (Watch::add_raw(fd, &a), Watch::add_raw(fd, &b));
    w.commit(vec![wa, wb]);
    let root = w.wd;
    assert!(root >= 0 && wa >= 0 && wb >= 0, "the sandbox could not be watched");
    w.begin(&a);
    w.abandon();
    assert!(ignored_wds(fd).is_empty(), "an abandoned scan of an expanded child removed its watch");
    w.abandon_wd(Watch::add_raw(fd, &b));
    assert!(ignored_wds(fd).is_empty(), "a late scan of an expanded child removed its watch");
    w.begin(parent.path());
    w.commit(vec![wa]);
    assert_eq!(ignored_wds(fd), vec![wb], "collapsing b removes b alone");
    assert!(w.is_current(root) && w.is_current(wa) && !w.is_current(wb));
    w.stop();
    let mut gone = ignored_wds(fd);
    gone.sort_unstable();
    let mut want = vec![root, wa];
    want.sort_unstable();
    assert_eq!(gone, want, "a stop removes the folder and every child still expanded");
    assert!(!w.is_current(root) && !w.is_current(wa));
}

// No descriptor here: an expanded child is current, never the open folder, and an unwatchable one is neither.
#[test]
fn an_expanded_child_is_current_but_not_the_open_folder() {
    let mut w = Watch { outline: Vec::new(), fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: 9, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    w.commit(vec![11, -1]);
    assert!(w.is_open_folder(9) && w.is_current(9));
    assert!(w.is_current(11) && !w.is_open_folder(11));
    assert!(!w.is_current(7) && !w.is_current(-1) && !w.is_open_folder(-1));
}

// An expanded child's volume going re-lists the open folder in place; only the folder's own unmount leaves it.
#[test]
fn an_expanded_childs_unmount_re_lists_and_only_the_folders_own_leaves() {
    let (tx, _rx) = crate::backend::events::channel();
    let w = Watch { outline: vec![9], fd: (-1).into(), events: tx.clone(), wd: 7, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    let poller = crate::backend::watchpoll::Poller::new(tx);
    let base = Path::new("/run/media/gm");
    let said = |wd: i32| {
        let mut out = Vec::<u8>::new();
        crate::backend::unmount::leave_mount(&mut out, base, &w, &poller, wd);
        String::from_utf8(out).expect("the wire is utf-8")
    };
    assert_eq!(said(9), format!("{}\n", changed_line(base)), "a child's unmount re-lists and stays");
    assert!(said(7).starts_with(r#"{"t":"unmounted","path":"/run/media/gm""#), "the folder's own unmount still leaves");
    assert_eq!(said(5), "", "a descriptor that is not the listing's says nothing");
}

// Arming the next scan over an incoming that aliases a shown child, or over itself, must leave that descriptor watched.
#[test]
fn arming_over_an_aliased_incoming_keeps_its_watch() {
    let parent = crate::backend::testdir::TestDir::new("watch-rearm");
    let (a, b) = (parent.dir("a"), parent.dir("b"));
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    w.begin(parent.path());
    let wa = Watch::add_raw(fd, &a);
    w.commit(vec![wa]);
    w.set_incoming(Watch::add_raw(fd, &a), std::path::PathBuf::new());
    assert!(wa >= 0 && w.incoming == wa, "the scan of a shown child aliases onto its descriptor");
    let wb = Watch::add_raw(fd, &b);
    w.set_incoming(wb, std::path::PathBuf::new());
    assert!(ignored_wds(fd).is_empty(), "arming the next scan removed the shown child's watch");
    w.set_incoming(wb, std::path::PathBuf::new());
    assert!(ignored_wds(fd).is_empty(), "arming one descriptor twice removed it");
    assert!(w.is_current(wa) && w.incoming == wb);
}

// Arming the next scan over an incoming nothing shows removes that one, so re-arming leaks no watch.
#[test]
fn arming_over_an_unused_incoming_drops_it() {
    let parent = crate::backend::testdir::TestDir::new("watch-rearm-unused");
    let (a, b) = (parent.dir("a"), parent.dir("b"));
    let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut w = Watch { outline: Vec::new(), fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
    w.begin(parent.path());
    w.commit(Vec::new());
    let root = w.wd;
    let wa = Watch::add_raw(fd, &a);
    w.set_incoming(wa, std::path::PathBuf::new());
    let wb = Watch::add_raw(fd, &b);
    assert!(root >= 0 && wa >= 0 && wb >= 0 && wa != wb, "the sandbox could not be watched");
    w.set_incoming(wb, std::path::PathBuf::new());
    assert_eq!(ignored_wds(fd), vec![wa], "arming the next scan kept the watch it replaced");
    assert!(w.is_open_folder(root) && w.incoming == wb);
}
