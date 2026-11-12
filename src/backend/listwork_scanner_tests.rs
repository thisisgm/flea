// Kept scanner reuse, panic recovery and bounded retries on a held device.
use super::*;
use crate::backend::quietpanic;
use crate::error::FleaError;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

fn refused(path: &str) -> Result<ListOut, ListErr> {
    Err(ListErr {
        error: FleaError {
            where_: "scan".into(),
            path: path.into(),
            msg: "refused".into(),
        },
        mode: 0,
        watch_wd: -1,
    })
}

// A thread start on every list was most of a fixed 0.15 ms on minipc, so every scan runs on the one thread the gate keeps.
#[test]
fn every_scan_runs_on_the_same_thread() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (id_tx, id_rx) = channel();
    for _ in 0..2 {
        let id_tx = id_tx.clone();
        gate.scan("/a".into(), "/".into(), move |_| {
            id_tx
                .send(thread::current().id())
                .expect("the test is listening");
            refused("/a")
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("the scan answers");
    }
    let (first, second) = (
        id_rx.recv().expect("a first scan ran"),
        id_rx.recv().expect("a second scan ran"),
    );
    assert_ne!(
        first,
        thread::current().id(),
        "the scan is off the loop's thread"
    );
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
    gate.scan(
        "/dead".into(),
        "/".into(),
        move |_| -> Result<ListOut, ListErr> {
            dead.send(thread::current().id())
                .expect("the test is listening");
            panic!("{}", quietpanic::DELIBERATE)
        },
    );
    let answered = rx.recv_timeout(Duration::from_secs(5));
    assert!(
        matches!(answered, Ok(Event::List(_))),
        "the panicked scan still answers"
    );
    gate.scan("/next".into(), "/".into(), move |_| {
        id_tx
            .send(thread::current().id())
            .expect("the test is listening");
        refused("/next")
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("the next scan answers");
    assert_eq!(
        id_rx.recv().expect("the panicking scan ran"),
        id_rx.recv().expect("the next scan ran"),
        "the same thread served both"
    );
}

// Repeated Reload reads replace the queued scan without starting an unbounded set of device readers.
#[test]
fn mtp_reload_has_two_readers_and_only_the_latest_queued_scan() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (started, reads) = channel();
    let mut releases = Vec::new();
    for n in 0..100 {
        gate.begin(Job {
            path: "/phone".into(),
            first: 0,
            line: "{}".into(),
            hidden: false,
            want_changed: false,
            session: None,
        });
        let started = started.clone();
        let (release, held) = channel();
        gate.scan("/phone".into(), "/phone".into(), move |_| {
            started.send(n).unwrap();
            if n < 2 {
                held.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            refused("/phone")
        });
        if n < 2 {
            releases.push(release);
            assert_eq!(reads.recv_timeout(Duration::from_secs(5)).unwrap(), n);
        }
        if n < 99 {
            let requested =
                list_path(r#"{"c":"list","path":"/phone","wantChanged":true}"#).unwrap();
            assert!(gate.let_go(&requested));
        }
    }
    assert_eq!(gate.scanner.as_ref().unwrap().workers(), 2);
    assert!(
        reads.try_recv().is_err(),
        "both workers remain in the held device read"
    );
    releases
        .into_iter()
        .for_each(|release| release.send(()).unwrap());
    assert_eq!(
        reads.recv_timeout(Duration::from_secs(5)).unwrap(),
        99,
        "only the latest queued retry reads"
    );
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Event::List(_))
    ));
    assert!(
        rx.recv_timeout(Duration::from_millis(100)).is_err(),
        "neither late first answer lands"
    );
}

fn job(path: &str) -> Job {
    Job {
        path: path.into(),
        first: 0,
        line: "{}".into(),
        hidden: false,
        want_changed: false,
        session: None,
    }
}

// Holds two let-go reads on /phone, the second let go for /nas/share, and returns their releases.
fn hold_phone_twice(
    gate: &mut Gate,
    started: &Sender<&'static str>,
    reads: &Receiver<&'static str>,
) -> Vec<Sender<()>> {
    let mut releases = Vec::new();
    for n in 0..2 {
        gate.begin(job("/phone"));
        let started = started.clone();
        let (release, held) = channel::<()>();
        releases.push(release);
        gate.scan("/phone".into(), "/phone".into(), move |_| {
            started.send("/phone").unwrap();
            let _ = held.recv_timeout(Duration::from_secs(10));
            refused("/phone")
        });
        assert_eq!(
            reads.recv_timeout(Duration::from_secs(5)).unwrap(),
            "/phone"
        );
        let requested = if n == 0 {
            list_path(r#"{"c":"list","path":"/phone","wantChanged":true}"#)
        } else {
            list_path(r#"{"c":"list","path":"/nas/share"}"#)
        };
        assert!(gate.let_go(&requested.unwrap()));
    }
    releases
}

fn scan_nas(gate: &mut Gate, started: &Sender<&'static str>) {
    gate.begin(job("/nas/share"));
    let started = started.clone();
    gate.scan("/nas/share".into(), "/nas".into(), move |_| {
        started.send("/nas/share").unwrap();
        refused("/nas/share")
    });
}

// Two reads held on one device leave a reader free for a list on another device.
#[test]
fn mtp_reload_held_device_does_not_starve_another_device() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (started, reads) = channel();
    let releases = hold_phone_twice(&mut gate, &started, &reads);
    scan_nas(&mut gate, &started);
    assert_eq!(
        reads.recv_timeout(Duration::from_secs(5)).unwrap(),
        "/nas/share",
        "the other device reads while both phone readers are held"
    );
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Event::List(_))
    ));
    assert_eq!(gate.scanner.as_ref().unwrap().workers(), 3);
    releases
        .into_iter()
        .for_each(|release| release.send(()).unwrap());
}

// The reader another device left idle never becomes a third reader on the held device.
#[test]
fn mtp_reload_idle_reader_keeps_the_held_device_cap() {
    let (tx, rx) = crate::backend::events::channel::<Event>();
    let mut gate = Gate::new(tx);
    let (started, reads) = channel();
    let releases = hold_phone_twice(&mut gate, &started, &reads);
    scan_nas(&mut gate, &started);
    assert_eq!(
        reads.recv_timeout(Duration::from_secs(5)).unwrap(),
        "/nas/share"
    );
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Event::List(_))
    ));
    gate.begin(job("/phone"));
    let third = started.clone();
    gate.scan("/phone".into(), "/phone".into(), move |_| {
        third.send("/phone").unwrap();
        refused("/phone")
    });
    assert!(
        reads.recv_timeout(Duration::from_millis(300)).is_err(),
        "a third phone read waits while both phone readers are held"
    );
    let mut releases = releases.into_iter();
    releases.next().unwrap().send(()).unwrap();
    assert_eq!(
        reads.recv_timeout(Duration::from_secs(5)).unwrap(),
        "/phone",
        "the queued phone read starts once a phone reader frees"
    );
    scan_nas(&mut gate, &started);
    assert_eq!(
        reads.recv_timeout(Duration::from_secs(5)).unwrap(),
        "/nas/share",
        "an idle reader still serves another device"
    );
    assert_eq!(gate.scanner.as_ref().unwrap().workers(), 3);
    releases.for_each(|release| release.send(()).unwrap());
}
