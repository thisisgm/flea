use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn search_fix2_collapsed_newer_query_invalidates_inflight_before_readiness() {
    for first_gen in [2, 0] {
        let root = TestDir::new("search-collapse-ready");
        root.file("alpha.txt", "a");
        let scope = root.path().to_str().unwrap();
        let (tx, rx) = crate::backend::events::channel();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(tx.clone()));
        st.search_session = Some(Session::open_sync(scope, scope, false));
        let tb = Tables::load();
        let (thumbs, _done) = channel();
        let pool = Pool::new(1, thumbs, root.dir("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let mut jobs = super::super::searchjobs::Gate::default();
        let mut buffered = VecDeque::new();
        let mut out = Vec::new();
        let query = |gen| Event::Request(format!(r#"{{"c":"searchquery","query":"alpha","gen":{gen},"count":1}}"#));
        jobs.route(query(1), &mut st, &mut out, &tb, &pool, &tx, &mut buffered, false);
        let ready = loop {
            let event = rx.recv_timeout(Duration::from_secs(30)).unwrap();
            if matches!(event, Event::SearchReady) { break event; }
        };
        tx.send(query(first_gen)).unwrap();
        tx.send(ready).unwrap();
        tx.send(Event::Request(r#"{"c":"window","start":0,"count":1}"#.into())).unwrap();
        tx.send(query(3)).unwrap();
        let event = rx.recv().unwrap();
        assert!(collapse_burst(&rx, event, &mut buffered, true, false, &mut jobs).is_none());
        let ready = buffered.pop_front().unwrap();
        assert!(matches!(ready, Event::SearchReady));
        jobs.route(ready, &mut st, &mut out, &tb, &pool, &tx, &mut buffered, false);
        let said = String::from_utf8(std::mem::take(&mut out)).unwrap();
        eprintln!("first_gen={first_gen} PUBLISHED AFTER QUEUED GEN 3: {said}");
        assert!(said.is_empty(), "the collapsed generation must suppress gen 1 before readiness");
        let Some(Event::Request(window)) = buffered.pop_front() else { panic!("window stays before newest query") };
        assert_eq!(crate::json::field_str(&window, "c").as_deref(), Some("window"));
        let Some(Event::Request(newest)) = buffered.pop_front() else { panic!("newest query stays after window") };
        assert_eq!(crate::json::field_usize(&newest, "gen"), Some(3));
        jobs.route(Event::Request(newest), &mut st, &mut out, &tb, &pool, &tx, &mut buffered, false);
        let ready = loop {
            let event = rx.recv_timeout(Duration::from_secs(30)).unwrap();
            if matches!(event, Event::SearchReady) { break event; }
        };
        jobs.route(ready, &mut st, &mut out, &tb, &pool, &tx, &mut buffered, false);
        let newest = String::from_utf8(out).unwrap();
        assert_eq!(newest.lines().count(), 3, "the newest query still owes its complete triplet");
        assert!(newest.lines().all(|line| crate::json::field_usize(line, "gen") == Some(3)));
        st.search_session.take().unwrap().close();
    }
}
