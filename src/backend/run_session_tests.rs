use super::*;
use crate::backend::testdir::TestDir;

const WIRE_DEADLINE: Duration = Duration::from_secs(30);

// The walk's landing as the loop meets it, past the walk progress that shares its event.
fn walk_landed(rx: &Receiver<Event>, st: &State, skip: fn(&Event) -> bool) {
    let session = st.search_session.as_ref().expect("a session stands");
    crate::backend::searchwalkhold::wait_walk_landed(rx, session, WIRE_DEADLINE, skip);
}

// Opens the session with its walk hold armed first, so the walk can never pass `at` before the test parks it.
fn open_held(st: &mut State, root: &str, events: &crate::backend::events::Sender<Event>, at: usize) -> (Receiver<()>, std::sync::mpsc::Sender<()>) {
    let probes = Arc::new(crate::backend::searchwalk::WalkProbes::new());
    let hold = probes.test_hold_walk_at(at);
    st.search_session = Some(Session::open_async_with_probes(root, root, false, events.clone(), probes));
    hold
}

// A live session over the wire: open, query, empty query, stale drop, close, and list closing it.
#[test]
fn a_session_answers_listed_rows_and_searched_with_its_generation() {
    let d = TestDir::new("sessionwire");
    d.dir("docs");
    d.file("docs/bench-notes.txt", "");
    d.file("bench.txt", "");
    d.file("other.txt", "");
    let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
    let tb = Tables::load();
    let (events, events_rx) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let (results, _done) = channel();
    let cache_root = d.join("cache");
    let pool = Pool::new(1, results, cache_root, Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    let (tx, _rx) = channel();
    let mut ops = Ops::new(tx);
    let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    let ask = |line: &str,
               out: &mut Vec<u8>,
               st: &mut State,
               ops: &mut Ops,
               watch: &mut Watch,
               peeks: &mut PeekWatch,
               fsinfo: &mut FsInfo,
               poller: &super::super::watchpoll::Poller| {
        assert!(
            handle_line(line, out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), watch, peeks, fsinfo, poller, &events, &mut Gate::new(events.clone()))
                == Control::Continue
        );
    };

    ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    // The walk lands on its own event; the query below must score the whole scope.
    walk_landed(&events_rx, &st, |_| false);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1, "open answers one line, no rows change: {lines:?}");
    assert_eq!(crate::json::field_str(&lines[0], "t").as_deref(), Some("searchopened"));

    ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched over the fixture: {lines:?}");
    assert_eq!(crate::json::field_str(&lines[0], "t").as_deref(), Some("listed"));
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(2), "bench.txt and docs/bench-notes.txt");
    assert_eq!(crate::json::field_str(&lines[0], "path").as_deref(), Some(root.as_str()));
    assert_eq!(crate::json::field_str(&lines[1], "t").as_deref(), Some("rows"));
    assert!(lines[1].contains("bench-notes.txt") && lines[1].contains("bench.txt"), "the window carries both matches");
    assert_eq!(crate::json::field_str(&lines[2], "t").as_deref(), Some("searched"));
    assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(1));
    assert_eq!(crate::json::field_usize(&lines[2], "n"), Some(2));
    assert_eq!(crate::json::field_str(&lines[2], "walking"), None, "walking is a bool, not a string");
    assert!(lines[2].contains(r#""walking":false"#), "the landed walk says walking false");

    // An empty query keeps the session and answers the folder's own listing with the same gen order, its folder sizes inline.
    st.search_session.as_mut().expect("the session is open").sizes.unhurried();
    ask(r#"{"c":"searchquery","query":"","gen":2,"sizes":40}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched for the folder: {lines:?}");
    assert_eq!(crate::json::field_str(&lines[0], "path").as_deref(), Some(root.as_str()), "the folder answers under its own path");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(3), "bench.txt, docs and other.txt");
    assert_eq!(crate::json::field_usize(&lines[0], "gen"), Some(2), "the folder's listed line carries its gen");
    assert_eq!(crate::json::field_usize(&lines[1], "gen"), Some(2), "the folder's rows carry its gen");
    let docs = (0..st.listing.len()).find(|&row| st.listing.name(row) == "docs").expect("docs is listed");
    let sized = format!(r#""z":[[{},{}]]}}"#, docs, super::super::dirsize::walk(&d.join("docs")).bytes);
    assert!(lines[1].ends_with(&sized), "the folder's rows carry its folder's size: {}", lines[1]);
    assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(2));

    // A stale generation answers nothing at all.
    ask(r#"{"c":"searchquery","query":"bench","gen":1}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    assert!(out.is_empty(), "gen 1 after gen 2 answers nothing");

    // Close drops the index; a later query answers nothing until a session opens again.
    ask(r#"{"c":"searchclose"}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(crate::json::field_str(&lines[0], "t").as_deref(), Some("searchclosed"));
    ask(r#"{"c":"searchquery","query":"bench","gen":3}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    assert!(out.is_empty(), "no session open answers nothing");

    // A navigation closes the session: open again, list away, and the query finds no session.
    ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    out.clear();
    ask(&format!(r#"{{"c":"list","path":"{}","first":10}}"#, root), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    out.clear();
    ask(r#"{"c":"searchquery","query":"bench","gen":4}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    assert!(out.is_empty(), "a list closes the session");
}

// A window past the head returns the head's continuation, byte for byte what a full-window query answers.
#[test]
fn a_window_past_the_head_returns_the_full_ranked_order() {
    let d = TestDir::new("sessionwindowpasthead");
    d.dir("docs");
    d.file("docs/bench-notes.txt", "");
    d.file("bench.txt", "");
    d.file("other.txt", "");
    let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
    let tb = Tables::load();
    let (events, events_rx) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let (results, _done) = channel();
    let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    let (tx, _rx) = channel();
    let mut ops = Ops::new(tx);
    let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    let ask = |line: &str,
               out: &mut Vec<u8>,
               st: &mut State,
               ops: &mut Ops,
               watch: &mut Watch,
               peeks: &mut PeekWatch,
               fsinfo: &mut FsInfo,
               poller: &super::super::watchpoll::Poller| {
        assert!(
            handle_line(line, out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), watch, peeks, fsinfo, poller, &events, &mut Gate::new(events.clone()))
                == Control::Continue
        );
    };

    ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    // The walk lands on its own event; the queries below must score the whole scope.
    walk_landed(&events_rx, &st, |_| false);
    out.clear();
    // Two bench matches over the fixture, asked with a window of one.
    ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":1}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched: {lines:?}");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(2), "both matches counted");
    let head_rows = lines[1].clone();
    let head_name = if head_rows.contains("bench-notes.txt") { "bench-notes.txt" } else { "bench.txt" };
    let past_name = if head_name == "bench.txt" { "bench-notes.txt" } else { "bench.txt" };
    // The window is the first request past the head, so only its own extension can bring the held match in.
    ask(r#"{"c":"window","start":1,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1, "one rows line: {lines:?}");
    let past_rows = lines[0].clone();
    assert!(past_rows.contains(past_name), "the window extends the listing with the held match: {past_rows:?}");
    assert_eq!(st.listing.len(), 2, "the window adopted the held match");
    let mut found = Vec::new();
    for rel in ["docs/bench-notes.txt", "bench.txt"] {
        ask(&format!(r#"{{"c":"locate","path":"{}/{}"}}"#, root, rel), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 1, "one located line: {lines:?}");
        found.push(crate::json::field_usize(&lines[0], "index"));
    }
    found.sort();
    assert_eq!(found, vec![Some(0), Some(1)], "the two matches sit at rows 0 and 1");
    // The same generation asked whole answers head plus continuation in one order.
    ask(r#"{"c":"searchquery","query":"bench","gen":2,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched: {lines:?}");
    let full_rows = lines[1].clone();
    assert!(full_rows.contains("bench-notes.txt") && full_rows.contains("bench.txt"), "the whole answer carries both: {full_rows:?}");
    let head_at = full_rows.find(head_name).expect("the head opens the whole answer");
    let past_at = full_rows.find(past_name).expect("the continuation follows in the whole answer");
    assert!(head_at < past_at, "the scrolled window keeps the ranked order");
}

// A select-all snapshot resolves every match the listed line counted, rows past the answered head included.
#[test]
fn a_menu_select_all_resolves_every_match_in_ranked_order() {
    let d = TestDir::new("sessionmenuselectall");
    d.dir("docs");
    d.file("docs/bench-notes.txt", "");
    d.file("bench.txt", "");
    d.file("other.txt", "");
    let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
    let tb = Tables::load();
    let (events, events_rx) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let (results, _done) = channel();
    let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    let ask = |line: &str,
               out: &mut Vec<u8>,
               st: &mut State,
               ops: &mut Ops,
               watch: &mut Watch,
               peeks: &mut PeekWatch,
               fsinfo: &mut FsInfo,
               poller: &super::super::watchpoll::Poller| {
        assert!(
            handle_line(line, out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), watch, peeks, fsinfo, poller, &events, &mut Gate::new(events.clone()))
                == Control::Continue
        );
    };

    ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    // The walk lands on its own event; the queries below must score the whole scope.
    walk_landed(&events_rx, &st, |_| false);
    out.clear();
    // Two bench matches over the fixture, asked with a window of one.
    ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":1}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched: {lines:?}");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(2), "both matches counted");
    // Select all: every row the listed line counted resolves into the snapshot.
    ask(r#"{"c":"menuaction","op":"snapshot","id":7,"rows":[0,1]}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let crate::backend::opsreq::OpMsg::Meta { line } = rx.recv_timeout(WIRE_DEADLINE).expect("the snapshot answers") else {
        panic!("a menuaction reply");
    };
    assert!(line.contains(r#""ok":true"#), "the snapshot takes: {line:?}");
    assert!(line.contains(r#""count":2"#), "both matches resolved, not just the head: {line:?}");
    // The snapshot's extension completed the listing, so a window over it stands in a whole fresh answer's order.
    ask(r#"{"c":"window","start":0,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1, "one rows line: {lines:?}");
    let window_rows = lines[0].clone();
    ask(r#"{"c":"searchquery","query":"bench","gen":2,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched: {lines:?}");
    let full_rows = lines[1].clone();
    let order_of = |rows: &str| {
        let notes = rows.find("bench-notes.txt").expect("the notes match is drawn");
        let plain = rows.find("bench.txt").expect("the plain match is drawn");
        notes < plain
    };
    assert_eq!(order_of(&window_rows), order_of(&full_rows), "the snapshot-extended listing keeps the ranked order");
}

// A locate after a partial extension completes the held tail rather than missing a row still held.
#[test]
fn a_locate_after_a_partial_extension_finds_a_row_in_the_held_remainder() {
    let d = TestDir::new("sessionlocatepartial");
    // Child-only writers cannot leak through another test's fork and close after this scope's watch is armed.
    let files: Vec<_> = (0..400).map(|i| d.join(&format!("bench-{:04}.txt", i))).collect();
    assert!(std::process::Command::new("/usr/bin/touch").args(&files).status().expect("the fixture writer starts").success(), "the fixture writer finishes");
    let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
    let tb = Tables::load();
    let (events, events_rx) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let (results, _done) = channel();
    let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    let (tx, _rx) = channel();
    let mut ops = Ops::new(tx);
    let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    let ask = |line: &str,
               out: &mut Vec<u8>,
               st: &mut State,
               ops: &mut Ops,
               watch: &mut Watch,
               peeks: &mut PeekWatch,
               fsinfo: &mut FsInfo,
               poller: &super::super::watchpoll::Poller| {
        assert!(
            handle_line(line, out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), watch, peeks, fsinfo, poller, &events, &mut Gate::new(events.clone()))
                == Control::Continue
        );
    };
    let held = |st: &State| st.search_session.as_ref().map(|s| s.pending_rest_len()).unwrap_or(usize::MAX);

    ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    // The walk lands on its own event; the queries below must score the whole scope.
    walk_landed(&events_rx, &st, |_| false);
    out.clear();
    // Four hundred bench matches over the fixture, asked with a window of one.
    ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":1}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3, "listed, rows, searched: {lines:?}");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(400), "all four hundred matches counted");
    assert_eq!(held(&st), 399, "the rest waits unranked for a scroll");
    // Empty windows clamp against the full count without ranking or materializing the held remainder.
    for (start, clamped) in [(200, 200), (999, 400)] {
        ask(&format!(r#"{{"c":"window","start":{start},"count":0}}"#), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        let reply = String::from_utf8(std::mem::take(&mut out)).unwrap();
        assert_eq!(crate::json::field_usize(&reply, "start"), Some(clamped));
        assert_eq!(crate::json::field_usize(&reply, "listing"), Some(st.generation as usize));
        assert!(reply.contains(r#""rows":[],"kinds":[],"ms":0.000"#));
        assert_eq!(held(&st), 399, "an empty request leaves every unranked match held");
        assert_eq!(st.listing.len(), 1);
        write_window_gen(&mut out, &mut st, start, 0, &tb, Some(7));
        let generated = String::from_utf8(std::mem::take(&mut out)).unwrap();
        assert_eq!(crate::json::field_usize(&generated, "start"), Some(clamped));
        assert!(generated.contains(r#""rows":[],"kinds":[],"ms":0.000"#));
        assert_eq!(held(&st), 399, "the generated empty window shares the dispatch clamp");
    }
    write_window_gen(&mut out, &mut st, 999, 1, &tb, Some(7));
    let beyond = String::from_utf8(std::mem::take(&mut out)).unwrap();
    assert_eq!(crate::json::field_usize(&beyond, "start"), Some(400));
    assert_eq!(held(&st), 399, "an out-of-range generated window ranks no held rows");
    // Past the head: one window of 350 joins, 49 stay held.
    ask(r#"{"c":"window","start":1,"count":1}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1, "one rows line: {lines:?}");
    assert_eq!(held(&st), 49, "the first scroll spends one chunk, not the hold");
    // The oracle is today's walk path over the same tree and query.
    let mut walk = crate::backend::search::Search::new(&root, "bench", false);
    let mut listing = crate::backend::listing::Listing::new();
    while !walk.step(&mut listing) {}
    walk.rank(&mut listing);
    let walk_names: Vec<String> = (0..listing.len()).map(|i| listing.name(i).to_string()).collect();
    assert_eq!(walk_names.len(), 400, "the oracle sees every match");
    assert!(lines[0].contains(&walk_names[1]), "the scrolled row is the ranked order's second");
    // The last match is still held; the locate completes the listing to find it.
    let last = walk_names.last().expect("the oracle names a last match");
    ask(&format!(r#"{{"c":"locate","path":"{}/{}"}}"#, root, last), &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
    let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1, "one located line: {lines:?}");
    assert!(lines[0].contains(r#""index":399"#), "the locate finds the held row at the end: {lines:?}");
    assert_eq!(held(&st), 0, "the locate spends the held remainder");
}

    // A query held past the cap answers from the partial index and re-runs with its own gen when the walk lands.
    #[test]
    fn a_walking_query_re_runs_at_landing_with_its_own_gen() {
        let d = TestDir::new("sessionlanding");
        d.dir("docs");
        for i in 0..10 {
            d.dir(&format!("pad{}", i));
        }
        d.file("docs/bench-notes.txt", "");
        d.file("bench.txt", "");
        let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
        let tb = Tables::load();
        let (events, events_rx) = crate::backend::events::channel::<Event>();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        let (results, _done) = channel();
        let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let cache = Cache::new();
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
        let mut fsinfo = FsInfo::new(events.clone());
        let poller = super::super::watchpoll::Poller::new(events.clone());
        let mut out = Vec::new();
        // Park the walk at its last directory of twelve, so the query answers from a partial index.
        let (parked, release) = open_held(&mut st, &root, &events, 12);
        parked.recv_timeout(WIRE_DEADLINE).expect("the walk parks at its last directory");
        assert!(
            handle_line(
                r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#,
                &mut out,
                &mut st,
                &tb,
                &pool,
                &cache,
                &mut ops,
                &mut flashslot::Ctx::new(channel().0),
                &mut watch,
                &mut peeks,
                &mut fsinfo,
                &poller,
                &events, &mut Gate::new(events.clone())
            ) == Control::Continue
        );
        assert!(out.is_empty(), "the key waits for the walk up to the cap");
        // The parked walk outlasts the cap, so the cap's event answers the key from the partial index.
        while !matches!(events_rx.recv_timeout(WIRE_DEADLINE).expect("the cap passes"), Event::SearchHeld) {}
        crate::backend::searchhold::on_search_held(&mut out, &mut st, &tb, &pool);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 3, "the walking query answers: {lines:?}");
        assert!(lines[2].contains(r#""walking":true"#), "the partial answer says walking");
        let _ = release.send(());
        walk_landed(&events_rx, &st, |_| false);
        on_search_walked(&mut out, &mut st, &tb, &pool);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 3, "the landing re-runs once: {lines:?}");
        assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(1), "the landing carries its own gen");
        assert!(lines[2].contains(r#""walking":false"#), "the landing says walking false");
        assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(2), "both matches after landing");
        on_search_walked(&mut out, &mut st, &tb, &pool);
        assert!(out.is_empty(), "no second landing re-run");
    }

    // An undo through the wire patches the index with no walk, even when a failing older step stops the reversal.
    #[test]
    fn an_undo_patches_the_session_even_when_an_older_step_fails() {
        use crate::backend::undo::{Entry, ItemIdentity, Step};
        let d = TestDir::new("sessionundo");
        d.file("alpha-new.txt", "a");
        d.file("alpha-old.txt", "squatter");
        d.file("beta-new.txt", "b");
        d.file("gamma-new.txt", "c");
        let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
        let tb = Tables::load();
        let (events, events_rx) = crate::backend::events::channel::<Event>();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        let (results, _done) = channel();
        let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let cache = Cache::new();
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
        let mut fsinfo = FsInfo::new(events.clone());
        let poller = super::super::watchpoll::Poller::new(events.clone());
        let mut out = Vec::new();
        let mut ask = |line: &str, st: &mut State, ops: &mut Ops| -> Vec<String> {
            assert!(
                handle_line(line, &mut out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), &mut watch, &mut peeks, &mut fsinfo, &poller, &events, &mut Gate::new(events.clone()))
                    == Control::Continue
            );
            String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect()
        };
        let moved = |old: &str, new: &str| {
            let identity = ItemIdentity::record(&std::fs::metadata(d.path().join(new)).expect("the renamed file stats"));
            Step::Moved { from: d.path().join(old), to: d.path().join(new), before: identity.clone(), after: identity, rename_fat: false }
        };
        // Older entry: alpha's reversal finds its old name taken, so it fails after beta's reverts.
        ops.journal.push(Entry { op: "rename".to_string(), steps: vec![moved("alpha-old.txt", "alpha-new.txt"), moved("beta-old.txt", "beta-new.txt")] });
        ops.journal.push(Entry { op: "rename".to_string(), steps: vec![moved("gamma-old.txt", "gamma-new.txt")] });
        ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut st, &mut ops);
        walk_landed(&events_rx, &st, |_| false);
        let total = |lines: &[String]| crate::json::field_usize(lines.last().expect("a searched line"), "n");
        assert_eq!(total(&ask(r#"{"c":"searchquery","query":"gamma-new","gen":1,"count":100}"#, &mut st, &mut ops)), Some(1));
        let opened = st.search_session.as_ref().expect("a session stands").probes_opened();
        let undone = ask(r#"{"c":"undo"}"#, &mut st, &mut ops);
        assert!(undone[0].contains(r#""t":"undone""#), "the whole undo answers undone: {undone:?}");
        assert_eq!(total(&ask(r#"{"c":"searchquery","query":"gamma-old","gen":2,"count":100}"#, &mut st, &mut ops)), Some(1), "gamma-old shows after the undo");
        assert_eq!(total(&ask(r#"{"c":"searchquery","query":"gamma-new","gen":3,"count":100}"#, &mut st, &mut ops)), Some(0), "gamma-new is gone after the undo");
        let failed = ask(r#"{"c":"undo"}"#, &mut st, &mut ops);
        assert!(failed[0].contains(r#""t":"error""#), "the partial undo answers its error: {failed:?}");
        assert!(d.path().join("beta-old.txt").exists(), "the newer step reverted on disk");
        assert_eq!(total(&ask(r#"{"c":"searchquery","query":"beta-old","gen":4,"count":100}"#, &mut st, &mut ops)), Some(1), "beta-old shows after the partial undo");
        assert_eq!(total(&ask(r#"{"c":"searchquery","query":"beta-new","gen":5,"count":100}"#, &mut st, &mut ops)), Some(0), "beta-new is gone after the partial undo");
        let session = st.search_session.as_ref().expect("a session stands");
        assert!(!session.walking() && session.probes_opened() == opened, "both undos patch with no walk");
    }

    // A newfile through the wire patches the index itself; no watch event is dispatched, so only the own-write patch can show it.
    #[test]
    fn a_newfile_patches_the_session_with_no_watch_event() {
        let d = TestDir::new("sessionnewfile");
        d.dir("sub");
        d.file("other.txt", "x");
        let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
        let tb = Tables::load();
        let (events, events_rx) = crate::backend::events::channel::<Event>();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        let (results, _done) = channel();
        let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let cache = Cache::new();
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
        let mut fsinfo = FsInfo::new(events.clone());
        let poller = super::super::watchpoll::Poller::new(events.clone());
        let mut out = Vec::new();
        let mut ask = |line: &str, st: &mut State, ops: &mut Ops| -> Vec<String> {
            assert!(
                handle_line(line, &mut out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), &mut watch, &mut peeks, &mut fsinfo, &poller, &events, &mut Gate::new(events.clone()))
                    == Control::Continue
            );
            String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect()
        };
        ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, root, root), &mut st, &mut ops);
        walk_landed(&events_rx, &st, |_| false);
        let last = |lines: &[String], key: &str| crate::json::field_usize(lines.last().expect("a searched line"), key);
        let first = ask(r#"{"c":"searchquery","query":"zz","gen":1,"count":100}"#, &mut st, &mut ops);
        assert_eq!((last(&first, "n"), last(&first, "gen")), (Some(0), Some(1)), "the first query finds no zz: {first:?}");
        let created = ask(&format!(r#"{{"c":"newfile","path":"{}","name":"zzfile.txt","id":7}}"#, root), &mut st, &mut ops);
        assert!(d.path().join("zzfile.txt").exists(), "the newfile lands on disk: {created:?}");
        let next = ask(r#"{"c":"searchquery","query":"zz","gen":2,"count":100}"#, &mut st, &mut ops);
        assert_eq!((last(&next, "n"), last(&next, "gen")), (Some(1), Some(2)), "the backend's own create shows in the next query: {next:?}");
        assert!(next.last().expect("a searched line").contains(r#""walking":false"#), "that answer lands after the walk");
    }

    // A cancel over a parked walk answers the held key, keeps the session and its partial index; a bare cancel answers only its searched line.
    #[test]
    fn a_cancel_stops_the_walk_and_keeps_the_session() {
        let d = TestDir::new("sessioncancel");
        for i in 0..12 {
            d.dir(&format!("d{}", i));
        }
        d.file("bench.txt", "");
        let root = d.path().to_str().expect("the sandbox path is utf-8").to_string();
        let tb = Tables::load();
        let (events, events_rx) = crate::backend::events::channel::<Event>();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        let (results, _done) = channel();
        let pool = Pool::new(1, results, d.join("cache"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let cache = Cache::new();
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
        let mut fsinfo = FsInfo::new(events.clone());
        let poller = super::super::watchpoll::Poller::new(events.clone());
        let mut out = Vec::new();
        let ask = |line: &str,
                   out: &mut Vec<u8>,
                   st: &mut State,
                   ops: &mut Ops,
                   watch: &mut Watch,
                   peeks: &mut PeekWatch,
                   fsinfo: &mut FsInfo,
                   poller: &super::super::watchpoll::Poller| {
            assert!(
                handle_line(line, out, st, &tb, &pool, &cache, ops, &mut flashslot::Ctx::new(channel().0), watch, peeks, fsinfo, poller, &events, &mut Gate::new(events.clone()))
                    == Control::Continue
            );
        };

        // Park the walk at its last directory of thirteen, so the cancel lands mid-walk.
        let (parked, release) = open_held(&mut st, &root, &events, 13);
        parked.recv_timeout(WIRE_DEADLINE).expect("the walk parks at its last directory");
        ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        assert!(out.is_empty(), "the key waits for the walk");
        let opened = st.search_session.as_ref().expect("a session stands").probes_opened();
        ask(r#"{"c":"searchcancel"}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 3, "the cancel re-answers listed, rows, searched: {lines:?}");
        assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(1), "the re-answer keeps the standing gen");
        assert!(lines[2].contains(r#""cancelled":true"#), "the terminal line names the stop");
        assert!(lines[2].contains(r#""walking":false"#), "the stopped walk says walking false");
        assert!(st.search_session.is_some(), "the session stands after a cancel");
        // The landing event still arrives, perhaps behind the held key's cap event; the cancel swallowed the landing, so no re-run follows it.
        let _ = release.send(());
        walk_landed(&events_rx, &st, |_| true);
        on_search_walked(&mut out, &mut st, &tb, &pool);
        assert!(out.is_empty(), "no landing re-run after a cancel");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(
            st.search_session.as_ref().expect("a session stands").probes_opened(),
            opened,
            "no directory opens after the cancel"
        );
        // A later query is still answered, from the kept partial index.
        ask(r#"{"c":"searchquery","query":"bench","gen":2,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 3, "the kept index answers: {lines:?}");
        assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(2));
        assert!(lines[2].contains(r#""cancelled":false"#), "an ordinary answer is not a cancel");
        let frozen = crate::json::field_usize(&lines[2], "scanned").expect("scanned");
        ask(r#"{"c":"searchquery","query":"benc","gen":3,"count":100}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(
            crate::json::field_usize(&lines[2], "scanned"),
            Some(frozen),
            "the index stays frozen across later queries"
        );

        // A cancel with no standing query answers only its searched line; with no walk running it stays silent.
        ask(r#"{"c":"searchclose"}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        out.clear();
        let (parked2, release2) = open_held(&mut st, &root, &events, 13);
        parked2.recv_timeout(WIRE_DEADLINE).expect("the second walk parks at its last directory");
        ask(r#"{"c":"searchcancel"}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        let lines: Vec<String> = String::from_utf8(std::mem::take(&mut out)).expect("utf-8").lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 1, "no standing query answers only searched: {lines:?}");
        assert_eq!(crate::json::field_str(&lines[0], "t").as_deref(), Some("searched"));
        assert!(lines[0].contains(r#""cancelled":true"#), "the line names the stop");
        assert!(st.search_session.is_some(), "the session stands after a bare cancel");
        let _ = release2.send(());
        ask(r#"{"c":"searchcancel"}"#, &mut out, &mut st, &mut ops, &mut watch, &mut peeks, &mut fsinfo, &poller);
        assert!(out.is_empty(), "no walk running stays silent");
    }
