use super::*;
use crate::backend::opscancel::Live;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn claimed(id: usize) -> (Live, Arc<AtomicBool>) {
    let live = Live::new();
    let flag = Arc::new(AtomicBool::new(false));
    live.claim(id, &flag);
    (live, flag)
}

// Issue 144: the flag is set on this thread, so a loop parked inside a read_dir on a FUSE mount is
// not what the cancel waits for. A reader that only forwarded the line would leave the flag clear.
#[test]
fn a_cancel_line_reaches_the_operation_before_the_loop_reads_it() {
    let (live, flag) = claimed(4);
    let line = "{\"c\":\"transfercancel\",\"id\":4}";
    let event = read_line(line.into(), &live);
    assert!(flag.load(Ordering::Relaxed), "the reader thread sets the flag itself");
    match event {
        Event::Request(l) => assert_eq!(l, line, "and the line still reaches the loop, unchanged"),
        _ => panic!("a request line is forwarded as a request"),
    }
}

// The whole input path, not just the one line, because acting on the cancel is what issue 144 changed here.
#[test]
fn the_input_path_cancels_on_the_reading_thread_and_forwards_every_line() {
    let (live, flag) = claimed(4);
    let (tx, rx) = channel();
    let lines = b"{\"c\":\"list\",\"path\":\"/tmp\"}\n{\"c\":\"transfercancel\",\"id\":4}\n";
    let mut tail = Vec::new();
    assert!(feed(&mut tail, lines, false, &tx, &live), "whole lines keep input open");
    assert!(flag.load(Ordering::Relaxed), "the reading thread acted on the cancel rather than only forwarding it");
    assert!(!feed(&mut tail, &[], true, &tx, &live), "the end of input closes it");
    // The sender outlives the input here, so the receiver is read to the end rather than waited on.
    drop(tx);
    let seen: Vec<Event> = rx.iter().collect();
    assert_eq!(seen.len(), 3, "both lines and the close reach the event loop");
    assert!(matches!(seen[2], Event::Closed), "and the input reports the stream ending");
}

#[test]
fn a_cancel_names_the_operation_it_stops_and_leaves_the_other_running() {
    let (live, flag) = claimed(4);
    read_line("{\"c\":\"transfercancel\",\"id\":5}".into(), &live);
    assert!(!flag.load(Ordering::Relaxed), "a cancel aimed at another operation reaches nothing");
    assert_eq!(live.running(), Some(4), "and the one that is running is still running");
}

// The token can arrive as data rather than as the command, which is the one thing the substring
// pre-filter in cancel_in can get wrong: it passes the line on, and the parse is what decides.
#[test]
fn a_path_that_carries_the_token_is_not_a_cancel() {
    let (live, flag) = claimed(4);
    read_line("{\"c\":\"list\",\"path\":\"/tmp/transfercancel\",\"id\":4}".into(), &live);
    assert!(!flag.load(Ordering::Relaxed), "the command decides, not the spelling anywhere on the line");
    assert_eq!(live.running(), Some(4));
}

// The first bad line stops input, leaving run.rs to report the failure without reading beyond it.
#[test]
fn a_line_that_does_not_decode_stops_input_where_it_failed() {
    let (live, flag) = claimed(4);
    let (tx, rx) = channel();
    let lines: &[u8] = b"{\"c\":\"list\"}\n\xff\n{\"c\":\"transfercancel\",\"id\":4}\n";
    assert!(!feed(&mut Vec::new(), lines, false, &tx, &live), "a bad line closes input");
    drop(tx);
    let seen: Vec<Event> = rx.iter().collect();
    assert_eq!(seen.len(), 2, "the good line and the failure, and nothing read past it");
    assert!(matches!(seen[1], Event::ReadError(_)), "the decode failure is what the loop is handed");
    assert!(!flag.load(Ordering::Relaxed), "a cancel past the failure is never acted on");
}

// BufRead::lines joins split reads, strips CR LF and preserves a final unterminated line's CR.
#[test]
fn input_framing_matches_the_line_reader_it_replaced() {
    let (live, _flag) = claimed(4);
    let (tx, rx) = channel();
    let mut tail = Vec::new();
    for chunk in [&b"{\"c\":\"pi"[..], b"ng\"}\r\n\n{\"c\":", b"\"quit\"}\r"] {
        assert!(feed(&mut tail, chunk, false, &tx, &live));
    }
    assert!(!feed(&mut tail, &[], true, &tx, &live));
    drop(tx);
    let seen: Vec<String> = rx.iter().map(|event| match event {
        Event::Request(line) => line,
        Event::Closed => "closed".into(),
        _ => panic!("only requests and the close are sent"),
    }).collect();
    assert_eq!(seen, ["{\"c\":\"ping\"}", "", "{\"c\":\"quit\"}\r", "closed"]);
}

#[test]
fn an_ordinary_request_cancels_nothing_on_its_way_through() {
    let (live, flag) = claimed(4);
    let event = read_line("{\"c\":\"list\",\"path\":\"/tmp\"}".into(), &live);
    assert!(!flag.load(Ordering::Relaxed), "only the one request this thread acts on is acted on");
    assert!(matches!(event, Event::Request(_)));
}
