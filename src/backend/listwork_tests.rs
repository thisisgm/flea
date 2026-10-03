use super::*;
use crate::backend::opsreq::OpMsg;
use crate::backend::testdir::TestDir;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

fn one_row() -> Listing {
    let mut l = Listing::new();
    l.push("a.txt", false);
    l
}

// Issue 144: the scan is what held the loop. With it on its own thread the loop's channel keeps
// delivering while the scan is still blocked, which is what lets a transferprogress line through.
#[test]
fn a_slow_scan_does_not_hold_the_loop_or_its_events() {
    let (tx, rx) = channel::<Event>();
    let (started_tx, started_rx) = channel::<()>();
    let (release_tx, release_rx) = channel::<()>();
    spawn_scan(tx.clone(), "/slow".into(), false, move |_, _| {
        started_tx.send(()).expect("the test is waiting");
        release_rx.recv().expect("the test releases the scan");
        Ok((one_row(), 1.0))
    }, |_| panic!("a successful scan must not stat for a mode"));
    // The scan is provably in flight and blocked, so anything the loop reads next it read freely.
    started_rx.recv_timeout(Duration::from_secs(5)).expect("the scan started");
    tx.send(Event::Op(OpMsg::Progress { id: 1, index: 0, name: "big.bin".into(), bytes: 1, total: 2, scanned: 0 }))
        .expect("the loop's channel is open");
    let first = rx.recv_timeout(Duration::from_millis(100)).expect("an event lands while the scan is blocked");
    assert!(matches!(first, Event::Op(_)), "the progress line is what the loop reads first");
    release_tx.send(()).expect("release the scan");
    let done = loop {
        let event = rx.recv_timeout(Duration::from_secs(5)).expect("the listing lands");
        if let Event::List(d) = event {
            break d;
        }
    };
    assert_eq!(done.result.expect("the fake scan succeeds").0.len(), 1, "and it is the scan's own answer");
    assert!(done.mode.is_none(), "a successful scan answers no mode and pays for no stat");
}

// Issue 144, the failure half: once the scan has failed, the mode stat alone can still hang on the
// same stalled mount. It now runs on the scan's own thread, so while it is blocked the loop keeps
// delivering, and the mode it read rides the answer instead of being stat'd after the answer lands.
#[test]
fn a_slow_mode_stat_after_a_failed_scan_does_not_hold_the_loop() {
    let (tx, rx) = channel::<Event>();
    let (stat_tx, stat_rx) = channel::<()>();
    let (release_tx, release_rx) = channel::<()>();
    spawn_scan(tx.clone(), "/stalled".into(), false,
        |_, _| Err(FleaError { where_: "scan".into(), path: "/stalled".into(), msg: "refused".into() }),
        move |_| {
            stat_tx.send(()).expect("the test is waiting");
            release_rx.recv().expect("the test releases the stat");
            0o40750
        });
    // The stat is provably in flight and blocked, so anything the loop reads next it read freely.
    stat_rx.recv_timeout(Duration::from_secs(5)).expect("the mode stat started on the worker");
    tx.send(Event::Op(OpMsg::Progress { id: 1, index: 0, name: "big.bin".into(), bytes: 1, total: 2, scanned: 0 }))
        .expect("the loop's channel is open");
    let first = rx.recv_timeout(Duration::from_millis(100)).expect("an event lands while the stat is blocked");
    assert!(matches!(first, Event::Op(_)), "the loop is not held by the failed scan's metadata");
    release_tx.send(()).expect("release the stat");
    let done = loop {
        let event = rx.recv_timeout(Duration::from_secs(5)).expect("the failed scan lands");
        if let Event::List(d) = event {
            break d;
        }
    };
    assert!(done.result.is_err(), "the scan still reports its failure");
    assert_eq!(done.mode, Some(0o40750), "and the mode the worker read rides the answer to the loop");
}

// The same guarantee from the loop's side: the error line carries the mode the answer brought, so a
// missing path never makes the loop stat for one. A fresh stat would answer 0 here and drop the
// field, so the field's presence on a path with no metadata is the proof it was read off the loop.
#[test]
fn a_failed_scan_answers_the_mode_the_worker_read_not_a_fresh_stat() {
    let d = TestDir::new("listwork-failed-mode");
    let tb = Tables::load();
    let (ev_tx, _ev_rx) = channel::<Event>();
    let mut st = State::new(crate::backend::dirsizeworker::Worker::new(ev_tx.clone()));
    let (thumbs_tx, _thumbs_rx) = channel();
    let pool = Pool::new(1, thumbs_tx, d.join("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let mut watch = Watch::start(ev_tx.clone());
    let mut fsinfo = FsInfo::new(ev_tx.clone());
    let missing = d.join("gone").to_string_lossy().into_owned();
    let landed = Landed {
        job: Job { path: missing.clone(), first: 10, line: "{}".into() },
        done: Done {
            result: Err(FleaError { where_: "scan".into(), path: missing, msg: "refused".into() }),
            mode: Some(0o40750),
        },
    };
    let mut out = Vec::new();
    finish(&mut out, &mut st, &tb, &pool, &mut watch, &mut fsinfo, landed);
    let text = String::from_utf8(out).expect("the error line is UTF-8");
    assert!(text.contains(r#""t":"error""#), "the failure is still an error line: {text}");
    assert!(text.contains(&format!(r#""mode":{}"#, 0o40750)), "the line carries the mode the worker read: {text}");
}

// The wire's order is the order the requests arrived in: what lands while a scan is out waits for
// it, then replays in order, so list/list/paths still sees both listings and the old numbering.
#[test]
fn requests_that_arrive_while_a_scan_is_out_replay_in_order() {
    let mut gate = Gate::new();
    assert!(!gate.holds(), "nothing is out before the first list");
    gate.begin(Job { path: "/a".into(), first: 0, line: "{}".into() });
    assert!(gate.holds());
    gate.defer("first".into());
    gate.defer("second".into());
    assert_eq!(gate.take().map(|job| job.path), Some("/a".to_string()));
    assert_eq!(gate.next().as_deref(), Some("first"), "the first request replays first");
    assert_eq!(gate.next().as_deref(), Some("second"), "and the second after it");
    assert_eq!(gate.next(), None);
}

// A change seen while a scan is out is answered after it, against the descriptor that became
// current, so a burst in the directory just left answers nothing.
#[test]
fn a_change_seen_during_a_scan_is_decided_once_the_scan_lands() {
    let mut gate = Gate::new();
    gate.begin(Job { path: "/a".into(), first: 0, line: "{}".into() });
    gate.note_change(7);
    gate.note_change(9);
    assert_eq!(gate.take_changes(), vec![7, 9], "every descriptor is kept for the decision");
    assert!(gate.take_changes().is_empty(), "and the next scan starts with none");
}
