// Every source of work the read loop waits on, and the threads that join them onto its one channel.
// std has no select, so each blocking source is a thread and the loop only ever waits on the receiver.
use crate::backend::opscancel::Live;
use crate::backend::opsreq::OpMsg;
use crate::backend::proto::{parse_request, Request, TRANSFER_CANCEL};
use crate::backend::thumbs::Done;
use crate::error::{from_io, FleaError};
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;

// std has no select, so every source of work reaches the loop as one of these.
pub enum Event {
    Git(String),
    Request(String),
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
    // A query held while a session walk runs reached the listing swap's cap; the loop answers it from the partial index.
    SearchHeld,
    ReadError(FleaError),
    Closed,
}

// stdin blocks, so reading it is a thread and the loop only ever waits on the channel.
// Issue 144: a cancel is acted on here rather than only forwarded, because the loop that would act on
// it can be inside a read_dir on a FUSE mount for as long as the device takes to answer.
pub fn spawn_reader(tx: Sender<Event>, live: Arc<Live>) {
    thread::spawn(move || read_lines(io::stdin().lock(), &tx, &live));
}

// The loop itself, over any reader, so what this thread does with a line is asserted without a stdin.
fn read_lines(source: impl BufRead, tx: &Sender<Event>, live: &Live) {
    for line in source.lines() {
        let event = match line {
            Ok(l) => read_line(l, live),
            // The reader has no writer, so the decode failure is handed back for the loop to report.
            Err(e) => Event::ReadError(from_io("read", "stdin", &e)),
        };
        let fatal = matches!(event, Event::ReadError(_));
        if tx.send(event).is_err() || fatal {
            return;
        }
    }
    let _ = tx.send(Event::Closed);
}

// One line's worth of that job, split out so a cancel and an ordinary request are asserted apart.
fn read_line(line: String, live: &Live) -> Event {
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
    Event::Request(line)
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
