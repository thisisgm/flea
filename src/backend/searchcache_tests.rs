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
    let (events, _received) = std::sync::mpsc::channel();
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
