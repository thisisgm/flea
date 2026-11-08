use super::*;
use crate::backend::testdir::TestDir;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn search_execution_reuses_one_worker_and_keeps_backspace_and_page_order() {
    let root = TestDir::new("search-execution");
    for i in 0..700 { root.file(&format!("alpha{i:04}.txt"), "a"); }
    let scope = root.path().to_str().unwrap();
    let session = Session::open_sync(scope, scope, false);
    let (events, _received) = crate::backend::events::channel();
    let worker = Worker::new(events);
    let thread = worker.handle.thread().id();
    assert_ne!(thread, std::thread::current().id());
    worker.tx.send((session, Work::Query("alpha".into(), 1, 2, None))).ok().unwrap();
    let done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(done.answer.unwrap().total, 700);
    assert_eq!(done.session.pending_rest_len(), 698);
    worker.tx.send((done.session, Work::Query("a".into(), 2, 2, None))).ok().unwrap();
    let mut done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let answer = done.answer.unwrap();
    assert_eq!(answer.gen, 2);
    assert_eq!(answer.total, 700);
    let mut rows = answer.rows;
    rows.extend(done.session.take_rest_chunk(usize::MAX, rows.len()).unwrap());
    assert_eq!(rows.len(), 700);
    assert_eq!(rows[0].0, "alpha0000.txt");
    assert_eq!(rows[699].0, "alpha0699.txt");
    assert_eq!(worker.handle.thread().id(), thread);
    let Worker { tx, rx, handle } = worker;
    drop(tx);
    drop(rx);
    handle.join().unwrap();
    done.session.close();
}

#[test]
fn search_execution_discards_superseded_answers_and_restores_the_published_remainder() {
    let root = TestDir::new("search-execution-discard");
    for i in 0..400 { root.file(&format!("alpha{i:04}.txt"), "a"); }
    let (events, received) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let scope = root.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    session.query(1, "alpha", 2).unwrap();
    st.search_session = Some(session);
    let tb = Tables::load();
    let (results, _done) = channel();
    let pool = Pool::new(1, results, root.dir("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let mut gate = Gate::default();
    let mut out = Vec::new();
    let mut buffered = VecDeque::new();
    let query = |gen| Event::Request(format!(r#"{{"c":"searchquery","query":"alpha","gen":{gen},"count":2}}"#));
    assert!(gate.route(query(2), &mut st, &mut out, &tb, &pool, &events, &mut buffered, false).is_none());
    assert!(gate.busy());
    gate.route(Event::Request(r#"{"c":"window","start":2,"count":1}"#.into()), &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    gate.route(query(3), &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    let ready = received.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(matches!(ready, Event::SearchReady));
    gate.route(ready, &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    assert!(out.is_empty(), "superseded work never publishes a generation");
    assert_eq!(st.search_session.as_ref().unwrap().last_answered_gen, 1);
    assert_eq!(st.search_session.as_ref().unwrap().pending_rest_len(), 398);
    let Some(Event::Request(first)) = buffered.pop_front() else { panic!("ordinary request first") };
    assert_eq!(crate::json::field_str(&first, "c").as_deref(), Some("window"));
    let Some(Event::Request(second)) = buffered.pop_front() else { panic!("newest query second") };
    assert_eq!(crate::json::field_usize(&second, "gen"), Some(3));
    gate.route(Event::Request(r#"{"c":"searchclose"}"#.into()), &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    assert!(gate.worker.is_none(), "close joins and releases the retained execution worker");
    st.search_session.take().unwrap().close();
}

#[test]
#[ignore]
fn search_fb5_probe_worker_scale_pressure() {
    for count in [682, 1024, 4096, 100_000, 1_000_000] {
        let root = TestDir::new("search-worker-probe");
        let path = root.path().to_str().unwrap();
        let mut session = Session::open_sync(path, path, false);
        let mut index = super::super::searchindex::Index::new();
        for i in 0..count { index.push(format!("directory{:04}/report{:04}.txt", i / 1000, i % 4096), false); }
        index.finish();
        let logical = super::super::searchindex::index_bytes(&index);
        session.index = Arc::new(std::sync::Mutex::new(index));
        let (events, _received) = crate::backend::events::channel();
        let worker = Worker::new(events);
        let queries = ["r", "re", "rep", "report", "report0042.txt", "rep"];
        let mut times = Vec::new();
        let cpu_start = process_cpu_ns();
        for (gen, query) in queries.iter().enumerate() {
            let started = std::time::Instant::now();
            worker.tx.send((session, Work::Query((*query).into(), gen + 1, 350, None))).ok().unwrap();
            let done = worker.rx.recv_timeout(Duration::from_secs(120)).unwrap();
            times.push(started.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(done.answer.as_ref().unwrap().gen, gen + 1);
            session = done.session;
        }
        let cpu_ms = (process_cpu_ns() - cpu_start) as f64 / 1e6;
        let all = times.iter().sum::<f64>();
        let query_pss_kb = pss_kb();
        let slowest = times.iter().copied().fold(0.0, f64::max);
        let (loop_events, loop_received) = crate::backend::events::channel();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(loop_events.clone()));
        st.search_session = Some(session);
        let tb = Tables::load();
        let (results, _done) = channel();
        let pool = Pool::new(1, results, root.dir("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let Worker { tx, rx, handle } = worker;
        drop(tx);
        drop(rx);
        handle.join().unwrap();
        let mut gate = Gate::default();
        let mut out = Vec::new();
        let mut buffered = VecDeque::new();
        // Broader than the standing "rep", so it is never answered inline and the close below waits on the worker.
        gate.route(Event::Request(r#"{"c":"searchquery","query":"re","gen":7,"count":350}"#.into()),
            &mut st, &mut out, &tb, &pool, &loop_events, &mut buffered, false);
        assert!(gate.busy(), "the gen 7 query runs on the worker, so this probe measures its close");
        let close = std::time::Instant::now();
        gate.route(Event::Request(r#"{"c":"searchclose"}"#.into()), &mut st, &mut out, &tb, &pool, &loop_events, &mut buffered, false);
        loop {
            let event = loop_received.recv_timeout(Duration::from_secs(120)).unwrap();
            if matches!(event, Event::SearchReady) {
                gate.route(event, &mut st, &mut out, &tb, &pool, &loop_events, &mut buffered, false);
                break;
            }
        }
        assert!(out.is_empty());
        let close_request = buffered.pop_front().unwrap();
        assert!(gate.route(close_request, &mut st, &mut out, &tb, &pool, &loop_events, &mut buffered, false).is_some());
        st.search_session.take().unwrap().close();
        let cancel_to_idle_us = close.elapsed().as_secs_f64() * 1e6;
        assert!(gate.worker.is_none());
        println!("SEARCH_WORKER_PROBE entries={count} queries=6 basename_modulus=4096 batch_ms={all:.3} slowest_query_ms={slowest:.3} close_cancel_to_idle_us={cancel_to_idle_us:.3} logical_bytes={logical} query_pss_kb={query_pss_kb} process_cpu_ms={cpu_ms:.3}");
    }
}

#[test]
fn search_query_burst_keeps_sizes_and_the_newest_line_in_its_ordinary_request_position() {
    let (tx, rx) = channel();
    tx.send(Event::Request(r#"{"c":"window","start":1,"count":2}"#.into())).unwrap();
    let newest = r#"{"c":"searchquery","query":"alpha","gen":9,"count":12,"sizes":7}"#;
    tx.send(Event::Request(newest.into())).unwrap();
    let mut buffered = VecDeque::new();
    assert!(super::super::searchreq::collapse_query_burst(&rx, r#"{"c":"searchquery","query":"a","gen":1}"#.into(), &mut buffered).0.is_none());
    let Some(Event::Request(window)) = buffered.pop_front() else { panic!("window remains first") };
    assert_eq!(crate::json::field_str(&window, "c").as_deref(), Some("window"));
    let Some(Event::Request(query)) = buffered.pop_front() else { panic!("newest query remains second") };
    assert_eq!(query, newest, "count, sizes and future request fields survive collapse");
}

fn pss_kb() -> usize {
    std::fs::read_to_string("/proc/self/smaps_rollup").unwrap().lines()
        .find_map(|line| line.strip_prefix("Pss:").and_then(|rest| rest.split_whitespace().next()).and_then(|n| n.parse().ok())).unwrap()
}

// CLOCK_PROCESS_CPUTIME_ID from clock_gettime(2): the CPU time this process has used.
const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;

fn process_cpu_ns() -> u64 {
    #[repr(C)]
    struct Timespec { seconds: i64, nanoseconds: i64 }
    extern "C" { fn clock_gettime(clock: i32, time: *mut Timespec) -> i32; }
    let mut time = Timespec { seconds: 0, nanoseconds: 0 };
    assert_eq!(unsafe { clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut time) }, 0);
    time.seconds as u64 * 1_000_000_000 + time.nanoseconds as u64
}

#[test]
fn search_worker_streams_only_the_newest_generation_then_one_complete_terminal() {
    let root = TestDir::new("search-worker-stream");
    root.dir("held");
    root.file("alpha.txt", "a");
    let probes = Arc::new(super::super::searchwalk::WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("held");
    let (events, _received) = crate::backend::events::channel();
    let path = root.path().to_str().unwrap();
    let session = Session::open_async_with_probes(path, path, false, events.clone(), probes);
    parked.recv_timeout(Duration::from_secs(30)).unwrap();
    let worker = Worker::new(events);
    worker.tx.send((session, Work::Query("a".into(), 1, 2, None))).ok().unwrap();
    let done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(done.answer.is_none(), "initial hold remains active");
    worker.tx.send((done.session, Work::Query("al".into(), 2, 2, None))).ok().unwrap();
    let mut done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(done.answer.is_none());
    done.session.hold_until = Some(std::time::Instant::now());
    worker.tx.send((done.session, Work::Held)).ok().unwrap();
    let mut done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let partial = done.answer.unwrap();
    assert_eq!(partial.gen, 2);
    assert!(partial.walking);
    done.session.index.lock().unwrap().push("alpine.txt", false);
    done.session.test_set_last_batch(std::time::Instant::now() - Duration::from_millis(101));
    worker.tx.send((done.session, Work::Walked)).ok().unwrap();
    let mut done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let partial = done.answer.unwrap();
    assert_eq!(partial.gen, 2);
    assert_eq!(partial.total, 2);
    assert!(partial.walking);
    release.send(()).unwrap();
    done.session.handle.take().unwrap().join().unwrap();
    worker.tx.send((done.session, Work::Walked)).ok().unwrap();
    let done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let final_answer = done.answer.unwrap();
    assert_eq!(final_answer.gen, 2);
    assert!(!final_answer.walking);
    worker.tx.send((done.session, Work::Walked)).ok().unwrap();
    let done = worker.rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(done.answer.is_none(), "a second completion cannot publish another terminal");
    let Worker { tx, rx, handle } = worker;
    drop(tx);
    drop(rx);
    handle.join().unwrap();
    done.session.close();
}

#[test]
fn search_discarded_landing_never_restores_row_ids_across_a_pending_repack() {
    let root = TestDir::new("search-repack-discard");
    for i in 0..400 { root.file(&format!("alpha{i:04}.txt"), "a"); }
    let (events, received) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let scope = root.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    session.query(1, "alpha", 2).unwrap();
    assert_eq!(session.pending_rest_len(), 398);
    session.announced = false;
    session.standing_while_walking = true;
    session.pending_patches.insert(std::path::PathBuf::new());
    let before = root.join("alpha0000.txt");
    let after = root.join("beta.txt");
    root.assert_contains(&before);
    root.assert_contains(&after);
    std::fs::rename(before, after).unwrap();
    let epoch = session.index.lock().unwrap().epoch;
    st.search_session = Some(session);
    let tb = Tables::load();
    let (results, _done) = channel();
    let pool = Pool::new(1, results, root.dir("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let mut gate = Gate::default();
    let mut out = Vec::new();
    let mut buffered = VecDeque::new();
    gate.route(Event::SearchWalked, &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    gate.route(Event::Request(r#"{"c":"searchquery","query":"alpha","gen":2,"count":2}"#.into()),
        &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    let ready = loop {
        let event = received.recv_timeout(Duration::from_secs(30)).unwrap();
        if matches!(event, Event::SearchReady) { break event; }
    };
    gate.route(ready, &mut st, &mut out, &tb, &pool, &events, &mut buffered, false);
    assert!(out.is_empty(), "the old generation remains suppressed after patching");
    let session = st.search_session.take().unwrap();
    assert_ne!(
        session.index.lock().unwrap().epoch,
        epoch,
        "the pending patch renumbered the index"
    );
    assert_eq!(session.pending_rest_len(), 0, "row IDs from before the patch must never be restored");
    session.close();
}
