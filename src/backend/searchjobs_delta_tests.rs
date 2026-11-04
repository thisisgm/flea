use super::*;
use crate::backend::testdir::TestDir;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

pub(super) struct Loop {
    st: State,
    tb: Tables,
    pool: Pool,
    pub(super) gate: Gate,
    events: crate::backend::events::Sender<Event>,
    received: Receiver<Event>,
    pub(super) buffered: VecDeque<Event>,
    wakes: VecDeque<Event>,
    pub(super) out: Vec<u8>,
}

impl Loop {
    // A session over root/scope with its recursive watch on the loop's own channel, walked and answered once.
    pub(super) fn open(root: &TestDir, query: &str, count: usize) -> Loop {
        let (events, received) = crate::backend::events::channel();
        let scope = root.join("scope");
        let scope = scope.to_str().unwrap();
        let mut session = Session::open_async(scope, scope, false, events.clone());
        session.join_walk();
        let tb = Tables::load();
        let (results, _done) = channel();
        let pool = Pool::new(
            1,
            results,
            root.dir("thumbs"),
            Arc::clone(&tb.aliases),
            Arc::clone(&tb.thumbs),
        );
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        session.query(1, query, count).unwrap();
        st.search_session = Some(session);
        Loop {
            st,
            tb,
            pool,
            gate: Gate::default(),
            events,
            received,
            buffered: VecDeque::new(),
            wakes: VecDeque::new(),
            out: Vec::new(),
        }
    }

    pub(super) fn route(&mut self, event: Event) -> Option<Event> {
        self.gate.route(
            event,
            &mut self.st,
            &mut self.out,
            &self.tb,
            &self.pool,
            &self.events,
            &mut self.buffered,
            false,
        )
    }

    // The next wake of one kind; readiness and delta wakes of the other kind wait their turn, and walk progress is skipped.
    fn wake(&mut self, ready: bool) -> Event {
        let wanted = |event: &Event| {
            if ready {
                matches!(event, Event::SearchReady)
            } else {
                matches!(event, Event::SearchDelta(_))
            }
        };
        if let Some(at) = self.wakes.iter().position(wanted) {
            return self.wakes.remove(at).unwrap();
        }
        loop {
            let event = self
                .received
                .recv_timeout(Duration::from_secs(30))
                .expect("a search wake");
            if wanted(&event) {
                return event;
            }
            if matches!(event, Event::SearchReady | Event::SearchDelta(_)) {
                self.wakes.push_back(event);
            }
        }
    }

    pub(super) fn ready(&mut self) {
        let ready = self.wake(true);
        self.route(ready);
    }

    fn delta(&mut self) -> Event {
        self.wake(false)
    }

    // One write group can wake the watch twice, so a short answer takes the next delta and answers again.
    fn settle(&mut self, count: usize) -> Vec<String> {
        loop {
            self.ready();
            let said = self.said();
            if said
                .get(2)
                .and_then(|line| crate::json::field_usize(line, "n"))
                == Some(count)
            {
                return said;
            }
            let delta = self.delta();
            self.route(delta);
        }
    }

    pub(super) fn said(&mut self) -> Vec<String> {
        String::from_utf8(std::mem::take(&mut self.out))
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    pub(super) fn session(&mut self) -> &mut Session {
        self.st.search_session.as_mut().unwrap()
    }

    pub(super) fn close(mut self) {
        self.route(Event::Request(r#"{"c":"searchclose"}"#.into()));
        self.st.search_session.take().unwrap().close();
    }
}

pub(super) fn kinds(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| crate::json::field_str(line, "t").unwrap_or_default())
        .collect()
}

fn size_of(rows: &str, name: &str) -> u64 {
    let at = rows
        .find(&format!(r#""n":"{name}""#))
        .unwrap_or_else(|| panic!("{name} in {rows}"));
    crate::json::field_usize(&rows[at..], "s").unwrap() as u64
}

#[test]
fn search_delta_arriving_while_a_streamed_answer_is_out_publishes_after_it() {
    let root = TestDir::new("search-delta-inflight");
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
    assert!(lp.gate.busy());
    let added = root.join("scope/alpha9.txt");
    root.assert_contains(&added);
    std::fs::write(&added, "a").unwrap();
    let delta = lp.delta();
    assert!(
        lp.route(delta).is_none(),
        "a delta waits behind the answer still out"
    );
    assert!(lp.out.is_empty());
    lp.ready();
    let first = lp.said();
    assert_eq!(kinds(&first), ["listed", "rows", "searched"]);
    assert!(first
        .iter()
        .all(|line| crate::json::field_usize(line, "gen") == Some(2)));
    let replay = lp
        .buffered
        .pop_front()
        .expect("the delta replays after the published answer");
    assert!(matches!(replay, Event::SearchDelta(_)));
    assert!(
        lp.route(replay).is_none(),
        "the replayed delta is reconciled on the execution worker"
    );
    let second = lp.settle(4);
    assert_eq!(kinds(&second), ["listed", "rows", "searched"]);
    assert!(
        second
            .iter()
            .all(|line| crate::json::field_usize(line, "gen") == Some(2)),
        "the standing gen is pushed again"
    );
    assert!(second[1].contains(r#""n":"alpha9.txt""#));
    let (before, after) = (
        crate::json::field_usize(&first[1], "listing"),
        crate::json::field_usize(&second[1], "listing"),
    );
    assert!(
        before.is_some() && after.is_some(),
        "both rows lines carry the listing stamp: {first:?} {second:?}"
    );
    assert!(after > before, "the listing stamp advances");
    lp.close();
}

#[test]
fn search_delta_against_reused_rows_reuses_the_unchanged_and_rereads_the_changed() {
    let root = TestDir::new("search-delta-reused");
    root.dir("scope");
    for i in 0..4 {
        root.file(&format!("scope/alpha{i}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 10);
    lp.route(Event::SearchRefresh);
    lp.ready();
    let rows = lp.said();
    assert_eq!(kinds(&rows), ["listed", "rows", "searched"]);
    let misses = lp.session().row_cache.misses;
    assert_eq!(misses, 4, "the first window reads every row");
    // A delta that touches no cached identity keeps every reused row and reads only the new one.
    std::fs::write(root.join("scope/alpha8.txt"), "a").unwrap();
    let delta = lp.delta();
    lp.route(delta);
    lp.settle(5);
    assert_eq!(
        lp.session().row_cache.misses,
        misses + 1,
        "only the added row is read"
    );
    // A delta beside a write to a reused row rereads that row; its size is the file's, never the cached one.
    let changed = root.join("scope/alpha1.txt");
    root.assert_contains(&changed);
    std::fs::write(&changed, "changed").unwrap();
    std::fs::write(root.join("scope/alpha9.txt"), "a").unwrap();
    let delta = lp.delta();
    lp.route(delta);
    let fresh = lp.settle(6);
    assert_eq!(kinds(&fresh), ["listed", "rows", "searched"]);
    assert_eq!(size_of(&fresh[1], "alpha1.txt"), 7);
    assert_eq!(size_of(&fresh[1], "alpha0.txt"), 1);
    let reread = lp.st.base.join("alpha1.txt");
    let rendered = lp
        .session()
        .row_cache
        .rendered(&reread)
        .map(|(row, _)| row.clone())
        .expect("the reread row is cached again");
    assert!(rendered.contains(r#""s":7"#), "{rendered}");
    lp.close();
}

#[test]
fn search_refresh_after_a_delta_answers_the_patched_index_with_a_valid_remainder() {
    let root = TestDir::new("search-delta-refresh");
    root.dir("scope");
    for i in 0..40 {
        root.file(&format!("scope/alpha{i:02}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 2);
    assert_eq!(lp.session().pending_rest_len(), 38);
    let gone = root.join("scope/alpha00.txt");
    root.assert_contains(&gone);
    std::fs::remove_file(&gone).unwrap();
    let delta = lp.delta();
    assert!(lp.route(delta).is_none());
    assert!(
        lp.route(Event::SearchRefresh).is_none(),
        "a refresh waits behind the delta still out"
    );
    lp.ready();
    let patched = lp.said();
    assert_eq!(crate::json::field_usize(&patched[2], "n"), Some(39));
    let refresh = lp
        .buffered
        .pop_front()
        .expect("the refresh replays after the delta's answer");
    assert!(matches!(refresh, Event::SearchRefresh));
    lp.route(refresh);
    lp.ready();
    let refreshed = lp.said();
    assert_eq!(kinds(&refreshed), ["listed", "rows", "searched"]);
    assert_eq!(crate::json::field_usize(&refreshed[2], "n"), Some(39));
    assert!(refreshed
        .iter()
        .all(|line| crate::json::field_usize(line, "gen") == Some(1)));
    assert!(!refreshed[1].contains("alpha00.txt"));
    let session = lp.session();
    let rest = session
        .take_rest_chunk(usize::MAX, 2)
        .expect("the remainder survives on fresh row ids");
    assert_eq!(rest.len(), 37);
    for (name, _) in &rest {
        assert_ne!(name, "alpha00.txt");
        assert!(
            root.join("scope").join(name).is_file(),
            "{name} names a file still on disk"
        );
    }
    lp.close();
}

#[test]
fn search_delta_discarded_by_a_newer_query_never_restores_renumbered_rest() {
    let root = TestDir::new("search-delta-discard");
    root.dir("scope");
    for i in 0..40 {
        root.file(&format!("scope/alpha{i:02}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 2);
    let gone = root.join("scope/alpha05.txt");
    root.assert_contains(&gone);
    std::fs::remove_file(&gone).unwrap();
    let delta = lp.delta();
    lp.route(delta);
    lp.route(Event::Request(
        r#"{"c":"searchquery","query":"alpha","gen":2,"count":2}"#.into(),
    ));
    lp.ready();
    assert!(
        lp.out.is_empty(),
        "the superseded delta answer is never published"
    );
    assert_eq!(
        lp.session().pending_rest_len(),
        0,
        "row ids from before the delta are never restored"
    );
    lp.close();
}

#[test]
fn search_delta_routed_while_the_worker_owns_the_session_waits_behind_its_answer() {
    let root = TestDir::new("search-delta-owned");
    root.dir("scope");
    for i in 0..3 {
        root.file(&format!("scope/alpha{i}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 10);
    lp.route(Event::Request(
        r#"{"c":"searchquery","query":"alpha","gen":2,"count":10}"#.into(),
    ));
    assert!(
        lp.gate.busy() && lp.st.search_session.is_none(),
        "the worker owns the session"
    );
    route_delta(
        Some(&mut lp.gate),
        7,
        &mut lp.st,
        &mut lp.out,
        &lp.tb,
        &lp.pool,
        &lp.events,
    );
    assert!(lp.out.is_empty());
    lp.ready();
    assert!(
        matches!(lp.buffered.pop_front(), Some(Event::SearchDelta(7))),
        "the delta waits behind the answer, never lost"
    );
    lp.close();
}

#[path = "searchjobs_landing_tests.rs"]
mod landing;
