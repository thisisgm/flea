use super::*;
use crate::backend::testdir::TestDir;
use std::sync::atomic::Ordering;

#[test]
fn search_growth_while_building_cannot_narrow_a_partial_candidate_set() {
    let root = TestDir::new("searchgrowth");
    root.dir("held");
    let scope = root.path().to_str().unwrap();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("held");
    let (events, _received) = crate::backend::events::channel();
    let mut session = Session::open_async_with_probes(scope, scope, false, events, Arc::clone(&probes));
    parked.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
    session.index.lock().unwrap().push("ab", false);
    assert_eq!(session.query(1, "a", 10).unwrap().total, 1);
    session.index.lock().unwrap().push("abc", false);
    assert_eq!(session.query(2, "ab", 10).unwrap().total, 2);
    release.send(()).unwrap();
    session.join_walk();
    assert_eq!(session.query(3, "abc", 10).unwrap().total, 1);
}

#[test]
fn search_published_growth_invalidates_narrowing_even_after_cancellation() {
    let root = TestDir::new("searchepoch");
    let path = root.path().to_str().unwrap();
    let mut session = Session::open_sync(path, path, false);
    session.index.lock().unwrap().push("ab", false);
    assert_eq!(session.query(1, "a", 10).unwrap().total, 1);
    session.index.lock().unwrap().push("abc", false);
    session.probes.cancel.store(true, Ordering::Relaxed);
    assert_eq!(session.query(2, "ab", 10).unwrap().total, 2);
}

#[test]
fn search_streams_completed_blocks_before_the_last_directory_lands() {
    let root = TestDir::new("searchstream");
    root.dir("held");
    root.file("alpha.txt", "a");
    let scope = root.path().to_str().unwrap();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("held");
    let (events, _received) = crate::backend::events::channel();
    let mut session = Session::open_async_with_probes(scope, scope, false, events, probes);
    parked.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
    let answer = session.query(1, "a", 10).unwrap();
    assert!(answer.walking);
    session.index.lock().unwrap().push("another.txt", false);
    session.test_set_last_batch(Instant::now() - std::time::Duration::from_millis(101));
    let streamed = session.poll_walked();
    assert!(streamed, "completed blocks must refresh the standing partial query before final readiness");
    // No query waited under the cap, so the standing re-answer is the path that answers the streamed block.
    assert!(session.answer_held().is_none(), "no held query stands, so the held path must answer nothing");
    let refreshed = session.re_answer_standing().expect("the standing partial query is re-answered");
    assert_eq!(refreshed.gen, 1);
    assert_eq!(refreshed.total, 2, "the re-answer counts the completed block's row");
    assert!(refreshed.rows.iter().any(|row| row.0 == "another.txt"), "the re-answer names the streamed row");
    release.send(()).unwrap();
    session.join_walk();
}
