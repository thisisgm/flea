// Every source of work the read loop waits on, and the threads that join them onto its one channel.
// std has no select, so each blocking source is a thread and the loop only ever waits on the receiver.
use crate::backend::opsreq::OpMsg;
use crate::backend::thumbs::Done;
use crate::error::{from_io, FleaError};
use std::io::{self, BufRead};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

// std has no select, so every source of work reaches the loop as one of these.
pub enum Event {
    Request(String),
    Thumb(Done),
    // A write operation's own thread reports here, so the loop stays the only writer of stdout.
    Op(OpMsg),
    // The watch descriptor that saw it, so a burst belonging to the directory the client has already
    // left is dropped, plus whether the burst affected a non-hidden name; see watch.rs.
    Changed(super::watch::Change),
    ReadError(FleaError),
    Closed,
}

// stdin blocks, so reading it is a thread and the loop only ever waits on the channel.
pub fn spawn_reader(tx: Sender<Event>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let event = match line {
                Ok(l) => Event::Request(l),
                // The reader has no writer, so the decode failure is handed back for the loop to report.
                Err(e) => Event::ReadError(from_io("read", "stdin", &e)),
            };
            let fatal = matches!(event, Event::ReadError(_));
            if tx.send(event).is_err() || fatal {
                return;
            }
        }
        let _ = tx.send(Event::Closed);
    });
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
