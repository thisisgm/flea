use super::*;
use crate::backend::searchsession::Session;
use crate::backend::searchwalk::WalkProbes;
use crate::backend::testdir::TestDir;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[path = "searchmountwatch_tests.rs"]
mod mount_tests;

fn fresh(d: &TestDir) -> (Session, mpsc::Receiver<Event>) {
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut session = Session::open_async(root, root, false, tx);
    session.join_walk();
    (session, rx)
}

fn settle(session: &mut Session, rx: &mpsc::Receiver<Event>, query: &str, wanted: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        session.apply_watch_deltas();
        if session.answer(query, 100).total == wanted { return; }
        assert!(Instant::now() < deadline, "{query} must reach {wanted}");
        if let Ok(Event::SearchDelta(token)) = rx.recv_timeout(Duration::from_millis(10)) {
            session.apply_watch_token(token);
        }
    }
}

#[test]
fn search_watch_deep_events_apply_before_rerunning_the_same_generation() {
    let d = TestDir::new("recursive");
    d.dir("deep/child");
    let (mut s, rx) = fresh(&d);
    assert_eq!(s.query(9, "needle", 10).unwrap().total, 0);
    let path = d.file("deep/child/needle", "");
    settle(&mut s, &rx, "needle", 1);
    let answer = s.re_answer_now().unwrap();
    assert_eq!((answer.gen, answer.total), (9, 1));
    d.assert_contains(&path);
    std::fs::remove_file(&path).unwrap();
    settle(&mut s, &rx, "needle", 0);
    assert_eq!(s.re_answer_now().unwrap().gen, 9);
    assert_eq!(s.counters.walks_started, 1);
}

#[test]
fn search_watch_mutations_before_and_after_enumeration_are_replayed() {
    for late in [false, true] {
        let d = TestDir::new("watchrace");
        d.dir("deep");
        let old = d.file("deep/old", "");
        let probes = Arc::new(WalkProbes::new());
        let (parked, release) = if late { probes.test_hold_walk_late_in("deep") } else { probes.test_hold_walk_in("deep") };
        let (tx, rx) = crate::backend::events::channel();
        let root = d.path().to_str().unwrap();
        let mut s = Session::open_async_with_probes(root, root, false, tx, probes);
        parked.recv_timeout(Duration::from_secs(10)).unwrap();
        d.assert_contains(&old);
        std::fs::remove_file(&old).unwrap();
        d.file("deep/needle", "");
        release.send(()).unwrap();
        s.join_walk();
        settle(&mut s, &rx, "needle", 1);
        assert_eq!(s.answer("old", 10).total, 0);
        assert_eq!(s.indexed(), 2);
        assert!(s.partial_reason().is_none());
    }
}

fn event(wd: i32, mask: u32, cookie: u32, name: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(wd.to_ne_bytes());
    bytes.extend(mask.to_ne_bytes());
    bytes.extend(cookie.to_ne_bytes());
    bytes.extend((name.len() as u32).to_ne_bytes());
    bytes.extend(name);
    bytes
}

#[test]
fn search_watch_cookies_overflow_and_reused_descriptors_keep_their_boundaries() {
    let d = TestDir::new("watchcookies");
    d.dir("from");
    d.dir("to");
    let (tx, _rx) = crate::backend::events::channel();
    let watch = Watch::new(d.path(), false, tx);
    let core = &watch.core;
    core.arm(Path::new("from"));
    core.arm(Path::new("to"));
    let (from, to) = {
        let state = core.state.lock().unwrap();
        (state.paths[Path::new("from")], state.paths[Path::new("to")])
    };
    let mut bytes = event(from, MOVED_FROM | ISDIR, 7, b"tree\0");
    bytes.extend(event(to, MOVED_TO | ISDIR, 7, b"tree\0"));
    core.ingest(&bytes);
    let delta = core.drain();
    assert!(delta.dirs.contains(Path::new("from")) && delta.dirs.contains(Path::new("to")));
    assert_eq!(delta.resets.len(), 2);
    core.state.lock().unwrap().retired.insert(from);
    core.ingest(&event(from, 0x100, 0, b"old\0"));
    assert!(core.drain().dirs.is_empty());
    core.ingest(&event(from, IGNORED, 0, b""));
    assert!(core.state.lock().unwrap().dirs.contains_key(&from), "retirement barrier must preserve a new registration");
    core.ingest(&event(from, 0x100, 0, b"new\0"));
    assert!(core.drain().dirs.contains(Path::new("from")));
    core.ingest(&event(-1, OVERFLOW, 0, b""));
    let delta = core.drain();
    assert!(delta.overflow && delta.dirs.contains(Path::new("")) && delta.dirs.contains(Path::new("to")));
}

#[test]
fn search_watch_registration_failure_and_overflow_cannot_claim_complete() {
    for overflow in [false, true] {
        let d = TestDir::new("watchfail");
        d.dir("deep");
        let (mut s, rx) = fresh(&d);
        let core = Arc::clone(&s.watch.as_ref().unwrap().core);
        if overflow {
            d.file("deep/needle", "");
            core.flush();
            core.drain();
            core.ingest(&event(-1, OVERFLOW, 0, b""));
        } else {
            core.refuse.store(true, Ordering::Relaxed);
            d.dir("new");
            d.file("new/needle", "");
        }
        settle(&mut s, &rx, "needle", 1);
        s.apply_watch_deltas();
        assert!(s.answer("needle", 10).partial.is_some());
    }
}

#[test]
fn search_watch_close_joins_the_pump_and_rejects_old_session_tokens() {
    let d = TestDir::new("watchclose");
    let (s, _rx) = fresh(&d);
    let token = s.watch_token();
    let core = Arc::clone(&s.watch.as_ref().unwrap().core);
    let started = Instant::now();
    s.close();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(core.state.lock().unwrap().fd.is_none());
    assert!(!core.arm(Path::new("")));
    let (mut next, _) = fresh(&d);
    assert!(!next.apply_watch_token(token));
    next.cancel_walk();
}

#[test]
fn search_watch_mutation_mid_readdir_is_replayed_before_completion() {
    let d = TestDir::new("midscan");
    d.dir("deep");
    for row in 0..100 { d.file(&format!("deep/file{row:03}"), ""); }
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_scan_in("deep", 3);
    let (tx, rx) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut s = Session::open_async_with_probes(root, root, false, tx, probes);
    parked.recv_timeout(Duration::from_secs(10)).unwrap();
    let old = d.join("deep/file099");
    let new = d.join("deep/needle");
    d.assert_contains(&old);
    d.assert_contains(&new);
    std::fs::rename(old, new).unwrap();
    release.send(()).unwrap();
    s.join_walk();
    settle(&mut s, &rx, "needle", 1);
    assert_eq!(s.indexed(), 101);
    assert_eq!(s.answer("file099", 10).total, 0);
    assert!(s.partial_reason().is_none());
}

#[test]
fn search_watch_closing_search_preserves_the_real_pane_and_peek_watches() {
    let d = TestDir::new("watchowners");
    let (tx, rx) = crate::backend::events::channel();
    let mut pane = crate::backend::watch::Watch::start(tx.clone());
    pane.begin(d.path());
    pane.commit(Vec::new());
    let mut peek = crate::backend::peekwatch::PeekWatch::start(tx);
    let wd = crate::backend::watch::Watch::add_raw(peek.raw_fd(), d.path());
    assert!(wd >= 0);
    peek.register(wd, d.path().to_path_buf());
    let (s, _) = fresh(&d);
    s.close();
    d.file("external", "");
    let deadline = Instant::now() + Duration::from_secs(10);
    let (mut pane_seen, mut peek_seen) = (false, false);
    while !(pane_seen && peek_seen) {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap() {
            Event::Changed(wd) => { assert!(pane.is_current(wd)); pane_seen = true; }
            Event::PeekChanged(wd) => { assert_eq!(peek.path_of(wd), Some(d.path())); peek_seen = true; }
            _ => panic!("unexpected watch event"),
        }
    }
    pane.stop();
}

#[test]
fn search_watch_cancel_mid_scan_joins_and_releases_recursive_descriptors() {
    let d = TestDir::new("watchcancel");
    d.dir("deep");
    d.file("deep/file", "");
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("deep");
    let (tx, _) = crate::backend::events::channel();
    let root = d.path().to_str().unwrap();
    let mut s = Session::open_async_with_probes(root, root, false, tx, probes);
    parked.recv_timeout(Duration::from_secs(10)).unwrap();
    let core = Arc::clone(&s.watch.as_ref().unwrap().core);
    s.cancel_walk().unwrap();
    assert!(core.state.lock().unwrap().fd.is_none());
    assert!(!core.arm(Path::new("deep")));
    release.send(()).unwrap();
    assert_eq!(s.partial_reason(), Some("cancelled"));
}

#[test]
fn search_watch_scoped_reader_preserves_explicit_roots_and_refuses_descendant_symlinks() {
    let d = TestDir::new("watchsymlink");
    d.dir("target");
    d.file("target/needle", "");
    let alias = d.join("alias");
    std::os::unix::fs::symlink(d.join("target"), &alias).unwrap();
    let chosen = crate::backend::searchread::read_root(&alias).unwrap();
    assert_eq!(chosen.entries, vec![(std::ffi::OsString::from("needle"), false)]);
    assert!(crate::backend::searchread::read(&alias).is_err());
}

#[test]
fn search_watch_row_swaps_discard_the_old_pageable_remainder() {
    let d = TestDir::new("watchrest");
    for row in 0..5 { d.file(&format!("needle{row}"), ""); }
    let (mut s, _rx) = fresh(&d);
    assert_eq!(s.query(6, "needle", 1).unwrap().total, 5);
    assert_eq!(s.pending_rest_len(), 4);
    let gone = d.join("needle0");
    d.assert_contains(&gone);
    std::fs::remove_file(gone).unwrap();
    assert!(s.apply_watch_deltas());
    assert_eq!(s.pending_rest_len(), 0);
    assert!(s.take_rest_chunk(usize::MAX, 1).is_none());
    let current = s.re_answer_now().unwrap();
    assert_eq!((current.gen, current.total), (6, 4));
    assert!(current.rows.iter().all(|(name, _)| name != "needle0"));
}
