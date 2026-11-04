use super::*;
use crate::backend::opscancel::Live;
use crate::backend::wake::EFD_CLOEXEC;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

// Long enough for a loaded test host; a pass never waits for it.
const DEADLINE: Duration = Duration::from_secs(10);
// Requests per cycle test: a stray takeover in one of them under host load stays inside the slack below.
const CYCLES: usize = 200;

extern "C" {
    fn pipe2(fds: *mut i32, flags: i32) -> i32;
}

// The inbox over a pipe in place of fd 0, with the test thread as the loop.
fn rig() -> (Inbox, Receiver<Event>, std::fs::File, OwnedFd, Arc<Live>) {
    let mut fds = [0i32; 2];
    assert_eq!(unsafe { pipe2(fds.as_mut_ptr(), EFD_CLOEXEC) }, 0);
    let (input, writer) = unsafe {
        (
            OwnedFd::from_raw_fd(fds[0]),
            std::fs::File::from(OwnedFd::from_raw_fd(fds[1])),
        )
    };
    let (tx, rx, pending) = open().expect("the doorbell and the timer open");
    let live = Arc::new(Live::new());
    let inbox = pending.start(input.as_raw_fd(), tx, Arc::clone(&live));
    (inbox, rx, writer, input, live)
}

fn request(event: Option<Event>) -> String {
    match event {
        Some(Event::Request(line)) => line,
        _ => panic!("a request line reaches the loop as a request"),
    }
}

fn wait_for(flag: &AtomicBool) {
    let at = Instant::now();
    while !flag.load(Ordering::Relaxed) {
        assert!(at.elapsed() < DEADLINE, "the cancel reached its operation");
        std::thread::sleep(Duration::from_millis(1));
    }
}

// The request path: the loop reads each line itself, so the helper never wakes and never takes stdin over.
#[test]
fn a_request_wakes_only_the_loop_that_answers_it() {
    let (mut inbox, rx, mut writer, _input, _live) = rig();
    for i in 0..CYCLES {
        writeln!(writer, "{{\"c\":\"ping\",\"i\":{i}}}").unwrap();
        assert_eq!(
            request(inbox.next(&rx)),
            format!("{{\"c\":\"ping\",\"i\":{i}}}")
        );
    }
    assert!(
        inbox.takeovers() * 10 <= CYCLES,
        "the helper took stdin over {} times in {CYCLES} answered requests",
        inbox.takeovers()
    );
}

// A send from another thread rings the doorbell while the loop sleeps, or lands before its last look; never lost either way.
#[test]
fn a_send_from_another_thread_always_wakes_the_sleeping_loop() {
    let (mut inbox, rx, _writer, _input, _live) = rig();
    let tx = inbox.shared.tx.clone();
    let sender = std::thread::spawn(move || {
        for _ in 0..1000 {
            tx.send(Event::SearchReady).unwrap();
            std::thread::yield_now();
        }
    });
    for _ in 0..1000 {
        assert!(matches!(inbox.next(&rx), Some(Event::SearchReady)));
    }
    sender.join().unwrap();
}

// Issue 144: a held loop still cancels through the helper, preserves input order and takes stdin back when it sleeps.
#[test]
fn a_cancel_reaches_its_operation_while_the_loop_is_held() {
    let (mut inbox, rx, mut writer, _input, live) = rig();
    let flag = Arc::new(AtomicBool::new(false));
    live.claim(4, &flag);
    writeln!(writer, "{{\"c\":\"list\",\"path\":\"/tmp\"}}").unwrap();
    writeln!(writer, "{{\"c\":\"transfercancel\",\"id\":4}}").unwrap();
    wait_for(&flag);
    assert_eq!(
        inbox.takeovers(),
        1,
        "the parked helper took stdin over once"
    );
    assert_eq!(
        request(inbox.next(&rx)),
        "{\"c\":\"list\",\"path\":\"/tmp\"}"
    );
    assert_eq!(
        request(inbox.next(&rx)),
        "{\"c\":\"transfercancel\",\"id\":4}"
    );
    for i in 0..CYCLES {
        writeln!(writer, "{{\"c\":\"ping\",\"i\":{i}}}").unwrap();
        assert_eq!(
            request(inbox.next(&rx)),
            format!("{{\"c\":\"ping\",\"i\":{i}}}")
        );
    }
    assert!(
        (inbox.takeovers() - 1) * 10 <= CYCLES,
        "the helper parked once the loop slept: {} takeovers",
        inbox.takeovers()
    );
}

// Search ticks never sleep, so the helper reads stdin for them, as the reader thread did.
#[test]
fn a_ticking_loop_hands_stdin_to_the_helper() {
    let (mut inbox, rx, mut writer, _input, live) = rig();
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    let flag = Arc::new(AtomicBool::new(false));
    live.claim(7, &flag);
    inbox.shared.disarm();
    inbox.hand_over();
    writeln!(writer, "{{\"c\":\"transfercancel\",\"id\":7}}").unwrap();
    wait_for(&flag);
    assert_eq!(
        request(rx.recv_timeout(DEADLINE).ok()),
        "{\"c\":\"transfercancel\",\"id\":7}"
    );
}

// The end of input is the close the reader thread sent, after the last whole line.
#[test]
fn the_end_of_input_closes_after_the_last_line() {
    let (mut inbox, rx, mut writer, _input, _live) = rig();
    write!(writer, "{{\"c\":\"ping\"}}\n{{\"c\":\"quit\"}}").unwrap();
    drop(writer);
    assert_eq!(request(inbox.next(&rx)), "{\"c\":\"ping\"}");
    assert_eq!(request(inbox.next(&rx)), "{\"c\":\"quit\"}");
    assert!(matches!(inbox.next(&rx), Some(Event::Closed)));
}
