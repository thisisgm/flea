use super::*;
use crate::backend::searchwalkhold::wait_walk_landed;
use crate::backend::testdir::TestDir;

const WIRE_DEADLINE: Duration = Duration::from_secs(30);

// One backend over a sandbox, driven line by line the way the loop drives it, with the walk parked at its last directory.
struct Wire {
    tb: Tables,
    events: crate::backend::events::Sender<Event>,
    events_rx: Receiver<Event>,
    st: State,
    pool: Pool,
    cache: Cache,
    ops: Ops,
    flash: flashslot::Ctx,
    watch: Watch,
    peeks: PeekWatch,
    fsinfo: FsInfo,
    poller: super::super::watchpoll::Poller,
    out: Vec<u8>,
    release: std::sync::mpsc::Sender<()>,
    // When the loop received the walk's landing, read against the session's own cap.
    landed_at: Option<Instant>,
}

impl Wire {
    // Twelve directories with two bench matches; the walk parks before its last, so a key arrives while it runs.
    fn parked(d: &TestDir) -> Wire {
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
        let (tx, _rx) = channel();
        let (watch, peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
        let (fsinfo, poller) = (FsInfo::new(events.clone()), super::super::watchpoll::Poller::new(events.clone()));
        let probes = Arc::new(crate::backend::searchwalk::WalkProbes::new());
        let (parked, release) = probes.test_hold_walk_in("docs");
        st.search_session = Some(Session::open_async_with_probes(&root, &root, false, events.clone(), probes));
        parked.recv_timeout(WIRE_DEADLINE).expect("the walk parks at docs");
        Wire { tb, events, events_rx, st, pool, cache: Cache::new(), ops: Ops::new(tx), flash: flashslot::Ctx::new(channel().0), watch, peeks, fsinfo, poller, out: Vec::new(), release, landed_at: None }
    }

    // The lines the wire wrote since the last take.
    fn taken(&mut self) -> Vec<String> {
        String::from_utf8(std::mem::take(&mut self.out)).expect("utf-8").lines().map(str::to_string).collect()
    }

    fn ask(&mut self, line: &str) -> Vec<String> {
        let w = &mut *self;
        let control = handle_line(line, &mut w.out, &mut w.st, RequestContext { tb: &w.tb, pool: &w.pool, cache: &w.cache, ops: &mut w.ops, flash: &mut w.flash, watch: &mut w.watch, peeks: &mut w.peeks, fsinfo: &mut w.fsinfo, poller: &w.poller, loop_tx: &w.events, gate: &mut Gate::new(w.events.clone()) });
        assert!(control == Control::Continue);
        self.taken()
    }

    // The next event of this kind, as the loop would receive it; any other event in between is skipped.
    fn next(&self, kind: fn(&Event) -> bool) {
        while !kind(&self.events_rx.recv_timeout(WIRE_DEADLINE).expect("the event arrives")) {}
    }

    // Releases the parked walk and answers its landing as the loop does; walk progress before the landing is passed over.
    fn land(&mut self) -> Vec<String> {
        let _ = self.release.send(());
        let session = self.st.search_session.as_ref().expect("a session stands");
        wait_walk_landed(&self.events_rx, session, WIRE_DEADLINE, |_| true);
        self.landed_at = Some(Instant::now());
        on_search_walked(&mut self.out, &mut self.st, &self.tb, &self.pool);
        self.taken()
    }
}

fn kinds(lines: &[String]) -> Vec<String> {
    lines.iter().map(|line| crate::json::field_str(line, "t").unwrap_or_default()).collect()
}

// A key typed while the walk runs answers once when the walk lands inside the cap: one rows line, from the whole index.
#[test]
fn a_key_while_the_walk_runs_answers_once_when_the_walk_lands_inside_the_cap() {
    let d = TestDir::new("sessionhold");
    let mut wire = Wire::parked(&d);
    let mut lines = wire.ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#);
    let cap = wire.st.search_session.as_ref().and_then(|session| session.hold_until).expect("the key opened the hold window");
    lines.extend(wire.land());
    assert!(wire.landed_at.expect("the walk landed") < cap, "the loop met the landing before the cap, so the cap never answered");
    let rows = lines.iter().filter(|line| crate::json::field_str(line, "t").as_deref() == Some("rows")).count();
    assert_eq!(rows, 1, "one rows line for the key, so one results frame: {lines:?}");
    assert_eq!(lines.len(), 3, "listed, rows, searched once: {lines:?}");
    assert!(lines[2].contains(r#""walking":false"#), "the one answer is the landed walk's");
    assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(1), "it carries the key's gen");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(2), "both matches, from the whole index");
}

// Keys typed while one is held replace it: the landing answers only the newest, and a stale gen stays silent.
#[test]
fn a_newer_key_replaces_the_held_one_and_the_landing_answers_only_it() {
    let d = TestDir::new("sessionholdnewer");
    let mut wire = Wire::parked(&d);
    assert!(wire.ask(r#"{"c":"searchquery","query":"ben","gen":1,"count":100}"#).is_empty(), "the first key waits");
    assert!(wire.ask(r#"{"c":"searchquery","query":"bench","gen":2,"count":100}"#).is_empty(), "the second key waits too");
    assert!(wire.ask(r#"{"c":"searchquery","query":"be","gen":1,"count":100}"#).is_empty(), "a stale gen answers nothing");
    let lines = wire.land();
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "one answer for both keys: {lines:?}");
    assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(2), "the newest key's gen");
    assert!(lines[2].contains(r#""walking":false"#), "from the whole index");
}

// Past the cap the held key answers from the partial index, and the landing re-runs it once with its own gen.
#[test]
fn past_the_cap_the_partial_index_answers_and_the_landing_completes_it() {
    let d = TestDir::new("sessionholdcap");
    let mut wire = Wire::parked(&d);
    let asked = Instant::now();
    assert!(wire.ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#).is_empty(), "the key waits for the walk");
    wire.next(|event| matches!(event, Event::SearchHeld));
    assert!(asked.elapsed() >= crate::backend::searchhold::HOLD, "the cap's event comes no sooner than the cap");
    crate::backend::searchhold::on_search_held(&mut wire.out, &mut wire.st, &wire.tb, &wire.pool);
    let lines = wire.taken();
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "the partial answer: {lines:?}");
    assert!(lines[2].contains(r#""walking":true"#), "it says the walk still runs");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(1), "the partial index holds one match");
    let lines = wire.land();
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "the landing re-runs once: {lines:?}");
    assert!(lines[2].contains(r#""walking":false"#) && lines[2].contains(r#""gen":1"#), "with its own gen, landed");
    assert_eq!(crate::json::field_usize(&lines[0], "n"), Some(2), "both matches after the landing");
}

// Once the cap passed, the walk is a slow source: a later key answers at once from the partial index, and the landing completes it.
#[test]
fn after_the_cap_a_later_key_answers_at_once() {
    let d = TestDir::new("sessionholdlater");
    let mut wire = Wire::parked(&d);
    assert!(wire.ask(r#"{"c":"searchquery","query":"ben","gen":1,"count":100}"#).is_empty(), "the first key waits");
    wire.next(|event| matches!(event, Event::SearchHeld));
    crate::backend::searchhold::on_search_held(&mut wire.out, &mut wire.st, &wire.tb, &wire.pool);
    assert_eq!(kinds(&wire.taken()), ["listed", "rows", "searched"], "the cap answers the first key");
    let lines = wire.ask(r#"{"c":"searchquery","query":"bench","gen":2,"count":100}"#);
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "the later key answers at once: {lines:?}");
    assert!(lines[2].contains(r#""walking":true"#) && lines[2].contains(r#""gen":2"#), "from the partial index, with its own gen");
    let lines = wire.land();
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "the landing re-runs the later key once: {lines:?}");
    assert!(lines[2].contains(r#""walking":false"#) && lines[2].contains(r#""gen":2"#), "with its own gen, landed");
}

// A cancel while a key is held answers that key once from the partial index, as a stop.
#[test]
fn a_cancel_answers_the_held_key_as_its_stop() {
    let d = TestDir::new("sessionholdcancel");
    let mut wire = Wire::parked(&d);
    assert!(wire.ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#).is_empty(), "the key waits for the walk");
    let lines = wire.ask(r#"{"c":"searchcancel"}"#);
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "the stop answers the held key: {lines:?}");
    assert!(lines[2].contains(r#""cancelled":true"#) && lines[2].contains(r#""gen":1"#), "as the key's own stop");
    assert!(wire.land().is_empty(), "no landing answer after the stop");
    crate::backend::searchhold::on_search_held(&mut wire.out, &mut wire.st, &wire.tb, &wire.pool);
    assert!(wire.taken().is_empty(), "nothing is left for the cap to answer");
}

// An emptied query answers the open folder's own listing at once, since no walk changes it, and drops the held key.
#[test]
fn an_emptied_query_answers_at_once_while_the_walk_runs() {
    let d = TestDir::new("sessionholdempty");
    let mut wire = Wire::parked(&d);
    assert!(wire.ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#).is_empty(), "the key waits for the walk");
    let lines = wire.ask(r#"{"c":"searchquery","query":"","gen":2,"count":100}"#);
    assert_eq!(kinds(&lines), ["listed", "rows", "searched"], "the folder answers at once: {lines:?}");
    assert_eq!(crate::json::field_usize(&lines[2], "gen"), Some(2), "with the empty query's gen");
    crate::backend::searchhold::on_search_held(&mut wire.out, &mut wire.st, &wire.tb, &wire.pool);
    assert!(wire.taken().is_empty(), "the held key is gone, so the cap answers nothing");
}

// Walk progress shares the landing event, so a walk still out after its release is waited for, never taken for its landing.
#[test]
fn a_landing_wait_skips_walk_progress_while_the_walk_is_still_out() {
    let d = TestDir::new("sessionholdprogress");
    let mut wire = Wire::parked(&d);
    assert!(
        wire.ask(r#"{"c":"searchquery","query":"bench","gen":1,"count":100}"#)
            .is_empty(),
        "the key waits for the walk"
    );
    wire.next(|event| matches!(event, Event::SearchHeld));
    crate::backend::searchhold::on_search_held(&mut wire.out, &mut wire.st, &wire.tb, &wire.pool);
    assert_eq!(
        kinds(&wire.taken()),
        ["listed", "rows", "searched"],
        "the cap answers from the partial index"
    );
    let probes = Arc::clone(
        &wire
            .st
            .search_session
            .as_ref()
            .expect("a session stands")
            .probes,
    );
    let (parked, release) = probes.test_hold_walk_late_in("docs");
    // The walk parks again after its first release for three progress ticks, so progress arrives before its landing.
    let late = std::thread::spawn(move || {
        parked
            .recv_timeout(WIRE_DEADLINE)
            .expect("the walk is still out after its first release");
        std::thread::sleep(Duration::from_millis(
            3 * crate::backend::searchsession::BATCH_MS,
        ));
        let _ = release.send(());
    });
    let lines = wire.land();
    late.join().expect("the late hold thread ends");
    assert_eq!(
        kinds(&lines),
        ["listed", "rows", "searched"],
        "the landing re-runs once: {lines:?}"
    );
    assert!(
        lines[2].contains(r#""walking":false"#) && lines[2].contains(r#""gen":1"#),
        "with its own gen, landed"
    );
}

// The cap is the listing swap's own, read from the client that holds a listing.
#[test]
fn the_search_hold_is_the_listing_swap_cap() {
    let swap = include_str!("../../ui/js/Swap.js");
    let line = swap.lines().find(|line| line.starts_with("var HOLD_MS = ")).expect("Swap.js names HOLD_MS");
    assert_eq!(line, format!("var HOLD_MS = {}", crate::backend::searchhold::HOLD.as_millis()));
}
