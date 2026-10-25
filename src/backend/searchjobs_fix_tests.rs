// Worker-level search tests that need the delta harness: a sort mid-query and a panicking worker.
use super::delta_tests::{kinds, Loop};
use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn search_fix4_discard_preserves_an_announced_walk_after_draining_the_landing() {
    let root = TestDir::new("search-discard-landed");
    root.dir("scope");
    root.file("scope/alpha.txt", "a");
    let mut lp = Loop::open(&root, "alpha", 10);
    assert!(lp.session().announced, "the first landing has been consumed");
    let query = |gen| Event::Request(format!(
        r#"{{"c":"searchquery","query":"alpha","gen":{gen},"count":10}}"#
    ));
    lp.route(query(2));
    lp.route(query(3));
    lp.ready();
    assert!(lp.said().is_empty(), "the superseded answer is discarded");
    let mut landings = 1;
    while let Some(event) = lp.buffered.pop_front() {
        let announced = lp.session().announced;
        lp.route(event);
        if lp.gate.busy() { lp.ready(); }
        if !announced && lp.session().announced { landings += 1; }
    }
    // A genuine trailing wake must also be silent after the already accepted landing.
    lp.route(Event::SearchWalked);
    lp.ready();
    let said = lp.said();
    assert_eq!(kinds(&said), ["listed", "rows", "searched"], "{said:?}");
    assert!(said.iter().all(|line| crate::json::field_usize(line, "gen") == Some(3)));
    assert_eq!(landings, 1, "discard must not make the same walk land twice");
    lp.close();
}

#[test]
fn a_sort_arriving_during_a_query_still_publishes_that_generation() {
    let root = TestDir::new("search-sort-inflight");
    root.dir("scope");
    for i in 0..3 {
        root.file(&format!("scope/alpha{i}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 10);
    assert!(lp
        .route(Event::Request(
            r#"{"c":"searchquery","query":"alpha","gen":2,"count":10}"#.into()
        ))
        .is_none());
    assert!(lp
        .route(Event::Request(r#"{"c":"sort","by":"name"}"#.into()))
        .is_none());
    lp.ready();
    let published = lp.said();
    assert_eq!(kinds(&published), ["listed", "rows", "searched"]);
    assert!(published
        .iter()
        .all(|line| crate::json::field_usize(line, "gen") == Some(2)));
    let sort = lp
        .buffered
        .pop_front()
        .expect("the sort replays after the answer it arrived behind");
    assert!(matches!(&sort, Event::Request(line) if line.contains(r#""c":"sort""#)));
    assert!(lp.buffered.is_empty(), "no walk landing replays for a sort");
    lp.close();
}

#[test]
fn a_panicking_search_worker_still_answers_so_the_gate_is_free_and_says_why() {
    let root = TestDir::new("search-worker-panic");
    root.dir("scope");
    for i in 0..3 {
        root.file(&format!("scope/alpha{i}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 2);
    let damaged = std::sync::Arc::clone(&lp.session().index);
    let probes = std::sync::Arc::clone(&lp.session().probes);
    assert_eq!(lp.session().pending_rest_len(), 1);
    assert!(lp
        .route(Event::Request(format!(
            r#"{{"c":"searchquery","query":"{INJECTED_PANIC_QUERY}","gen":2,"count":10}}"#
        )))
        .is_none());
    lp.ready();
    assert!(!lp.gate.busy(), "the gate is free once the panicked worker answered");
    let said = lp.said();
    assert_eq!(kinds(&said), ["error"], "{said:?}");
    assert!(said[0].contains(r#""where":"search""#), "{}", said[0]);
    assert!(damaged.is_poisoned(), "the panic unwound while holding the index");
    assert_eq!(damaged.lock().err().unwrap().into_inner().len(), 4, "the panic followed an index write");
    assert!(probes.cancel.load(std::sync::atomic::Ordering::Relaxed), "the failed walk is stopped");
    let recovered = lp.session();
    assert!(!std::sync::Arc::ptr_eq(&damaged, &recovered.index));
    assert!(!recovered.index.is_poisoned());
    assert_eq!(recovered.pending_rest_len(), 0, "old row ids never enter the rebuilt index");
    assert_eq!(recovered.last_answered_gen, 1, "the last accepted generation survives recovery");
    recovered.join_walk();
    // The rebuilt session must answer its next query with that query's own generation and no phantom row.
    assert!(lp
        .route(Event::Request(
            r#"{"c":"searchquery","query":"alpha","gen":3,"count":10}"#.into()
        ))
        .is_none());
    lp.ready();
    let after = lp.said();
    assert_eq!(kinds(&after), ["listed", "rows", "searched"], "{after:?}");
    assert!(after
        .iter()
        .all(|line| crate::json::field_usize(line, "gen") == Some(3)), "{after:?}");
    assert_eq!(crate::json::field_usize(&after[2], "n"), Some(3), "{after:?}");
    lp.close();
}
