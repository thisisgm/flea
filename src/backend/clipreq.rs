// Set, get and clear answer one clip line each; clipWatch answers nothing before its changed lines.
use crate::backend::opsreq::OpMsg;
use crate::clip::{self, control, own, reply, watch, x11};
use std::sync::mpsc::Sender;

static SETS: std::sync::OnceLock<SetQueue> = std::sync::OnceLock::new();

// Validated before any spawn: a bad op or path answers rather than owning.
pub fn request_set(replies: Sender<OpMsg>, op: String, paths: Vec<String>) {
    SETS.get_or_init(SetQueue::new).start(replies, move || {
        if clip::use_x11() { x11::set(&op, &paths) } else { own::spawn_owner(&op, &paths) }
    });
}

struct SetWork {
    replies: Sender<OpMsg>,
    start: Box<dyn FnOnce() -> String + Send>,
    op: &'static str,
}

// One queue keeps clipboard mutations and replies in request order without holding the backend loop.
struct SetQueue {
    sets: Sender<SetWork>,
}

impl SetQueue {
    fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<SetWork>();
        std::thread::spawn(move || {
            for work in rx {
                let line = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work.start)) {
                    Ok(line) => line,
                    Err(_) => {
                        let error = if work.op == "set" { "the clipboard owner start panicked" } else { "the clipboard clear start panicked" };
                        mutation_failure(work.op, error)
                    }
                };
                let _ = work.replies.send(OpMsg::Meta { line });
            }
        });
        Self { sets: tx }
    }

    fn start(&self, replies: Sender<OpMsg>, start: impl FnOnce() -> Result<String, String> + Send + 'static) {
        self.enqueue(replies, "set", move || match start() {
            Ok(token) => reply::reply_set(true, &token, ""),
            Err(e) => reply::reply_set(false, "", &e),
        });
    }

    fn clear(&self, replies: Sender<OpMsg>, clear: impl FnOnce() -> String + Send + 'static) {
        self.enqueue(replies, "clear", clear);
    }

    fn enqueue(&self, replies: Sender<OpMsg>, op: &'static str, start: impl FnOnce() -> String + Send + 'static) {
        if let Err(error) = self.sets.send(SetWork { replies, start: Box::new(start), op }) {
            let line = mutation_failure(op, &format!("the clipboard {} worker stopped", op));
            let _ = error.0.replies.send(OpMsg::Meta { line });
        }
    }
}

fn mutation_failure(op: &str, error: &str) -> String {
    match op {
        "clear" => reply::reply_clear(false, false, error),
        _ => reply::reply_set(false, "", error),
    }
}

// A read can block on a foreign source, so this answers beside the loop like jump does.
pub fn request_get(replies: Sender<OpMsg>) {
    beside(replies, || match if clip::use_x11() { x11::get() } else { control::get() } {
        Ok(got) => reply::reply_get(true, Some(&got), ""),
        Err(e) => reply::reply_get(false, None, &e),
    });
}

// Either clear form can block on a foreign owner, so it waits in the mutation queue off the loop.
pub fn request_clear(replies: Sender<OpMsg>, token: String, cut: Vec<String>) {
    SETS.get_or_init(SetQueue::new).clear(replies, move || clear_line(&token, &cut));
}

fn clear_line(token: &str, cut: &[String]) -> String {
    if clip::use_x11() {
        let result = if !token.is_empty() { x11::clear(token) } else { x11::clear_cut(cut) };
        return match result {
            Ok(cleared) => reply::reply_clear(true, cleared, ""),
            Err(e) => reply::reply_clear(false, false, &e),
        };
    }
    if !cut.is_empty() {
        match control::clear_cut(cut) {
            Ok(cleared) => reply::reply_clear(true, cleared, ""),
            Err(e) => reply::reply_clear(false, false, &e),
        }
    } else if token.is_empty() {
        reply::reply_clear(false, false, "the token names the copy to clear")
    } else {
        clear_token_line(token, control::clear)
    }
}

pub(crate) fn clear_token_line(token: &str, clear: impl FnOnce(&str) -> Result<bool, String>) -> String {
    if own::withdraw(token) {
        return reply::reply_clear(true, true, "");
    }
    match clear(token) {
        Ok(cleared) => reply::reply_clear(true, cleared, ""),
        Err(e) => reply::reply_clear(false, false, &e),
    }
}

// One thread per request: its line goes back through the ops channel the loop already drains.
fn beside(replies: Sender<OpMsg>, work: impl FnOnce() -> String + Send + 'static) {
    std::thread::spawn(move || {
        let _ = replies.send(OpMsg::Meta { line: work() });
    });
}

// Idempotent: the first starts the watcher's one thread, later ones answer nothing.
pub fn request_watch(replies: Sender<OpMsg>, watching: &mut bool) {
    if *watching {
        return;
    }
    *watching = true;
    if clip::use_x11() { x11::watch(replies) } else { watch::start(replies) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{field_str, field_usize};

    const TEST_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(5);

    #[test]
    fn mutation_wrappers_use_one_queue_and_refuse_bad_input() {
        // A captured queue pins both wrappers without starting an owner or sharing it with another test.
        let (sets, pending) = std::sync::mpsc::channel();
        assert!(SETS.set(SetQueue { sets }).is_ok(), "only this test may use the static mutation queue");
        for (op, path) in [("move", "/tmp/a"), ("copy", "relative/a")] {
            let (tx, _rx) = std::sync::mpsc::channel();
            request_set(tx, op.into(), vec![path.into()]);
            let work = pending.try_recv().expect("request_set must enqueue on the static mutation queue");
            assert_eq!(work.op, "set");
            let line = (work.start)();
            assert_eq!(field_str(&line, "t").as_deref(), Some("clip"));
            assert_eq!(field_str(&line, "op").as_deref(), Some("set"));
            assert!(line.contains(r#""ok":false"#));
            assert!(line.contains(if op == "move" { "the clipboard operation is copy or cut" } else { "absolute" }));
        }
        let (tx, _rx) = std::sync::mpsc::channel();
        request_clear(tx, String::new(), Vec::new());
        let work = pending.try_recv().expect("request_clear must enqueue on the same static mutation queue");
        assert_eq!(work.op, "clear");
        let line = (work.start)();
        assert_eq!(field_str(&line, "op").as_deref(), Some("clear"));
        assert!(line.contains(r#""ok":false"#));
    }

    #[test]
    fn an_empty_clear_token_is_refused() {
        let queue = SetQueue::new();
        let (tx, rx) = std::sync::mpsc::channel();
        queue.clear(tx, || clear_line("", &[]));
        let line = match rx.recv_timeout(TEST_WATCHDOG).expect("a clear reply") {
            OpMsg::Meta { line } => line,
            _ => panic!("a clear reply"),
        };
        assert!(line.contains(r#""ok":false"#));
        assert_eq!(field_str(&line, "op").as_deref(), Some("clear"));
    }

    #[test]
    fn a_foreign_token_uses_the_verified_clear_result() {
        const TOKEN: &str = "cb1acb1acb1acb1acb1acb1acb1acb1a";
        for (result, expected) in [
            (Ok(true), r#"{"t":"clip","op":"clear","ok":true,"cleared":true}"#),
            (Ok(false), r#"{"t":"clip","op":"clear","ok":true,"cleared":false}"#),
            (Err("foreign owner refused".to_string()), r#"{"t":"clip","op":"clear","ok":false,"error":"foreign owner refused"}"#),
        ] {
            let called = std::cell::Cell::new(false);
            let line = clear_token_line(TOKEN, |token| {
                assert_eq!(token, TOKEN);
                called.set(true);
                result
            });
            assert!(called.get(), "an unowned token must reach the verified selection clear");
            assert_eq!(line, expected);
        }
    }

    #[test]
    fn the_get_error_shape_carries_no_clip() {
        let line = reply::reply_get(false, None, "no clipboard protocol: neither data-control manager is offered");
        assert_eq!(field_str(&line, "t").as_deref(), Some("clip"));
        assert!(line.contains(r#""ok":false"#));
        let line = reply::reply_get(true, Some(&reply::Got {
            clip: "cut".into(), paths: vec!["/a".into()], token: "t".into(), skipped: 1,
        }), "");
        assert_eq!(field_str(&line, "clip").as_deref(), Some("cut"));
        assert_eq!(field_usize(&line, "skipped"), Some(1));
    }
}

#[cfg(test)]
#[path = "clipreq_tests.rs"]
mod threading_tests;
