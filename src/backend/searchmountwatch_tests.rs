use super::*;
use crate::backend::searchindex::Index;
use crate::backend::searchwalkhold::{is_delta, wait_walk_landed};

#[test]
fn search_mount_patch_and_replacement_never_arm_nested_remote_directories() {
    let d = TestDir::new("nestedwatch");
    d.dir("nas/deep");
    d.file("nas/deep/needle", "");
    d.dir("local");
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - fuse.sshfs sshfs rw\n", d.join("nas").display());
    let (tx, _) = crate::backend::events::channel();
    let watch = Watch::new(d.path(), false, tx);
    let mut index = Index::new();
    assert!(index.patch_watched(d.path(), Path::new(""), false, &body, Some(&watch.core)));
    assert!(watch.core.state.lock().unwrap().paths.contains_key(Path::new("local")));
    assert!(!watch.core.state.lock().unwrap().paths.contains_key(Path::new("nas")), "patch descent must classify before inotify_add_watch");
    assert!(!watch.core.state.lock().unwrap().paths.contains_key(Path::new("nas/deep")));
    assert!(index.reset_tree(d.path(), Path::new("nas"), false, &body, Some(&watch.core)));
    assert!(index.patch_watched(d.path(), Path::new("nas"), false, &body, Some(&watch.core)));
    assert!(!watch.core.state.lock().unwrap().paths.contains_key(Path::new("nas")), "replacement and explicit patch hooks share the boundary rule");
    assert_eq!(index.len(), 4);
    assert_eq!(index.partial, None);
}

#[test]
fn search_mount_unplug_mid_walk_stops_enumeration_and_invalidates_coverage() {
    let d = TestDir::new("unplugwalk");
    d.dir("usb/deep");
    d.file("usb/deep/needle", "");
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("usb");
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async_with_probes(root, root, false, tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(10)).unwrap();
    let core = Arc::clone(&session.watch.as_ref().unwrap().core);
    let wd = core.state.lock().unwrap().paths[Path::new("usb")];
    core.ingest(&event(wd, UNMOUNT, 0, b""));
    session.apply_watch_deltas();
    assert!(!session.walking(), "unplug must stop a walk before it publishes a complete result");
    assert_eq!(session.partial_reason(), Some("watch_failed"));
    assert!(probes.cancel.load(Ordering::Relaxed));
    assert!(session.watch.is_none(), "unplug releases recursive descriptors");
    assert!(core.state.lock().unwrap().fd.is_none());
    let opened = probes.opened.load(Ordering::Relaxed);
    release.send(()).unwrap();
    wait_walk_landed(&rx, &session, Duration::from_secs(10), is_delta);
    session.join_walk();
    assert_eq!(probes.opened.load(Ordering::Relaxed), opened);
    assert_eq!(session.answer("needle", 10).total, 0);
    assert_eq!(session.partial_reason(), Some("watch_failed"));
}

#[test]
fn search_mount_initial_walk_watches_local_usb_but_not_nested_remote_directories() {
    let d = TestDir::new("initialmountwatch");
    d.dir("usb/deep");
    d.file("usb/deep/needle", "");
    d.dir("nas/deep");
    d.file("nas/deep/needle", "");
    let probes = Arc::new(WalkProbes::new());
    *probes.body.lock().unwrap() = Some(format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs server:/share rw\n31 1 8:2 / {} rw - vfat /dev/usb rw\n", d.join("nas").display(), d.join("usb").display()));
    let (tx, _) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async_with_probes(root, root, false, tx, probes);
    session.join_walk();
    assert_eq!(session.answer("needle", 10).total, 2);
    assert_eq!(session.partial_reason(), None);
    let core = Arc::clone(&session.watch.as_ref().unwrap().core);
    let wd = {
        let state = core.state.lock().unwrap();
        assert!(state.paths.contains_key(Path::new("usb/deep")));
        assert!(!state.paths.contains_key(Path::new("nas")));
        assert!(!state.paths.contains_key(Path::new("nas/deep")));
        state.paths[Path::new("usb")]
    };
    session.tags.hold_read();
    assert!(session.query(1, "tag:Red", 1).unwrap().walking);
    core.ingest(&event(wd, UNMOUNT | IGNORED, 0, b""));
    assert!(session.apply_watch_deltas());
    assert_eq!(session.partial_reason(), Some("watch_failed"));
    let tagged = session.query(2, "tag:Red", 1).unwrap();
    assert!(!tagged.walking, "an unmounted scope must not restart its stopped tag pass");
    assert_eq!(session.tags.reads(), 0);
    session.tags.release_read();
    let answer = session.query(3, "needle", 1).unwrap();
    assert_eq!(answer.partial, Some("watch_failed"));
    assert!(!answer.walking);
    assert_eq!(answer.rows.len(), 1);
    assert!(session.watch.is_none());
}


#[test]
fn search_unmount_refuses_own_write_reconciliation() {
    let d = TestDir::new("unmountpatch");
    d.dir("usb");
    d.file("usb/before", "");
    let (mut session, _) = fresh(&d);
    let core = Arc::clone(&session.watch.as_ref().unwrap().core);
    let wd = core.state.lock().unwrap().paths[Path::new("usb")];
    core.ingest(&event(wd, UNMOUNT, 0, b""));
    assert!(session.apply_watch_deltas());
    assert_eq!(session.partial_reason(), Some("watch_failed"));
    let new = d.file("usb/exposed-underlying-file", "");
    crate::backend::iomount::test_reset();
    let patched = session.patch_after_write(&new);
    let answer = session.answer("exposed-underlying-file", 10);
    eprintln!("SEARCH_UNMOUNT post_unmount patched={patched} exposed_matches={}", answer.total);
    assert_eq!(answer.total, 0, "an unmounted scope must not enumerate the exposed mountpoint through an own-write hook");
    assert!(!patched);
    assert!(!session.patch_open_dir(&d.join("usb")));
    assert!(!session.patch(Path::new("usb")));
    assert!(!session.flush_patches());
    assert_eq!(crate::backend::iomount::test_calls(), 0, "no post-unmount metadata probe or directory reconciliation");
    let (mut reopened, _) = fresh(&d);
    assert_eq!(reopened.answer("exposed-underlying-file", 10).total, 1, "reopening permits the fresh scope to enumerate again");
}

#[test]
fn search_unmount_discards_waiting_patches_at_walk_landing() {
    let d = TestDir::new("unmountpending");
    d.dir("usb");
    d.file("usb/before", "");
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_late_in("usb");
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async_with_probes(root, root, false, tx, probes);
    parked.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!session.patch_after_write(&d.join("usb/before")));
    release.send(()).unwrap();
    wait_walk_landed(&rx, &session, Duration::from_secs(10), is_delta);
    let core = Arc::clone(&session.watch.as_ref().unwrap().core);
    let wd = core.state.lock().unwrap().paths[Path::new("usb")];
    core.ingest(&event(wd, UNMOUNT, 0, b""));
    d.file("usb/exposed-underlying-file", "");
    assert!(session.poll_walked());
    assert!(session.pending_patches.is_empty());
    let answer = session.answer("exposed-underlying-file", 10);
    eprintln!("SEARCH_UNMOUNT pending_patch exposed_matches={}", answer.total);
    assert_eq!(answer.total, 0, "unmount observed at walk landing must discard pending patches before reconciliation");
}


#[test]
fn search_unmount_completes_empty_standing_query_once() {
    check_empty_unmount(true);
    check_empty_unmount(false);
}

fn check_empty_unmount(walking: bool) {
    let d = TestDir::new("unmountempty");
    d.dir("usb");
    let probes = Arc::new(WalkProbes::new());
    let held = walking.then(|| probes.test_hold_walk_in("usb"));
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async_with_probes(root, root, false, tx, probes);
    if let Some((parked, _)) = &held {
        parked.recv_timeout(Duration::from_secs(10)).unwrap();
    } else {
        session.join_walk();
    }
    assert!(session.claim_gen(41, "", 1));
    session.note_folder_count(7);
    let core = Arc::clone(&session.watch.as_ref().unwrap().core);
    let wd = core.state.lock().unwrap().paths[Path::new("usb")];
    core.ingest(&event(wd, UNMOUNT, 0, b""));
    let (events, _) = crate::backend::events::channel();
    let mut state = crate::backend::state::State::new(crate::backend::dirsizeworker::Worker::new(events));
    state.base = d.path().to_path_buf();
    state.held = crate::backend::state::Held::Walk;
    for row in 0..7 { state.listing.push(&format!("folder-row{row}"), false); }
    let listing_generation = state.generation;
    state.search_session = Some(session);
    let tables = crate::backend::state::Tables::load();
    let (thumbs, _) = mpsc::channel();
    let pool = crate::backend::thumbs::Pool::new(1, thumbs, d.dir("thumbs"),
        Arc::clone(&tables.aliases), Arc::clone(&tables.thumbs));
    let mut out = Vec::new();
    crate::backend::searchdelta::on_delta(&mut out, &mut state, &tables, &pool, core.token);
    crate::backend::searchdelta::on_delta(&mut out, &mut state, &tables, &pool, core.token);
    if let Some((_, release)) = held {
        release.send(()).unwrap();
        let walking = state.search_session.as_ref().expect("a session stands");
        wait_walk_landed(&rx, walking, Duration::from_secs(10), is_delta);
    }
    let session = state.search_session.as_mut().unwrap();
    let later_landing = session.poll_walked();
    assert!(!later_landing);
    assert!(session.landing_empty().is_none(), "the status is spent exactly once");
    assert_eq!(state.listing.len(), 7);
    assert_eq!(state.listing.name(0), "folder-row0");
    assert_eq!(state.generation, listing_generation);
    assert_eq!(state.base, d.path());
    assert_eq!(state.held, crate::backend::state::Held::Walk);
    let wire = String::from_utf8(out).unwrap();
    eprintln!("SEARCH_UNMOUNT empty_query wire={wire:?} later_landing={later_landing}");
    assert!(wire.contains("watch_failed"), "an empty-query client must receive terminal partial status when its scope unmounts");
    assert_eq!(wire.lines().count(), 1, "no replacement rows or duplicate status");
    assert_eq!(crate::json::field_str(&wire, "t").as_deref(), Some("searched"));
    assert_eq!(crate::json::field_usize(&wire, "gen"), Some(41));
    assert_eq!(crate::json::field_usize(&wire, "n"), Some(7));
    assert_eq!(crate::json::field_usize(&wire, "scanned"), Some(7));
    assert!(!crate::json::field_bool(&wire, "walking"));
    assert!(crate::json::field_bool(&wire, "partial"));
    assert!(!crate::json::field_bool(&wire, "cancelled"));
}


#[test]
fn search_unmount_stops_inflight_tag_reads_before_next_path() {
    let d = TestDir::new("unmounttags");
    d.dir("usb");
    for row in 0..20 { d.file(&format!("usb/file{row}"), ""); }
    let (mut session, _) = fresh(&d);
    let hold = crate::backend::tagreadprobe::Hold::new(d.join("usb"));
    assert!(session.query(1, "tag:Red", 10).unwrap().walking);
    hold.wait();
    let core = Arc::clone(&session.watch.as_ref().unwrap().core);
    let wd = core.state.lock().unwrap().paths[Path::new("usb")];
    core.ingest(&event(wd, UNMOUNT, 0, b""));
    assert!(session.apply_watch_deltas());
    hold.release();
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.tags.reads() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let reads = hold.reads();
    eprintln!("SEARCH_UNMOUNT tag_reads_after_unmount={}", reads.saturating_sub(1));
    assert_eq!(reads, 1, "a stopped tag pass must not start further per-file calls when its first in-flight call returns");
}

#[test]
fn search_user_cancel_still_reconciles_own_writes_and_open_directory_changes() {
    let d = TestDir::new("cancelpatches");
    d.dir("usb");
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("usb");
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async_with_probes(root, root, false, tx, probes);
    parked.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(session.cancel_walk().is_some());
    let written = d.file("usb/after-cancel-write", "");
    assert!(session.patch_after_write(&written));
    d.file("usb/after-cancel-open", "");
    assert!(session.patch_open_dir(&d.join("usb")));
    assert_eq!(session.answer("after-cancel", 10).total, 2);
    release.send(()).unwrap();
    wait_walk_landed(&rx, &session, Duration::from_secs(10), is_delta);
    assert_eq!(session.answer("after-cancel", 10).total, 2);
}
