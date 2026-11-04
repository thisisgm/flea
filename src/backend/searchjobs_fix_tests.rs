// Worker-level search tests that need the delta harness: a sort mid-query and a panicking worker.
use super::delta_tests::{kinds, Loop};
use super::*;
use crate::backend::testdir::TestDir;

thread_local! {
    static STAT_PATHS: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

impl super::super::searchrowcache::Cache {
    pub(crate) fn test_record_stat_path(path: &std::path::Path) {
        STAT_PATHS.with(|paths| paths.borrow_mut().push(path.to_path_buf()));
    }
}

#[test]
fn search_fix4_discard_preserves_an_announced_walk_after_draining_the_landing() {
    let root = TestDir::new("search-discard-landed");
    root.dir("scope");
    root.file("scope/alpha.txt", "a");
    let mut lp = Loop::open(&root, "alpha", 10);
    assert!(
        lp.session().announced,
        "the first landing has been consumed"
    );
    let query = |gen| {
        Event::Request(format!(
            r#"{{"c":"searchquery","query":"alpha","gen":{gen},"count":10}}"#
        ))
    };
    lp.route(query(2));
    lp.route(query(3));
    lp.ready();
    assert!(lp.said().is_empty(), "the superseded answer is discarded");
    let mut landings = 1;
    while let Some(event) = lp.buffered.pop_front() {
        let announced = lp.session().announced;
        lp.route(event);
        if lp.gate.busy() {
            lp.ready();
        }
        if !announced && lp.session().announced {
            landings += 1;
        }
    }
    // A genuine trailing wake must also be silent after the already accepted landing.
    lp.route(Event::SearchWalked);
    lp.ready();
    let said = lp.said();
    assert_eq!(kinds(&said), ["listed", "rows", "searched"], "{said:?}");
    assert!(said
        .iter()
        .all(|line| crate::json::field_usize(line, "gen") == Some(3)));
    assert_eq!(
        landings, 1,
        "discard must not make the same walk land twice"
    );
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
    assert!(
        !lp.gate.busy(),
        "the gate is free once the panicked worker answered"
    );
    let said = lp.said();
    assert_eq!(kinds(&said), ["error"], "{said:?}");
    assert!(said[0].contains(r#""where":"search""#), "{}", said[0]);
    assert!(
        damaged.is_poisoned(),
        "the panic unwound while holding the index"
    );
    assert_eq!(
        damaged.lock().err().unwrap().into_inner().len(),
        4,
        "the panic followed an index write"
    );
    assert!(
        probes.cancel.load(std::sync::atomic::Ordering::Relaxed),
        "the failed walk is stopped"
    );
    let recovered = lp.session();
    assert!(!std::sync::Arc::ptr_eq(&damaged, &recovered.index));
    assert!(!recovered.index.is_poisoned());
    assert_eq!(
        recovered.pending_rest_len(),
        0,
        "old row ids never enter the rebuilt index"
    );
    assert_eq!(
        recovered.last_answered_gen, 1,
        "the last accepted generation survives recovery"
    );
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
    assert!(
        after
            .iter()
            .all(|line| crate::json::field_usize(line, "gen") == Some(3)),
        "{after:?}"
    );
    assert_eq!(
        crate::json::field_usize(&after[2], "n"),
        Some(3),
        "{after:?}"
    );
    lp.close();
}

#[test]
fn search_fb5_typed_generations_reuse_the_requested_rich_rows() {
    use crate::jsondoc::{self, Json};
    fn rich_rows(said: &[String], gen: usize) -> (Vec<Json>, Vec<Json>) {
        assert!(
            said.iter()
                .all(|line| crate::json::field_usize(line, "gen") == Some(gen)),
            "{said:?}"
        );
        let document = jsondoc::parse(&said[1]).unwrap();
        let kinds = document
            .get("kinds")
            .and_then(Json::as_array)
            .unwrap()
            .to_vec();
        let mut rows = document
            .get("rows")
            .and_then(Json::as_array)
            .unwrap()
            .to_vec();
        rows.sort_by_key(|row| row.get("n").and_then(Json::as_str).unwrap().to_owned());
        assert_eq!(
            rows.len(),
            2,
            "the reused window must carry both complete rows"
        );
        for (row, (name, size)) in rows.iter().zip([("alpha.txt", "1"), ("alphabet.txt", "2")]) {
            assert_eq!(row.get("n").and_then(Json::as_str), Some(name));
            assert_eq!(row.get("s"), Some(&Json::Num(size.into())));
            let Some(Json::Num(kind)) = row.get("k") else {
                panic!("a row carries its kind index")
            };
            let kind: usize = kind.parse().expect("a kind index is an unsigned integer");
            assert!(
                kinds
                    .get(kind)
                    .and_then(Json::as_str)
                    .is_some_and(|name| !name.is_empty()),
                "a kind index names a nonempty response-local kind"
            );
            assert_eq!(row.get("d"), Some(&Json::Bool(false)));
        }
        (rows, kinds)
    }
    let root = TestDir::new("search-generation-cache");
    root.dir("scope");
    root.file("scope/alpha.txt", "a");
    root.file("scope/alphabet.txt", "ab");
    root.file("scope/alphabetical.txt", "abc");
    let mut lp = Loop::open(&root, "alpha", 2);
    let mut previous_rows = None;
    for (gen, query) in [(2, "alpha"), (3, "alph")] {
        super::super::meta::SEARCH_STAT_CALLS.with(|calls| calls.set(0));
        super::super::tags::SEARCH_TAG_CALLS.with(|calls| calls.set(0));
        STAT_PATHS.with(|paths| paths.borrow_mut().clear());
        lp.route(Event::Request(format!(
            r#"{{"c":"searchquery","query":"{query}","gen":{gen},"count":2}}"#
        )));
        lp.ready();
        let said = lp.said();
        assert_eq!(kinds(&said), ["listed", "rows", "searched"], "{said:?}");
        let rows = rich_rows(&said, gen);
        assert_eq!(crate::json::field_usize(&said[2], "n"), Some(3));
        if let Some(previous) = &previous_rows {
            assert_eq!(
                &rows, previous,
                "cached row metadata stays identical across typed generations"
            );
        }
        previous_rows = Some(rows);
        assert_eq!(
            lp.session().row_cache.misses,
            2,
            "the second generation reuses both metadata entries"
        );
        assert_eq!(
            lp.session().row_cache.renders,
            2,
            "the second generation reuses both serialized rows"
        );
        if gen == 3 {
            assert_eq!(
                super::super::meta::SEARCH_STAT_CALLS.with(|calls| calls.get()),
                2,
                "GAP229 validates only the two requested rows"
            );
            let mut paths = STAT_PATHS.with(|paths| std::mem::take(&mut *paths.borrow_mut()));
            paths.sort();
            assert_eq!(
                paths,
                [
                    root.join("scope/alpha.txt"),
                    root.join("scope/alphabet.txt")
                ],
                "validation must visit each requested path once and no unrequested match"
            );
            assert_eq!(
                super::super::tags::SEARCH_TAG_CALLS.with(|calls| calls.get()),
                0
            );
        }
    }
    lp.close();
}

#[test]
fn search_fb5_empty_standing_progress_answers_only_at_the_final_landing() {
    use super::super::searchwalk::WalkProbes;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    let root = TestDir::new("search-empty-progress");
    root.dir("held");
    let scope = root.path().to_str().unwrap();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("held");
    let (events, received) = crate::backend::events::channel();
    let mut session =
        Session::open_async_with_probes(scope, scope, false, events.clone(), Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(session.claim_gen(1, "", 1));
    session.note_folder_count(7);
    session.index.lock().unwrap().push("another.txt", false);
    session.test_set_last_batch(Instant::now() - Duration::from_millis(101));
    assert!(
        session.poll_walked(),
        "the progress tick must take the changed-index branch"
    );
    let Worker { tx, rx, handle } = Worker::new(events);
    tx.send((session, Work::Walked))
        .unwrap_or_else(|_| panic!("send progress"));
    let progress = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(progress.session.walking());
    assert!(
        progress.answer.is_none() && progress.empty.is_none(),
        "progress cannot publish a terminal answer"
    );
    release.send(()).unwrap();
    super::super::searchwalkhold::wait_walk_landed(
        &received,
        &progress.session,
        Duration::from_secs(30),
        |_| true,
    );
    tx.send((progress.session, Work::Walked))
        .unwrap_or_else(|_| panic!("send landing"));
    let landed = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(
        landed.empty.map(|(gen, n, scanned, _)| (gen, n, scanned)),
        Some((1, 7, 7))
    );
    tx.send((landed.session, Work::Walked))
        .unwrap_or_else(|_| panic!("send trailing wake"));
    let trailing = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(
        trailing.answer.is_none() && trailing.empty.is_none(),
        "the landing answers once"
    );
    trailing.session.close();
    drop(tx);
    drop(rx);
    handle.join().unwrap();
}
