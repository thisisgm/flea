use super::*;
use crate::backend::quietpanic;
use crate::backend::scanhold::{self, Stage};
use crate::backend::testdir::TestDir;
use crate::error::FleaError;
use std::os::unix::fs::MetadataExt;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

// Everything `start` and `finish` take, built the way the loop builds them.
struct Rig {
    dir: TestDir,
    tb: Tables,
    st: State,
    pool: Pool,
    watch: Watch,
    gate: Gate,
    rx: Receiver<Event>,
    _thumbs_rx: Receiver<crate::backend::thumbs::Done>,
}

fn rig(tag: &str) -> Rig {
    let dir = TestDir::new(tag);
    let tb = Tables::load();
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let st = State::new(crate::backend::dirsizeworker::Worker::new(tx.clone()));
    let (thumbs_tx, _thumbs_rx) = channel();
    let pool = Pool::new(1, thumbs_tx, dir.join("thumbs"), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let watch = Watch::start(tx.clone());
    let gate = Gate::new(tx.clone());
    Rig { dir, tb, st, pool, watch, gate, rx, _thumbs_rx }
}

impl Rig {
    // The loop's own call: a remote list returns at once, whatever the scan on its thread is doing.
    fn start(&mut self, path: &str) -> Started {
        self.start_out(&mut Vec::new(), path)
    }

    fn start_out(&mut self, out: &mut Vec<u8>, path: &str) -> Started {
        let job = Job { path: path.to_string(), first: 10, line: "{}".into(), hidden: false, want_changed: false, session: None };
        start(out, &mut self.st, &self.pool, &mut self.watch, &mut self.gate, job, Arc::clone(&self.tb.mime))
    }

    fn the_scan_lands(&self) -> Done {
        loop {
            if let Event::List(done) = self.rx.recv_timeout(Duration::from_secs(10)).expect("the scan lands") {
                return done;
            }
        }
    }
}

// A sandbox mapped onto a hung nfs mount, so start takes its remote path for it.
fn remote_body(dir: &std::path::Path) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", dir.to_string_lossy())
}

// Issue 144: start must return before list_dir's read answers, so the loop keeps delivering transferprogress meanwhile.
#[test]
fn a_slow_scan_does_not_hold_the_loop_or_its_events() {
    let mut r = rig("listwork-slow-scan");
    r.dir.file("a.txt", "a");
    let path = r.dir.path().to_string_lossy().into_owned();
    let _body = crate::backend::iomount::test_hold_body(remote_body(r.dir.path()));
    let hold = scanhold::hold(&path, Stage::Read);
    assert!(matches!(r.start(&path), Started::Remote), "the held scan proves the remote path");
    assert!(!hold.over(), "start waited out the held scan, so list_dir ran on the loop");
    hold.entered.recv_timeout(Duration::from_secs(5)).expect("the scan reached the read");
    hold.release();
    let listed = match r.the_scan_lands().result {
        Ok(listed) => listed,
        Err(failed) => panic!("the held directory lists once released: {}", failed.error.msg),
    };
    assert_eq!(listed.listing.len(), 1, "and it is list_dir's own answer");
}

// Issue 144, the failure half: the mode stat can hang on the same stalled mount, so it runs on the worker and rides the answer.
#[test]
fn a_slow_mode_stat_after_a_failed_scan_does_not_hold_the_loop() {
    let mut r = rig("listwork-slow-mode");
    let file = r.dir.file("plain.txt", "a");
    let path = file.to_string_lossy().into_owned();
    let _body = crate::backend::iomount::test_hold_body(remote_body(r.dir.path()));
    let hold = scanhold::hold(&path, Stage::Mode);
    assert!(matches!(r.start(&path), Started::Remote), "the held mode stat proves the remote path");
    assert!(!hold.over(), "start waited out the held mode stat, so it ran on the loop");
    hold.entered.recv_timeout(Duration::from_secs(5)).expect("the mode stat started on the worker");
    hold.release();
    let failed = match r.the_scan_lands().result {
        Err(failed) => failed,
        Ok(_) => panic!("a file is not a listing"),
    };
    assert_eq!(failed.mode, std::fs::metadata(&file).expect("the file exists").mode(), "the mode the worker read rides the answer");
}

// The loop's side: the error line carries the mode the answer brought, so finish never stats a path for one.
#[test]
fn a_failed_scan_answers_the_mode_the_worker_read_not_a_fresh_stat() {
    let mut r = rig("listwork-failed-mode");
    let missing = r.dir.join("gone").to_string_lossy().into_owned();
    let landed = Landed {
        job: Job { path: missing.clone(), first: 10, line: "{}".into(), hidden: false, want_changed: false, session: None },
        done: Done {
            result: Err(ListErr {
                error: FleaError { where_: "scan".into(), path: missing, msg: "refused".into() },
                mode: 0o40750, watch_wd: -1,
            }),
        },
    };
    let mut out = Vec::new();
    assert!(!finish(&mut out, &mut r.st, &r.tb, &r.pool, &mut r.watch, landed), "a failed scan lands false");
    let text = String::from_utf8(out).expect("the error line is UTF-8");
    assert!(text.contains(r#""t":"error""#), "the failure is still an error line: {text}");
    // A fresh stat of the missing path would answer 0 and drop the field.
    assert!(text.contains(&format!(r#""mode":{}"#, 0o40750)), "the line carries the mode the worker read: {text}");
}

// A local list never reaches the scan thread and a remote one always does, with no timing in the verdict.
#[test]
fn a_local_list_runs_inline_and_a_remote_list_reaches_the_scan_thread() {
    let mut r = rig("listwork-route-local");
    r.dir.file("a.txt", "a");
    let path = r.dir.path().to_string_lossy().into_owned();
    let mut out = Vec::new();
    match r.start_out(&mut out, &path) {
        Started::Inline(landed) => {
            assert_eq!(landed.job.path, path, "the inline answer carries its own request");
            assert!(r.gate.scanner.is_none(), "no scan thread started for a local list");
            assert!(!r.gate.holds(), "the gate holds nothing after an inline list");
            assert!(r.rx.try_recv().is_err(), "no Event::List follows an inline list");
            assert!(finish(&mut out, &mut r.st, &r.tb, &r.pool, &mut r.watch, *landed), "the inline answer lands through finish");
            let text = String::from_utf8(out).expect("the listing is UTF-8");
            assert!(text.contains(r#""t":"listed""#) && text.contains(r#""t":"rows""#), "finish wrote the listing: {text}");
        }
        Started::Remote => panic!("a local list runs inline, never on the scan thread"),
    }
    let mut r = rig("listwork-route-remote");
    r.dir.file("a.txt", "a");
    let path = r.dir.path().to_string_lossy().into_owned();
    let _body = crate::backend::iomount::test_hold_body(remote_body(r.dir.path()));
    let mut out = Vec::new();
    match r.start_out(&mut out, &path) {
        Started::Remote => {
            assert!(r.gate.holds(), "the gate holds while the remote scan is out");
            assert!(r.gate.scanner.is_some(), "the remote scan took the kept thread");
            assert!(out.is_empty(), "nothing lands before the remote scan answers");
        }
        Started::Inline(_) => panic!("a remote list runs on the scan thread, never inline"),
    }
    let done = r.the_scan_lands();
    let job = r.gate.take().expect("the answer closes the job");
    assert!(finish(&mut Vec::new(), &mut r.st, &r.tb, &r.pool, &mut r.watch, Landed { job, done }), "the remote answer lands through finish");
}

// A scan thread that dies must still answer, or the gate holds forever and every later request waits.
#[test]
fn a_scan_that_panics_answers_an_error_and_leaves_the_gate_free() {
    let mut r = rig("listwork-dead-scan");
    r.gate.begin(Job { path: "/dead".into(), first: 0, line: "{}".into(), hidden: false, want_changed: false, session: None });
    quietpanic::quiet();
    r.gate.scan("/dead".into(), |_| -> Result<ListOut, ListErr> { panic!("{}", quietpanic::DELIBERATE) });
    let event = r.rx.recv_timeout(Duration::from_secs(2));
    let done = match event.expect("a dead scan still answers") {
        Event::List(done) => done,
        _ => panic!("the answer is a list event"),
    };
    let job = r.gate.take().expect("the answer closes the job");
    assert!(!r.gate.holds(), "the gate is free once the answer lands");
    let mut out = Vec::new();
    assert!(!finish(&mut out, &mut r.st, &r.tb, &r.pool, &mut r.watch, Landed { job, done }), "a dead scan lands false");
    let text = String::from_utf8(out).expect("the error line is UTF-8");
    let said = crate::backend::iomount::internal_failure("scan", Path::new("/dead")).msg;
    assert!(text.contains(&said), "the line is iomount's own dead-worker sentence: {text}");
    assert!(!text.contains(r#""mode""#), "no mode was read, so the line carries none: {text}");
}

// Requests that land while a scan is out wait for it and replay in arrival order, so list/list/paths sees both listings.
#[test]
fn requests_that_arrive_while_a_scan_is_out_replay_in_order() {
    let mut gate = Gate::new(crate::backend::events::channel().0);
    assert!(!gate.holds(), "nothing is out before the first list");
    gate.begin(Job { path: "/a".into(), first: 0, line: "{}".into(), hidden: false, want_changed: false, session: None });
    assert!(gate.holds());
    gate.defer("first".into());
    gate.defer("second".into());
    assert_eq!(gate.take().map(|job| job.path), Some("/a".to_string()));
    assert_eq!(gate.next().as_deref(), Some("first"), "the first request replays first");
    assert_eq!(gate.next().as_deref(), Some("second"), "and the second after it");
    assert_eq!(gate.next(), None);
}

// A change seen while a scan is out is answered after it, against the descriptor that became current.
#[test]
fn a_change_seen_during_a_scan_is_decided_once_the_scan_lands() {
    let mut gate = Gate::new(crate::backend::events::channel().0);
    gate.begin(Job { path: "/a".into(), first: 0, line: "{}".into(), hidden: false, want_changed: false, session: None });
    gate.note_change(7);
    gate.note_change(9);
    assert_eq!(gate.take_changes(), vec![7, 9], "every descriptor is kept for the decision");
    assert!(gate.take_changes().is_empty(), "and the next scan starts with none");
}

// An unmount seen while a scan is out is kept apart from the changes and replays in the order it arrived.
#[test]
fn an_unmount_seen_during_a_scan_waits_apart_from_the_changes_and_keeps_its_order() {
    let mut gate = Gate::new(crate::backend::events::channel().0);
    gate.begin(Job { path: "/a".into(), first: 0, line: "{}".into(), hidden: false, want_changed: false, session: None });
    gate.note_change(7);
    gate.note_unmount(9);
    gate.note_unmount(4);
    assert_eq!(gate.take_changes(), vec![7], "a change is not an unmount");
    assert_eq!(gate.take_unmounts(), vec![9, 4], "every unmount is kept, in arrival order");
    assert!(gate.take_unmounts().is_empty(), "and the next scan starts with none");
}

fn refused(path: &str) -> Result<ListOut, ListErr> {
    Err(ListErr { error: FleaError { where_: "scan".into(), path: path.into(), msg: "refused".into() }, mode: 0, watch_wd: -1 })
}

// A thread start on every list was most of a fixed 0.15 ms on minipc, so every scan runs on the one thread the gate keeps.
#[test]
fn every_scan_runs_on_the_same_thread() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (id_tx, id_rx) = channel();
    for _ in 0..2 {
        let id_tx = id_tx.clone();
        gate.scan("/a".into(), move |_| {
            id_tx.send(thread::current().id()).expect("the test is listening");
            refused("/a")
        });
        rx.recv_timeout(Duration::from_secs(5)).expect("the scan answers");
    }
    let (first, second) = (id_rx.recv().expect("a first scan ran"), id_rx.recv().expect("a second scan ran"));
    assert_ne!(first, thread::current().id(), "the scan is off the loop's thread");
    assert_eq!(first, second, "both scans were served by one thread");
}

// A scan that panics answers, and the thread that ran it is still there for the next listing.
#[test]
fn a_panicked_scan_leaves_the_thread_serving_the_next() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (id_tx, id_rx) = channel();
    quietpanic::quiet();
    let dead = id_tx.clone();
    gate.scan("/dead".into(), move |_| -> Result<ListOut, ListErr> {
        dead.send(thread::current().id()).expect("the test is listening");
        panic!("{}", quietpanic::DELIBERATE)
    });
    let answered = rx.recv_timeout(Duration::from_secs(5));
    assert!(matches!(answered, Ok(Event::List(_))), "the panicked scan still answers");
    gate.scan("/next".into(), move |_| {
        id_tx.send(thread::current().id()).expect("the test is listening");
        refused("/next")
    });
    rx.recv_timeout(Duration::from_secs(5)).expect("the next scan answers");
    assert_eq!(id_rx.recv().expect("the panicking scan ran"), id_rx.recv().expect("the next scan ran"), "the same thread served both");
}

// A scanner that is gone must not turn into silence: the scan runs where it was asked, and the next one starts a fresh thread.
#[test]
fn a_gone_scanner_still_answers_and_is_started_afresh() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (gone, queue) = channel::<Work>();
    drop(queue);
    gate.scanner = Some(gone);
    let (id_tx, id_rx) = channel();
    let first = id_tx.clone();
    gate.scan("/a".into(), move |_| {
        first.send(thread::current().id()).expect("the test is listening");
        refused("/a")
    });
    rx.recv_timeout(Duration::from_secs(5)).expect("the scan still answers");
    assert!(gate.scanner.is_none(), "the dead scanner is dropped");
    gate.scan("/b".into(), move |_| {
        id_tx.send(thread::current().id()).expect("the test is listening");
        refused("/b")
    });
    rx.recv_timeout(Duration::from_secs(5)).expect("the next scan answers");
    let (inline, fresh) = (id_rx.recv().expect("the first scan ran"), id_rx.recv().expect("the second scan ran"));
    assert_eq!(inline, thread::current().id(), "the first scan ran where it was asked");
    assert_ne!(fresh, thread::current().id(), "the next scan has a thread of its own again");
}
