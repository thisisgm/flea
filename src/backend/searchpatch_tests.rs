use crate::backend::searchsession::Session;
use crate::backend::searchwalk::WalkProbes;
use crate::backend::searchwalkhold::{is_delta, wait_walk_landed};
use crate::backend::testdir::TestDir;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

fn scope_of(d: &TestDir) -> String {
    d.path().to_str().expect("utf-8").to_string()
}

#[test]
fn a_query_asked_while_a_cancelled_walk_winds_down_does_not_claim_a_landing() {
    let d = TestDir::new("cancelwinding");
    for i in 0..4 {
        d.dir(&format!("d{}", i));
        d.file(&format!("bench-{}.txt", i), "");
    }
    let scope = scope_of(&d);
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(2);
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks on its second directory");
    s.cancel_walk().expect("a running walk cancels");
    // The walk thread is still parked, as a remote read past its cancel would leave it.
    let answer = s.query(1, "bench", 100).expect("generation 1 answers");
    assert!(!answer.walking, "a cancelled walk never lands a re-run, so no answer may say it still runs");
    release.send(()).expect("the parked walk takes its release");
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), is_delta);
    assert!(!s.poll_walked(), "the landing of a cancelled walk is swallowed");
    assert!(s.landing().is_none(), "nothing is left to re-run, so the client would wait for ever");
}

#[test]
fn an_own_write_into_a_directory_the_walk_has_not_read_adds_its_rows_once() {
    let d = TestDir::new("walkpatch");
    d.dir("sub");
    d.file("sub/a.txt", "");
    let scope = scope_of(&d);
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("sub");
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks as it opens sub");
    let added = d.file("sub/new.txt", "");
    s.patch_after_write(&added);
    release.send(()).expect("the parked walk takes its release");
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), is_delta);
    assert!(s.poll_walked(), "the landing announces once");
    let answer = s.query(1, "txt", 100).expect("generation 1 answers");
    assert_eq!(answer.total, 2, "the walk and the patch both read sub, and each file is one row");
    assert_eq!(s.indexed(), 3, "sub, sub/a.txt and sub/new.txt");
}

#[test]
fn a_paste_of_many_items_into_one_folder_patches_that_folder_once() {
    let d = TestDir::new("patchbatch");
    d.dir("sub");
    let landed: Vec<PathBuf> = (0..20).map(|i| d.file(&format!("sub/f{:02}.txt", i), "")).collect();
    let scope = scope_of(&d);
    let mut s = Session::open_sync(&scope, &scope, false);
    let before = s.index.lock().unwrap().scanned.load(Ordering::Relaxed);
    assert!(s.patch_after_writes(&landed), "the landed items are under the scope");
    let scanned = s.index.lock().unwrap().scanned.load(Ordering::Relaxed) - before;
    assert_eq!(scanned, 20, "twenty landed items inspect only their directory block");
}

#[test]
fn a_session_asks_the_bound_for_its_roots_figures_rather_than_statting_on_the_loop() {
    crate::backend::iomount::test_reset();
    let d = TestDir::new("openbound");
    d.file("a.txt", "");
    let scope = scope_of(&d);
    let s = Session::open_sync(&scope, &scope, false);
    assert_eq!(crate::backend::iomount::test_calls(), 1, "a dead server must cost the bound's deadline, not the loop");
    assert!(s.dev != 0 && s.writable, "a live root still answers its own figures");
}

// A walk parked in sub, with done already indexed and its old.txt renamed to new.txt on disk and in the patch queue.
fn renamed_behind_a_parked_walk(
    tag: &str,
) -> (TestDir, Session, std::sync::mpsc::Receiver<crate::backend::events::Event>, std::sync::mpsc::Sender<()>) {
    let d = TestDir::new(tag);
    d.dir("done");
    d.dir("sub");
    let old = d.file("done/old.txt", "");
    d.file("sub/x.txt", "");
    let scope = scope_of(&d);
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("sub");
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks as it opens sub");
    for _ in 0..1000 {
        if s.indexed() == 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(s.indexed(), 3, "done, sub and done/old.txt are indexed before the rename");
    let new = d.join("done/new.txt");
    std::fs::rename(&old, &new).expect("the sandbox renames");
    assert!(!s.patch_after_write(&new), "a patch behind a running walk waits");
    (d, s, walked_rx, release)
}

#[test]
fn a_patch_that_waited_for_the_walk_applies_at_its_landing() {
    let (_d, mut s, walked_rx, release) = renamed_behind_a_parked_walk("patchlanding");
    release.send(()).expect("the parked walk takes its release");
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), is_delta);
    assert!(s.poll_walked(), "the landing announces once");
    assert_eq!(s.query(1, "new", 100).expect("generation 1 answers").total, 1, "the renamed file is indexed");
    assert_eq!(s.query(2, "old", 100).expect("generation 2 answers").total, 0, "the old name is gone");
}

#[test]
fn a_patch_that_waited_for_the_walk_applies_when_the_walk_is_cancelled() {
    let (_d, mut s, _walked_rx, release) = renamed_behind_a_parked_walk("patchcancel");
    s.cancel_walk().expect("a running walk cancels");
    assert_eq!(s.query(1, "new", 100).expect("generation 1 answers").total, 1, "the renamed file is indexed");
    assert_eq!(s.query(2, "old", 100).expect("generation 2 answers").total, 0, "the old name is gone");
    drop(release);
}

#[test]
fn a_walker_that_has_read_a_directory_pushes_nothing_once_the_walk_is_cancelled() {
    let d = TestDir::new("cancellate");
    d.dir("sub");
    d.file("sub/x.txt", "");
    let scope = scope_of(&d);
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_late_in("sub");
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks with sub read and not yet pushed");
    let added = d.file("sub/new.txt", "");
    s.patch_after_write(&added);
    s.cancel_walk().expect("a running walk cancels");
    release.send(()).expect("the parked walk takes its release");
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), is_delta);
    let answer = s.query(1, "txt", 100).expect("generation 1 answers");
    assert_eq!(answer.total, 2, "the patch read sub once and the cancelled walk added nothing to it");
    assert_eq!(s.indexed(), 3, "sub, sub/x.txt and sub/new.txt");
}

#[test]
fn a_change_the_landing_applies_re_runs_an_answer_given_after_the_walk() {
    for held in [false, true] {
        let d = TestDir::new("landingdelta");
        d.dir("deep/child");
        let scope = scope_of(&d);
        let (tx, rx) = crate::backend::events::channel();
        let probes = Arc::new(WalkProbes::new());
        let (parked, release) = probes.test_hold_walk_in("child");
        let mut s = Session::open_async_with_probes(&scope, &scope, false, tx.clone(), probes);
        parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks before reading child");
        if held { assert!(s.ask(41, "needle", 3, &tx).is_none()); }
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
                Ok(crate::backend::events::Event::SearchWalked) => break,
                Ok(crate::backend::events::Event::SearchHeld | crate::backend::events::Event::SearchDelta(_)) => {},
                _ => panic!("the walk did not land"),
            }
        }
        // A hold timer can answer after the walk finishes but before the loop processes its landing.
        let answered = if held { s.answer_held() } else { s.ask(41, "needle", 3, &tx) }.unwrap();
        assert_eq!((answered.gen, answered.total, answered.walking), (41, 0, false));
        assert!(s.landing().is_none(), "the completed answer does not itself need a re-run");
        d.file("deep/child/needle-a", "a");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
                Ok(crate::backend::events::Event::SearchDelta(token)) => {
                    assert_eq!(token, s.watch_token(), "the pending change belongs to this session");
                    break;
                }
                Ok(crate::backend::events::Event::SearchHeld) => {},
                _ => panic!("the external create did not publish its watch delta"),
            }
        }
        assert!(s.poll_walked(), "the landing announces once");
        assert_eq!(s.landing(), Some((41, "needle".to_string(), 3)), "the landing arms the standing re-run branch");
        assert!(s.answer_held().is_none(), "the already answered query is no longer held");
        let landed = s.re_answer_standing().expect("the landing's change re-runs the standing query");
        assert_eq!((landed.gen, landed.total, landed.walking), (41, 1, false));
        assert_eq!(landed.rows[0].0, "deep/child/needle-a");
        assert!(s.landing().is_none(), "one re-run");
        assert!(!s.poll_walked(), "one landing");
        assert!(!s.apply_watch_deltas(), "the landing already took the delta");
    }
}
