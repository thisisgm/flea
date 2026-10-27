use super::*;
use crate::backend::testdir::TestDir;
use crate::clip::protocol::{DISPLAY, DISPLAY_GET_REGISTRY, DISPLAY_SYNC};
use crate::clip::wire::Conn;
use crate::json::field_str;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, ThreadId};
use std::time::Duration;

// Hang bounds for handshakes that must arrive, never durations the code under test is held to.
const TEST_WATCHDOG: Duration = Duration::from_secs(5);
const TEST_HOLD_WATCHDOG: Duration = Duration::from_secs(10);
const TEST_WAIT_MS: u32 = 5000;

type StartEvents = Arc<Mutex<Vec<(&'static str, ThreadId)>>>;

fn record_start_event(events: &StartEvents, event: &'static str) {
    events.lock().unwrap().push((event, std::thread::current().id()));
}

fn assert_serial_starts(events: &StartEvents) -> ThreadId {
    let events = events.lock().unwrap();
    let names: Vec<_> = events.iter().map(|event| event.0).collect();
    assert_eq!(names, ["first start", "first end", "second start", "second end"]);
    let worker = events.first().expect("a recorded start").1;
    assert!(events.iter().all(|event| event.1 == worker), "one queue must run every start on one thread: {:?}", *events);
    worker
}

fn clip_line(message: OpMsg) -> String {
    let OpMsg::Meta { line } = message else { panic!("a clip line") };
    line
}

// A silent compositor for the clear path: it signals the handshake, then holds the stream open until released.
fn fake_compositor(tag: &str) -> (TestDir, PathBuf, Receiver<()>, Sender<()>, JoinHandle<()>) {
    let dir = TestDir::new(tag);
    let sock = dir.join("fake-hung");
    let listener = UnixListener::bind(&sock).unwrap();
    let (accepted_tx, accepted) = channel();
    let (release_tx, release) = channel();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut conn = Conn::over(OwnedFd::from(stream));
        // Both handshake requests are read, so closing later ends the stream cleanly, never with a reset.
        for opcode in [DISPLAY_GET_REGISTRY, DISPLAY_SYNC] {
            let event = conn.next_raw(TEST_WAIT_MS).unwrap().expect("a handshake request");
            assert_eq!((event.sender, event.opcode), (DISPLAY, opcode));
        }
        accepted_tx.send(()).unwrap();
        // The accepted stream stays open and silent until the test lets go of it.
        release.recv_timeout(TEST_HOLD_WATCHDOG).unwrap();
    });
    (dir, sock, accepted, release_tx, worker)
}

#[test]
fn a_request_runs_beside_the_loop_that_made_it() {
    let (replies, result) = channel();
    let (entered, running) = channel();
    let (release, proceed) = channel();
    beside(replies, move || {
        entered.send(()).unwrap();
        proceed.recv_timeout(TEST_HOLD_WATCHDOG).unwrap();
        reply::reply_clear(Ok(false))
    });
    // The work is parked until released here, so a call that ran it inline would never reach this line.
    running.recv_timeout(TEST_WATCHDOG).unwrap();
    release.send(()).unwrap();
    assert_eq!(field_str(&clip_line(result.recv_timeout(TEST_WATCHDOG).unwrap()), "op").as_deref(), Some("clear"));
}

#[test]
fn a_silent_set_owner_never_holds_the_request_loop_and_still_refuses() {
    let queue = SetQueue::new();
    let dir = TestDir::new("clip-set-silent");
    let script = dir.script("owner", "#!/bin/sh\ncat >/dev/null\nprintf 'payload-read\\n'\nexec tail -f /dev/null\n");
    let (replies, result) = channel();
    let (entered, running) = channel();
    let (release, proceed) = channel();
    let (returned, free) = channel();
    let caller = std::thread::spawn(move || {
        queue.start(replies, move || own::spawn_owner_with(&script, "copy", &["/tmp/a".into()], |rx| {
            let read = rx.recv_timeout(TEST_WATCHDOG).unwrap();
            assert_eq!(read.1.trim_end(), "payload-read");
            entered.send(()).unwrap();
            proceed.recv_timeout(TEST_HOLD_WATCHDOG).unwrap();
            Err(RecvTimeoutError::Timeout)
        }));
        returned.send(()).unwrap();
    });
    running.recv_timeout(TEST_WATCHDOG).unwrap();
    let request_returned = free.recv_timeout(TEST_WATCHDOG);
    release.send(()).ok();
    caller.join().unwrap();
    assert!(request_returned.is_ok(), "request_set must return while the silent owner is still awaiting ready");
    let line = clip_line(result.recv_timeout(TEST_WATCHDOG).unwrap());
    assert!(line.contains(r#""ok":false"#));
    assert!(line.contains("the clipboard owner did not answer"));
}

#[test]
fn a_hung_compositor_gets_a_refusal_once_it_lets_go() {
    let (_dir, sock, accepted, release, compositor) = fake_compositor("clip-clear-hang");
    let queue = SetQueue::new();
    let (replies, result) = channel();
    queue.clear(replies, move || clear_line("", &["/tmp/a.txt".to_string()], Some(&sock)));
    accepted.recv_timeout(TEST_WATCHDOG).unwrap();
    release.send(()).unwrap();
    let line = clip_line(result.recv_timeout(TEST_WATCHDOG).unwrap());
    compositor.join().unwrap();
    assert!(line.contains(r#""ok":false"#), "{}", line);
    assert_eq!(field_str(&line, "error").as_deref(), Some("the compositor closed the connection"));
}

#[test]
fn a_watch_starts_once_and_the_second_is_silent() {
    use crate::backend::{dirsizeworker::Worker, listing::Listing, state::{Held, State}};
    // A socket nobody listens on, so the thread ends after one honest error line and the channel closes with it.
    let dir = TestDir::new("clip-watch-no-compositor");
    let missing = dir.join("no-compositor");
    let (tx, rx) = channel();
    let (events, _events) = channel();
    // Exhaustive construction pins that the backend retains no clipboard report state.
    let mut state = State {
        listing: Listing::new(),
        base: Default::default(),
        asked: Vec::new(),
        outstanding: 0,
        deferred: Default::default(),
        window_meta: Default::default(),
        dirsizes: Default::default(),
        dirsize_queue: Vec::new(),
        dirsize_worker: Worker::new(events),
        search: None,
        search_session: None,
        search_reported: std::time::Instant::now(),
        generation: 0,
        held: Held::List,
        clip_watching: false, filter: None,
    };
    request_watch(tx.clone(), &mut state.clip_watching, Some(missing.clone()));
    assert!(state.clip_watching);
    request_watch(tx, &mut state.clip_watching, Some(missing.clone()));
    let mut lines = 0;
    for m in rx.iter() {
        let line = clip_line(m);
        assert!(line.contains(r#""op":"changed""#));
        assert!(line.contains(&missing.display().to_string()), "the error names the socket the request passed: {}", line);
        lines += 1;
    }
    assert_eq!(lines, 1, "one watcher means one error line, never two");
}

#[test]
fn sets_start_and_answer_in_request_order() {
    let queue = SetQueue::new();
    let events = StartEvents::default();
    let first_events = events.clone();
    let second_events = events.clone();
    let (replies, result) = channel();
    let (entered, running) = channel();
    let (release, proceed) = channel();
    let (second_entered, second_running) = channel();
    let result = Arc::new(Mutex::new(result));
    let second_result = result.clone();
    queue.start(replies.clone(), move || {
        record_start_event(&first_events, "first start");
        entered.send(()).unwrap();
        proceed.recv_timeout(TEST_HOLD_WATCHDOG).unwrap();
        record_start_event(&first_events, "first end");
        Ok("first".into())
    });
    running.recv_timeout(TEST_WATCHDOG).unwrap();
    queue.start(replies, move || {
        record_start_event(&second_events, "second start");
        let first_reply = second_result.lock().unwrap().try_recv().map(clip_line);
        second_entered.send(first_reply).unwrap();
        record_start_event(&second_events, "second end");
        Ok("second".into())
    });
    release.send(()).unwrap();
    let first = second_running.recv_timeout(TEST_WATCHDOG).unwrap().expect("the first set already answered before the second start");
    assert_eq!(field_str(&first, "token").as_deref(), Some("first"));
    let second = clip_line(result.lock().unwrap().recv_timeout(TEST_WATCHDOG).unwrap());
    assert_eq!(field_str(&second, "token").as_deref(), Some("second"));
    assert_serial_starts(&events);
}

#[test]
fn a_clear_waits_for_the_held_set_to_answer() {
    let queue = SetQueue::new();
    let events = StartEvents::default();
    let set_events = events.clone();
    let clear_events = events.clone();
    let (set_replies, set_result) = channel();
    let set_result = Arc::new(Mutex::new(set_result));
    let (clear_replies, clear_result) = channel();
    let (entered, running) = channel();
    let (release, proceed) = channel();
    let (ended, finished) = channel();
    let (observed, first_reply) = channel();
    queue.start(set_replies, move || {
        record_start_event(&set_events, "first start");
        entered.send(()).unwrap();
        proceed.recv_timeout(TEST_HOLD_WATCHDOG).unwrap();
        record_start_event(&set_events, "first end");
        ended.send(()).unwrap();
        Ok("held".into())
    });
    running.recv_timeout(TEST_WATCHDOG).unwrap();
    queue.clear(clear_replies, move || {
        record_start_event(&clear_events, "second start");
        observed.send(set_result.lock().unwrap().try_recv().map(clip_line)).unwrap();
        let line = clear_line("", &[], None);
        record_start_event(&clear_events, "second end");
        line
    });
    release.send(()).unwrap();
    finished.recv_timeout(TEST_WATCHDOG).unwrap();
    let clear = clip_line(clear_result.recv_timeout(TEST_WATCHDOG).unwrap());
    assert_serial_starts(&events);
    let set = first_reply.recv_timeout(TEST_WATCHDOG).unwrap().expect("the set must answer before clear starts");
    assert_eq!(field_str(&set, "token").as_deref(), Some("held"));
    assert_eq!(field_str(&clear, "op").as_deref(), Some("clear"));
}

#[test]
fn a_silent_compositor_clear_reports_the_timeout() {
    let (_dir, sock, _accepted, release, compositor) = fake_compositor("clip-clear-timeout");
    let queue = SetQueue::new();
    let (replies, result) = channel();
    queue.clear(replies, move || clear_line("", &["/tmp/a.txt".into()], Some(&sock)));
    let line = clip_line(result.recv_timeout(TEST_HOLD_WATCHDOG).unwrap());
    release.send(()).unwrap();
    compositor.join().unwrap();
    assert!(line.contains(r#""ok":false"#), "{}", line);
    assert!(line.contains("the compositor timed out waiting for a message"), "{}", line);
}

#[test]
fn an_empty_clear_token_is_refused() {
    let queue = SetQueue::new();
    let (tx, rx) = channel();
    queue.clear(tx, || clear_line("", &[], None));
    let line = clip_line(rx.recv_timeout(TEST_WATCHDOG).expect("a clear reply"));
    assert!(line.contains(r#""ok":false"#));
    assert_eq!(field_str(&line, "op").as_deref(), Some("clear"));
}

#[test]
fn independent_set_queues_do_not_block_each_other() {
    let held_queue = SetQueue::new();
    let free_queue = SetQueue::new();
    let held_events = StartEvents::default();
    let held_first_events = held_events.clone();
    let held_second_events = held_events.clone();
    let free_events = StartEvents::default();
    let free_first_events = free_events.clone();
    let free_second_events = free_events.clone();
    let (held_reply, held_result) = channel();
    let (entered, running) = channel();
    let (release, proceed) = channel();
    held_queue.start(held_reply.clone(), move || {
        record_start_event(&held_first_events, "first start");
        entered.send(()).unwrap();
        proceed.recv_timeout(TEST_HOLD_WATCHDOG).unwrap();
        record_start_event(&held_first_events, "first end");
        Ok("held".into())
    });
    running.recv_timeout(TEST_WATCHDOG).unwrap();
    held_queue.start(held_reply, move || {
        record_start_event(&held_second_events, "second start");
        record_start_event(&held_second_events, "second end");
        Ok("held-second".into())
    });
    let (free_reply, free_result) = channel();
    free_queue.start(free_reply.clone(), move || {
        record_start_event(&free_first_events, "first start");
        record_start_event(&free_first_events, "first end");
        Ok("independent".into())
    });
    free_queue.start(free_reply, move || {
        record_start_event(&free_second_events, "second start");
        record_start_event(&free_second_events, "second end");
        Ok("independent-second".into())
    });
    let independent = free_result.recv_timeout(TEST_WATCHDOG);
    let independent_second = free_result.recv_timeout(TEST_WATCHDOG);
    release.send(()).unwrap();
    let line = clip_line(independent.expect("an independent queue must answer while another queue is held"));
    assert_eq!(field_str(&line, "token").as_deref(), Some("independent"));
    let line = clip_line(independent_second.expect("both independent starts must finish before the held queue is released"));
    assert_eq!(field_str(&line, "token").as_deref(), Some("independent-second"));
    held_result.recv_timeout(TEST_WATCHDOG).unwrap();
    held_result.recv_timeout(TEST_WATCHDOG).unwrap();
    assert_ne!(assert_serial_starts(&held_events), assert_serial_starts(&free_events));
}

#[test]
fn a_panicking_start_is_refused_and_the_next_set_still_answers() {
    let queue = SetQueue::new();
    let (replies, result) = channel();
    queue.start(replies.clone(), || panic!("test owner start failed"));
    queue.clear(replies.clone(), || panic!("test clear failed"));
    queue.start(replies, || Ok("second".into()));
    let first = clip_line(result.recv_timeout(TEST_WATCHDOG).expect("a panicking start must still answer"));
    assert!(first.contains(r#""ok":false"#));
    assert_eq!(field_str(&first, "error").as_deref(), Some("the clipboard owner start panicked"));
    let clear = clip_line(result.recv_timeout(TEST_WATCHDOG).expect("a panicking clear must still answer"));
    assert_eq!(field_str(&clear, "op").as_deref(), Some("clear"));
    assert!(clear.contains(r#""ok":false"#));
    assert_eq!(field_str(&clear, "error").as_deref(), Some("the clipboard clear start panicked"));
    let second = clip_line(result.recv_timeout(TEST_WATCHDOG).expect("the next set must still answer"));
    assert!(second.contains(r#""ok":true"#));
    assert_eq!(field_str(&second, "token").as_deref(), Some("second"));
}

#[test]
fn a_stopped_set_worker_keeps_its_refusal() {
    let (sets, worker) = channel();
    drop(worker);
    let queue = SetQueue { sets };
    let (replies, result) = channel();
    queue.start(replies.clone(), || panic!("a stopped worker must not start an owner"));
    queue.clear(replies, || panic!("a stopped worker must not clear"));
    let line = clip_line(result.recv_timeout(TEST_WATCHDOG).unwrap());
    assert!(line.contains(r#""ok":false"#));
    assert_eq!(field_str(&line, "error").as_deref(), Some("the clipboard set worker stopped"));
    let line = clip_line(result.recv_timeout(TEST_WATCHDOG).unwrap());
    assert_eq!(field_str(&line, "op").as_deref(), Some("clear"));
    assert!(line.contains(r#""ok":false"#));
    assert_eq!(field_str(&line, "error").as_deref(), Some("the clipboard clear worker stopped"));
}
