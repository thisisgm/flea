// A narrowed answer of a few matches publishes from the loop; a query that rescans still takes the worker.
use super::*;
use crate::backend::searchindex::PARALLEL_SCORE_AT;
use crate::backend::testdir::TestDir;
use std::sync::Arc;
use std::time::Duration;

// Sample input: {"t":"searched","n":400,"scanned":400,"ms":0.212,"cancelled":false,"gen":2,"walking":false,"partial":false,"partial_reason":""}
fn searched(out: &[u8]) -> Vec<(usize, usize)> {
    String::from_utf8_lossy(out)
        .lines()
        .filter(|line| crate::json::field_str(line, "t").as_deref() == Some("searched"))
        .map(|line| {
            (
                crate::json::field_usize(line, "gen").unwrap(),
                crate::json::field_usize(line, "n").unwrap(),
            )
        })
        .collect()
}

#[test]
fn search_a_small_narrowed_query_publishes_from_the_loop_and_a_rescan_takes_the_worker() {
    let root = TestDir::new("search-inline");
    for i in 0..400 {
        root.file(&format!("ab{i:04}.txt"), "a");
    }
    let (events, received) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let scope = root.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    session.query(1, "ab", 2).unwrap();
    st.search_session = Some(session);
    let tb = Tables::load();
    let (results, _done) = channel();
    let pool = Pool::new(
        1,
        results,
        root.dir("thumbs"),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    let mut gate = Gate::default();
    let mut out = Vec::new();
    let mut buffered = VecDeque::new();
    let query = |text: &str, gen: usize| {
        Event::Request(format!(
            r#"{{"c":"searchquery","query":"{text}","gen":{gen},"count":2}}"#
        ))
    };
    assert!(gate
        .route(
            query("ab0", 2),
            &mut st,
            &mut out,
            &tb,
            &pool,
            &events,
            &mut buffered,
            false
        )
        .is_none());
    assert!(
        !gate.busy() && gate.worker.is_none(),
        "400 narrowed matches answer with no worker hop"
    );
    assert_eq!(searched(&std::mem::take(&mut out)), vec![(2, 400)]);
    let session = st.search_session.as_ref().unwrap();
    assert_eq!(
        (session.last_answered_gen, session.pending_rest_len()),
        (2, 398)
    );
    // "zz" does not extend "ab0", so it scores the whole index on the worker.
    assert!(gate
        .route(
            query("zz", 3),
            &mut st,
            &mut out,
            &tb,
            &pool,
            &events,
            &mut buffered,
            false
        )
        .is_none());
    assert!(gate.busy() && gate.worker.is_some());
    let ready = received.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(matches!(ready, Event::SearchReady));
    gate.route(
        ready,
        &mut st,
        &mut out,
        &tb,
        &pool,
        &events,
        &mut buffered,
        false,
    );
    assert_eq!(searched(&out), vec![(3, 0)]);
    assert!(buffered.is_empty());
    gate.route(
        Event::Request(r#"{"c":"searchclose"}"#.into()),
        &mut st,
        &mut out,
        &tb,
        &pool,
        &events,
        &mut buffered,
        false,
    );
    st.search_session.take().unwrap().close();
}

fn opened(root: &TestDir) -> Session {
    let scope = root.path().to_str().unwrap();
    Session::open_sync(scope, scope, false)
}

#[test]
fn search_a_narrowing_of_one_parallel_pass_of_matches_takes_the_worker() {
    let root = TestDir::new("search-inline-size");
    for i in 0..PARALLEL_SCORE_AT - 1 {
        root.file(&format!("ab_{i:04}"), "a");
    }
    root.file("ac_0000", "a");
    // A row the query "a" misses, so a narrowed pass scores fewer rows than the whole index.
    root.file("zz_0000", "z");
    let mut session = opened(&root);
    session.query(1, "a", 2).unwrap();
    assert_eq!(
        (session.indexed(), session.last_matches.len()),
        (PARALLEL_SCORE_AT + 1, PARALLEL_SCORE_AT)
    );
    assert!(
        !session.answers_small("ab"),
        "a narrowing of one parallel pass of matches scores on the worker"
    );
    session.query(2, "ab", 2).unwrap();
    assert_eq!(
        (session.counters.rows_scored, session.last_matches.len()),
        (PARALLEL_SCORE_AT, PARALLEL_SCORE_AT - 1),
        "answer() narrowed it"
    );
    assert!(
        session.answers_small("ab_"),
        "one match fewer stays on the loop"
    );
}

#[test]
fn search_a_narrowing_whose_last_answer_left_out_its_typo_rows_takes_the_worker() {
    let root = TestDir::new("search-inline-typos");
    for i in 0..10 {
        root.file(&format!("abcd{i:04}.txt"), "a");
    }
    let mut session = opened(&root);
    session.query(1, "abcd", 2).unwrap();
    assert!(
        !session.last_typos,
        "exact prefix hits leave the typo rows out"
    );
    assert!(
        !session.answers_small("abcd5"),
        "an extension that loses its exact hits rescans the index for typos"
    );
    let previous = session.last_matches.len();
    session.query(2, "abcd5", 2).unwrap();
    assert!(
        session.counters.rows_scored > previous,
        "answer() took the typo rescan"
    );
    assert!(
        session.last_typos && session.answers_small("abcd5."),
        "last matches holding every typo row narrow on the loop"
    );
}

#[test]
fn search_a_narrowing_after_the_index_grew_or_went_partial_takes_the_worker() {
    let root = TestDir::new("search-inline-index");
    for i in 0..20 {
        root.file(&format!("ab_{i:04}"), "a");
    }
    let mut session = opened(&root);
    session.query(1, "ab", 2).unwrap();
    assert!(session.answers_small("ab_"));
    assert!(session.index.lock().unwrap().push("ab_late", false));
    assert!(
        !session.answers_small("ab_"),
        "a row added since the last answer was never scored"
    );
    session.query(2, "ab", 2).unwrap();
    assert!(
        session.answers_small("ab_"),
        "the next answer scored the grown index"
    );
    session.index.lock().unwrap().mark_partial("watch_failed");
    assert!(
        !session.answers_small("ab_"),
        "a partial index holds rows the last matches never saw"
    );
}
