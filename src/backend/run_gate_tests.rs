use super::*;
use crate::backend::scanhold::{self, Stage};
use crate::backend::searchwalkhold::wait_walk_landed;
use crate::backend::testdir::TestDir;
use crate::json::field_str;

// Bounds a wait for an event the test caused, so a missing one fails instead of hanging.
const EVENT_DEADLINE: Duration = Duration::from_secs(10);

// Everything run() owns, built the way it builds them, with the loop's channel read by the test instead.
struct Loop {
    dir: TestDir,
    out: Vec<u8>,
    st: State,
    tb: Tables,
    pool: Pool,
    cache: Cache,
    ops: Ops,
    flash: flashslot::Ctx,
    watch: Watch,
    peeks: PeekWatch,
    fsinfo: FsInfo,
    poller: super::super::watchpoll::Poller,
    gate: Gate,
    tx: crate::backend::events::Sender<Event>,
    rx: Receiver<Event>,
    _thumbs: Receiver<Done>,
    _ops: Receiver<OpMsg>,
    _flash: Receiver<flashslot::FlashMsg>,
}

impl Loop {
    fn new(tag: &str) -> Loop {
        let dir = TestDir::new(tag);
        let tb = Tables::load();
        let (tx, rx) = crate::backend::events::channel::<Event>();
        let st = State::new(super::super::dirsizeworker::Worker::new(tx.clone()));
        let (thumbs_tx, _thumbs) = channel();
        let pool = Pool::new(1, thumbs_tx, dir.join("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let cache = Cache::at(dir.join("thumbs"));
        let (ops_tx, _ops) = channel();
        let (flash_tx, _flash) = channel();
        let (watch, peeks) = (Watch::start(tx.clone()), PeekWatch::start(tx.clone()));
        let (fsinfo, poller) = (FsInfo::new(tx.clone()), super::super::watchpoll::Poller::new(tx.clone()));
        let gate = Gate::new(tx.clone());
        Loop { dir, out: Vec::new(), st, tb, pool, cache, ops: Ops::new(ops_tx), flash: flashslot::Ctx::new(flash_tx), watch, peeks,
               fsinfo, poller, gate, tx, rx, _thumbs, _ops, _flash }
    }

    // One event through the loop's own dispatch, exactly as run() hands it over.
    fn feed(&mut self, event: Event) -> Control {
        dispatch(event, &mut self.out, &mut self.st, &self.tb, &self.pool, &self.cache, &mut self.ops, &mut self.flash,
                 &mut self.watch, &mut self.peeks, &mut self.fsinfo, &self.poller, &self.tx, &mut self.gate)
    }

    fn request(&mut self, line: &str) -> Control {
        self.feed(Event::Request(line.to_string()))
    }

    // Reads the loop's channel up to the awaited event; a remote folder's figures and polls also arrive there, and no check reads them.
    fn wait(&self, wanted: fn(&Event) -> bool) -> Event {
        loop {
            let event = self.rx.recv_timeout(EVENT_DEADLINE).expect("the awaited event reaches the loop's channel");
            if wanted(&event) {
                return event;
            }
        }
    }

    // The walk's landing, past the walk progress that shares its event and the events wait() passes over.
    fn landing(&self) -> Event {
        let session = self.st.search_session.as_ref().expect("a session stands");
        wait_walk_landed(&self.rx, session, EVENT_DEADLINE, |_| true);
        Event::SearchWalked
    }

    fn said(&mut self) -> String {
        String::from_utf8(std::mem::take(&mut self.out)).expect("the wire is UTF-8")
    }
}

fn is_list(event: &Event) -> bool {
    matches!(event, Event::List(_))
}

fn is_changed(event: &Event) -> bool {
    matches!(event, Event::Changed(_))
}

// The sandbox mapped onto an nfs mount, so every folder in it takes the scan thread and holds the gate.
fn remote_body(dir: &Path) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", dir.to_string_lossy())
}

fn list(path: &Path) -> String {
    format!(r#"{{"c":"list","path":"{}","first":10}}"#, crate::json::escape(&path.to_string_lossy()))
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// Sample: protocol.sh's list A, list B, paths with listing 1, here with B's scan provably out when the requests and the close arrive.
#[test]
fn requests_a_close_and_a_burst_seen_during_a_scan_wait_for_it_and_replay_in_order() {
    let mut l = Loop::new("rungate-order");
    let (old, new) = (l.dir.dir("old"), l.dir.dir("new"));
    l.dir.file("old/a.txt", "a");
    let kept = l.dir.file("new/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    assert!(l.request(&list(&old)) == Control::Continue);
    assert!(l.gate.holds(), "the remote list is out on the scan thread");
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let first = l.said();
    assert!(first.contains(r#""t":"listed""#), "the first folder lands before the second is asked for: {first}");
    let stale = l.st.generation;
    let hold = scanhold::hold(&text(&new), Stage::Read);
    assert!(l.request(&list(&new)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the second scan is out and held at its read");
    assert!(l.request(&format!(r#"{{"c":"paths","rows":[0],"listing":{stale}}}"#)) == Control::Continue);
    assert!(l.request(r#"{"c":"paths","rows":[0]}"#) == Control::Continue);
    // A burst in the folder being left, which is still the current watch until the scan lands.
    std::fs::create_dir(old.join("burst")).expect("the sandbox takes a folder");
    let burst = l.wait(is_changed);
    assert!(l.feed(burst) == Control::Continue);
    assert!(l.feed(Event::Closed) == Control::Continue, "a close while the scan is out waits for its answer");
    let early = l.said();
    assert!(early.is_empty(), "nothing is answered while the scan is out: {early}");
    hold.release();
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Quit, "the closed loop leaves once the scan and every queued request are answered");
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 4, "listed, rows, then the two requests in order, and no changed line for the folder left: {said}");
    assert_eq!(field_str(lines[0], "t").as_deref(), Some("listed"), "{said}");
    assert_eq!(field_str(lines[0], "path"), Some(text(&new)), "the scan's own folder became the listing: {said}");
    assert_eq!(field_str(lines[1], "t").as_deref(), Some("rows"), "{said}");
    assert_eq!(field_str(lines[2], "where").as_deref(), Some("stale"), "the first request named the old numbering, so it is refused: {said}");
    assert_eq!(lines[3], paths_line(&[text(&kept)]), "the second request is answered against the new listing");
}

// A decode failure while a scan is out reports at once, then waits as a close does, so the requests read before it are answered.
#[test]
fn a_read_error_while_a_scan_is_out_waits_for_it_and_answers_the_queued_request() {
    let mut l = Loop::new("rungate-readerror");
    let new = l.dir.dir("new");
    let kept = l.dir.file("new/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let hold = scanhold::hold(&text(&new), Stage::Read);
    assert!(l.request(&list(&new)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the scan is out and held at its read");
    assert!(l.request(r#"{"c":"paths","rows":[0]}"#) == Control::Continue);
    let failure = crate::error::from_io("read", "stdin", &std::io::Error::from(std::io::ErrorKind::InvalidData));
    assert!(l.feed(Event::ReadError(failure)) == Control::Continue, "a decode failure while the scan is out waits for its answer");
    hold.release();
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Quit, "the loop leaves once the scan and the queued request are answered");
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 4, "the report, then listed, rows and the queued request: {said}");
    assert_eq!(field_str(lines[0], "t").as_deref(), Some("error"), "{said}");
    assert_eq!(field_str(lines[1], "t").as_deref(), Some("listed"), "{said}");
    assert_eq!(lines[3], paths_line(&[text(&kept)]), "the request read before the failure is answered against the new listing");
}

// Issue 144: the loop keeps delivering a transfer's progress while a remote scan is out, so the gate holds requests and nothing else.
#[test]
fn a_transfer_progress_line_goes_out_while_a_scan_is_out() {
    let mut l = Loop::new("rungate-progress");
    let new = l.dir.dir("new");
    l.dir.file("new/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let hold = scanhold::hold(&text(&new), Stage::Read);
    assert!(l.request(&list(&new)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the scan is out and held at its read");
    assert!(l.gate.holds(), "the scan holds the gate");
    let progress = Event::Op(OpMsg::Progress { id: 1, index: 0, name: "big.bin".into(), bytes: 1, total: 2, scanned: 0 });
    assert!(l.feed(progress) == Control::Continue);
    assert_eq!(l.said(), crate::backend::opsreq::transferprogress_line(1, 0, "big.bin", 1, 2, 0) + "\n", "the progress line is not held behind the scan");
    hold.release();
}

// A change inside the folder being scanned is answered once the scan lands, after its rows.
#[test]
fn a_burst_in_the_folder_being_scanned_is_answered_once_it_lands() {
    let mut l = Loop::new("rungate-burst");
    let new = l.dir.dir("new");
    l.dir.file("new/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let hold = scanhold::hold(&text(&new), Stage::Read);
    assert!(l.request(&list(&new)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the scan is out and held at its read");
    // The worker armed the folder's watch before its read, so this burst carries the descriptor that becomes current.
    std::fs::create_dir(new.join("burst")).expect("the sandbox takes a folder");
    let burst = l.wait(is_changed);
    assert!(l.feed(burst) == Control::Continue);
    let early = l.said();
    assert!(early.is_empty(), "the burst waits for the scan: {early}");
    hold.release();
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 3, "listed, rows, then one changed line: {said}");
    assert_eq!(field_str(lines[1], "t").as_deref(), Some("rows"), "{said}");
    assert_eq!(lines[2], changed_line(&new), "the burst names the folder that became current");
}

// An empty query on a remote session folder scans on the worker, so the loop is free and the folder answer lands in order.
#[test]
fn an_empty_query_on_a_remote_folder_scans_off_the_loop_and_answers_once_it_lands() {
    let mut l = Loop::new("rungate-emptyquery");
    let folder = l.dir.dir("share");
    let kept = l.dir.file("share/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let named = crate::json::escape(&text(&folder));
    assert!(l.request(&format!(r#"{{"c":"searchopen","path":"{named}","folder":"{named}"}}"#)) == Control::Continue);
    let walked = l.landing();
    assert!(l.feed(walked) == Control::Continue);
    let opened = l.said();
    assert_eq!(opened.lines().count(), 1, "searchopened alone: {opened}");
    let hold = scanhold::hold(&text(&folder), Stage::Read);
    assert!(l.request(r#"{"c":"searchquery","query":"","gen":1,"count":10}"#) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the folder scan reached its read");
    assert!(!hold.over(), "the query returned to the loop while the folder read was still held");
    assert!(l.gate.holds(), "the folder scan is out on the scan thread");
    assert!(l.request(r#"{"c":"paths","rows":[0]}"#) == Control::Continue);
    let early = l.said();
    assert!(early.is_empty(), "nothing is answered while the folder scan is out: {early}");
    hold.release();
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 4, "listed, rows, searched, then the waiting request: {said}");
    assert_eq!(field_str(lines[0], "t").as_deref(), Some("listed"), "{said}");
    assert_eq!(field_str(lines[0], "path"), Some(text(&folder)), "{said}");
    assert_eq!(crate::json::field_usize(lines[0], "gen"), Some(1), "the folder answer carries its gen: {said}");
    assert_eq!(field_str(lines[1], "t").as_deref(), Some("rows"), "{said}");
    assert_eq!(field_str(lines[2], "t").as_deref(), Some("searched"), "{said}");
    assert_eq!(crate::json::field_usize(lines[2], "gen"), Some(1), "{said}");
    assert_eq!(crate::json::field_usize(lines[2], "n"), Some(1), "{said}");
    assert_eq!(lines[3], paths_line(&[text(&kept)]), "the waiting request reads the session's folder rows");
}

// A walk that lands while the folder scan is out is announced after the folder answer, as an inline scan orders them.
#[test]
fn a_walk_landing_during_the_folder_scan_is_announced_after_the_folder_answer() {
    let mut l = Loop::new("rungate-emptywalk");
    let folder = l.dir.dir("share");
    l.dir.dir("share/sub");
    l.dir.file("share/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let probes = Arc::new(crate::backend::searchwalk::WalkProbes::new());
    let (parked, walk) = probes.test_hold_walk_in("sub");
    let root = text(&folder);
    l.st.search_session = Some(Session::open_async_with_probes(&root, &root, false, l.tx.clone(), probes));
    parked.recv_timeout(EVENT_DEADLINE).expect("the walk parks at sub");
    let hold = scanhold::hold(&root, Stage::Read);
    assert!(l.request(r#"{"c":"searchquery","query":"","gen":1,"count":10}"#) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the folder scan reached its read");
    let _ = walk.send(());
    let walked = l.landing();
    assert!(l.feed(walked) == Control::Continue);
    let early = l.said();
    assert!(early.is_empty(), "the walk's landing waits for the folder answer: {early}");
    hold.release();
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 4, "listed, rows, searched while walking, then the walk's landing line: {said}");
    assert_eq!(field_str(lines[1], "t").as_deref(), Some("rows"), "{said}");
    assert!(lines[2].contains(r#""walking":true"#) && lines[2].contains(r#""gen":1"#), "the folder answer was claimed mid-walk: {said}");
    assert!(lines[3].contains(r#""walking":false"#) && lines[3].contains(r#""gen":1"#), "the landing closes the same gen: {said}");
}

// The next event as run() takes it: a buffered line first, then the channel, each passed through the burst collapse.
fn take(l: &mut Loop, buffered: &mut VecDeque<Event>, wait: bool) -> Option<Event> {
    loop {
        let event = match buffered.pop_front() {
            Some(event) => event,
            None if wait => l.rx.recv_timeout(EVENT_DEADLINE).expect("the awaited event reaches the loop's channel"),
            None => l.rx.try_recv().ok()?,
        };
        if let Some(event) = collapse_burst(&l.rx, event, buffered, l.st.search_session.is_some(), l.gate.holds(), &mut super::super::searchjobs::Gate::default()) {
            return Some(event);
        }
    }
}

// Sample: a searchopen and a typed burst behind a held list scan; only the newest query answers, the siblings in their places.
#[test]
fn a_query_burst_behind_a_held_scan_answers_only_its_newest_query() {
    let mut l = Loop::new("rungate-heldburst");
    let old = l.dir.dir("old");
    let rows: Vec<PathBuf> = ["o0.txt", "o1.txt"].iter().map(|name| l.dir.file(&format!("old/{name}"), "o")).collect();
    let folder = l.dir.dir("share");
    let shared: Vec<PathBuf> = ["a.txt", "b.txt", "c.txt"].iter().map(|name| l.dir.file(&format!("share/{name}"), "s")).collect();
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let hold = scanhold::hold(&text(&old), Stage::Read);
    assert!(l.request(&list(&old)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the list scan is out and held at its read");
    let named = crate::json::escape(&text(&folder));
    let query = |gen: usize| format!(r#"{{"c":"searchquery","query":"","gen":{gen},"count":10}}"#);
    let lines = [format!(r#"{{"c":"searchopen","path":"{named}","folder":"{named}"}}"#), query(1), r#"{"c":"paths","rows":[0]}"#.to_string(),
                 query(2), r#"{"c":"paths","rows":[1]}"#.to_string(), query(3), r#"{"c":"paths","rows":[2]}"#.to_string()];
    for line in lines {
        l.tx.send(Event::Request(line)).expect("the loop's channel is open");
    }
    let mut buffered = VecDeque::new();
    // Everything sent is read while the scan holds, as the loop reads it; only requests reach dispatch here.
    while let Some(event) = take(&mut l, &mut buffered, false) {
        if matches!(event, Event::Request(_)) {
            assert!(l.feed(event) == Control::Continue);
        }
    }
    assert!(l.gate.holds(), "the list scan is still out");
    let early = l.said();
    assert!(early.is_empty(), "nothing is answered while the scan is out: {early}");
    hold.release();
    // Each landing replays the waiting lines; a replayed empty query scans its folder and holds the gate again.
    loop {
        let event = take(&mut l, &mut buffered, true).expect("the loop's channel is open");
        if matches!(event, Event::Request(_) | Event::List(_)) {
            let landed = matches!(event, Event::List(_));
            assert!(l.feed(event) == Control::Continue);
            if landed && !l.gate.holds() {
                break;
            }
        }
    }
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    let answers: Vec<usize> = lines.iter().filter(|line| field_str(line, "t").as_deref() == Some("listed"))
        .filter_map(|line| crate::json::field_usize(line, "gen")).collect();
    assert_eq!(answers, vec![3], "one session answer, for the newest query: {said}");
    assert!(lines.iter().all(|line| !matches!(crate::json::field_usize(line, "gen"), Some(1 | 2))), "no older gen answers: {said}");
    let paths: Vec<usize> = lines.iter().enumerate().filter(|(_, line)| field_str(line, "t").as_deref() == Some("paths")).map(|(at, _)| at).collect();
    let answer = lines.iter().position(|line| crate::json::field_usize(line, "gen") == Some(3)).expect("the gen 3 answer");
    assert_eq!(paths.len(), 3, "the three sibling requests are answered: {said}");
    assert_eq!(lines[paths[0]], paths_line(&[text(&rows[0])]), "the first sibling reads the listed folder: {said}");
    assert_eq!(lines[paths[1]], paths_line(&[text(&rows[1])]), "the second sibling follows it: {said}");
    assert!(paths[1] < answer && answer < paths[2], "the newest query answers in its own place among its siblings: {said}");
    assert_eq!(lines[paths[2]], paths_line(&[text(&shared[2])]), "the last sibling reads the session's folder rows: {said}");
}

#[path = "run_gate_gone_tests.rs"]
mod gone;
#[path = "run_gate_unmount_tests.rs"]
mod unmount;
#[path = "run_gate_newest_tests.rs"]
mod newest;
#[path = "run_gate_alias_tests.rs"]
mod alias;
#[path = "run_gate_poll_tests.rs"]
mod poll;
#[path = "run_gate_delta_tests.rs"]
mod delta;
#[path = "run_eof_tests.rs"]
mod eof;

#[test]
fn search_listing_gate_replays_enter_the_worker_and_keep_ordinary_request_order() {
    let mut l = Loop::new("search-replay-worker");
    l.dir.file("alpha.txt", "a");
    let scope = l.dir.path().to_str().unwrap();
    l.st.search_session = Some(Session::open_sync(scope, scope, false));
    l.gate.defer(r#"{"c":"searchquery","query":"alpha","gen":1,"count":1}"#.into());
    l.gate.defer(r#"{"c":"window","start":0,"count":1}"#.into());
    l.gate.defer(r#"{"c":"searchquery","query":"alpha","gen":2,"count":1}"#.into());
    let mut jobs = super::super::searchjobs::Gate::default();
    let done = super::super::listwork::Done { result: Err(super::super::iomount::ListErr {
        error: super::super::iomount::internal_failure("list", l.dir.path()), mode: 0, watch_wd: -1,
    }) };
    assert!(dispatch_inner(Event::List(done), &mut l.out, &mut l.st, &l.tb, &l.pool, &l.cache, &mut l.ops,
        &mut l.flash, &mut l.watch, &mut l.peeks, &mut l.fsinfo, &l.poller, &l.tx, &mut l.gate, Some(&mut jobs)) == Control::Continue);
    assert!(jobs.busy());
    assert!(l.out.is_empty(), "listing replay must not score or publish on dispatch");
    let ready = l.wait(|event| matches!(event, Event::SearchReady));
    let mut buffered = VecDeque::new();
    jobs.route(ready, &mut l.st, &mut l.out, &l.tb, &l.pool, &l.tx, &mut buffered, false);
    assert!(l.out.is_empty(), "the newer replayed generation supersedes the old answer");
    let Some(Event::Request(window)) = buffered.pop_front() else { panic!("window first") };
    assert_eq!(field_str(&window, "c").as_deref(), Some("window"));
    let Some(Event::Request(query)) = buffered.pop_front() else { panic!("query second") };
    assert_eq!(crate::json::field_usize(&query, "gen"), Some(2));
    l.st.search_session.take().unwrap().close();
}

#[path = "run_newfolder_tests.rs"]
mod newfolder_tests;

#[path = "run_thumb_identity_tests.rs"]
mod thumb_identity_tests;
