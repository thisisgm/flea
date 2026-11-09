// The loop polls stdin and its channel's doorbell together; threads forward every other blocking source to that channel.
#[cfg(test)]
pub use super::wake::channel;
pub use super::wake::Sender;
use crate::backend::opscancel::Live;
use crate::backend::opsreq::OpMsg;
use crate::backend::proto::{parse_request, Request, TRANSFER_CANCEL};
use crate::backend::thumbs::Done;
use crate::error::{from_io, FleaError};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::thread;

// Every source of work reaches the loop as one of these.
pub enum Event {
    Contents(super::contents::Done),
    Git(String),
    Request(String),
    Nearby { ticket: u64, line: String },
    Thumb(Done),
    DirSize(crate::backend::dirsizeworker::Done),
    // A slow mount's figures, decided off the loop so no statfs ever blocks a window behind it.
    FsInfo(crate::backend::fsinforeq::Done),
    // A write operation's own thread reports here, so the loop stays the only writer of stdout.
    Op(OpMsg),
    // Issue 144: a list's scan, so a slow mount's read_dir never holds the loop; see src/backend/listwork.rs.
    List(crate::backend::listwork::Done),
    // A flash worker's own thread reports here, on the same single receiver as every other event.
    Flash(super::flashslot::FlashMsg),
    // The watch descriptor that saw it, so a burst belonging to the directory the client has already
    // left is dropped rather than answered for the new one; see src/backend/watch.rs.
    Changed(i32),
    // The watch descriptor whose mount went away, so the pane leaves the volume instead of re-listing it.
    Unmounted(i32),
    // A network folder's mtime moved between polls, which inotify never delivers; the open path only.
    PollChanged(PathBuf),
    // A list worker that outlived its call hands back the watch it armed; the loop drops it unless live.
    AbandonWatch(i32),
    // A peek armed a watch on a column's directory; the loop holds it and names it when it fires.
    PeekArmed(i32, PathBuf),
    // The peek watch descriptor that saw a change, answered as a changed line for that column's directory.
    PeekChanged(i32),
    // The kernel dropped a peek watch itself, a deleted or unmounted directory, so the loop stops holding it.
    PeekGone(i32),
    // A session walk landed; the loop re-runs the standing query once when it is still the newest.
    SearchWalked,
    // The session token rejects events from a closed scope before directory deltas are applied.
    SearchDelta(usize),
    // A query held while a session walk runs reached the listing swap's cap; the loop answers it from the partial index.
    SearchHeld,
    SearchReady,
    SearchRefresh,
    ReadError(FleaError),
    Closed,
}

// Issue 144: whichever thread reads the cancel sets its operation's flag before forwarding the line to the loop.
pub(crate) fn read_line(line: String, live: &Live) -> Event {
    #[cfg(test)]
    super::renametiming::input(&line);
    if let Some(Request::TransferCancel { id }) = cancel_in(&line) {
        live.cancel(id);
    }
    if line.contains("jobs")
        && crate::json::field_str(&line, "c").as_deref() == Some("jobs")
        && crate::json::field_str(&line, "op").as_deref() == Some("cancel")
    {
        live.cancel(crate::json::field_usize(&line, "id").unwrap_or(0));
    }
    // A quit cancels here too, as Cancel does, so a listing still out cannot hold a copy past it.
    if line.contains("quit") && crate::json::field_str(&line, "c").as_deref() == Some("quit") {
        cancel_running(live);
    }
    Event::Request(line)
}

// The slot operation and every detached job (a copy is one), as the drain does once the loop gets there.
fn cancel_running(live: &Live) {
    if let Some(id) = live.running() {
        live.cancel(id);
    }
    live.1.cancel_all();
}

// Sample input: b"{\"c\":\"pi" then b"ng\"}\r\n" sends {"c":"ping"}; CR LF ends a line as LF does.
// False once input is over: the end sends the rest, then Closed, as BufRead::lines stops.
pub(crate) fn feed(
    tail: &mut Vec<u8>,
    bytes: &[u8],
    end: bool,
    tx: &Sender<Event>,
    live: &Live,
) -> bool {
    let mut rest = bytes;
    while let Some(at) = rest.iter().position(|&b| b == b'\n') {
        tail.extend_from_slice(&rest[..at]);
        rest = &rest[at + 1..];
        if tail.last() == Some(&b'\r') {
            tail.pop();
        }
        if !send_line(std::mem::take(tail), tx, live) {
            return false;
        }
    }
    tail.extend_from_slice(rest);
    if !end {
        return true;
    }
    if !tail.is_empty() && !send_line(std::mem::take(tail), tx, live) {
        return false;
    }
    // The end of input is a quit, so it cancels at the door too.
    cancel_running(live);
    let _ = tx.send(Event::Closed);
    false
}

fn send_line(bytes: Vec<u8>, tx: &Sender<Event>, live: &Live) -> bool {
    match String::from_utf8(bytes) {
        Ok(line) => tx.send(read_line(line, live)).is_ok(),
        // The framing cannot be trusted past a line that is not UTF-8, so input stops there and the loop reports it.
        Err(_) => {
            let _ = tx.send(Event::ReadError(from_io(
                "read",
                "stdin",
                &io::Error::from(io::ErrorKind::InvalidData),
            )));
            false
        }
    }
}

// Parsed only for the one request this thread acts on, so no other line pays for a second parse.
fn cancel_in(line: &str) -> Option<Request> {
    if !line.contains(TRANSFER_CANCEL) {
        return None;
    }
    Some(parse_request(line))
}

// An operation thread answers on its own channel, joined onto the loop's receiver the same way the pool's is.
pub fn spawn_op_forwarder(results: Receiver<OpMsg>, tx: Sender<Event>) {
    thread::spawn(move || {
        for msg in results {
            if tx.send(Event::Op(msg)).is_err() {
                return;
            }
        }
    });
}

// The pool answers on its own channel, so one thread joins the two onto the single receiver the loop waits on.
pub fn spawn_forwarder(results: Receiver<Done>, tx: Sender<Event>) {
    thread::spawn(move || {
        for done in results {
            if tx.send(Event::Thumb(done)).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
