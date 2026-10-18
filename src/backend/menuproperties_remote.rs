// One remote bound covers metadata, every folder walk and the final selected-identity checks.
use super::{response, Selected};
use crate::backend::opsreq::OpMsg;
use crate::backend::trashmanifest::Cancellation;
use crate::json::field_bool;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub(super) fn serve(line: &str, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation) -> bool {
    serve_bounded(line, items, replies, cancel, crate::backend::iomount::BULK_DEADLINE)
}

fn serve_bounded(line: &str, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation, deadline: Duration) -> bool {
    let bound = Instant::now() + deadline;
    let request = line.to_owned();
    let selected = items.to_vec();
    let stop = Cancellation::default().next();
    let worker_stop = stop.clone();
    let (tx, rx) = channel();
    std::thread::spawn(move || { super::serve_local(&request, &selected, &tx, &worker_stop); });
    let ending = loop {
        if cancel.check().is_err() {
            break Ok(r#""cancelled":true,"partial":true,"done":false"#.to_string());
        }
        let remaining = bound.saturating_duration_since(Instant::now());
        if remaining.is_zero() { break Err("The selected items are not responding.".into()); }
        match rx.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(OpMsg::Meta { line }) => {
                // Recheck after receiving too, so cancellation cannot be followed by a queued exact ending.
                if cancel.check().is_err() || Instant::now() >= bound { continue; }
                let terminal = !field_bool(&line, "ok") || field_bool(&line, "done") || field_bool(&line, "cancelled");
                if replies.send(OpMsg::Meta { line }).is_err() { stop.next(); return false; }
                if terminal { return true; }
            }
            Ok(_) => unreachable!("Properties emits only menu replies"),
            Err(RecvTimeoutError::Timeout) => (),
            Err(RecvTimeoutError::Disconnected) => break Err("The selected items stopped answering.".into()),
        }
    };
    stop.next();
    replies.send(OpMsg::Meta { line: response(line, ending) }).is_ok()
}
