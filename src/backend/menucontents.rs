// Properties on a folder counts its contents with the folder size walker, on the menu worker and under the request's own cancel.
use super::dirsize::{walk_core, walk_counted, Progress, Tally};
use super::menu_actions::{response, Selected};
use super::opsreq::{OpMsg, PROGRESS_EVERY};
use super::trashmanifest::Cancellation;
use crate::json::field_usize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Sample input: {"c":"menuaction","op":"contents","id":4} answers a progress line every PROGRESS_EVERY, then one final line.
// False once the receiver is gone, so the worker ends with it.
pub(super) fn serve(line: &str, id: usize, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation) -> bool {
    serve_with_deadline(line, id, items, replies, cancel, super::iomount::BULK_DEADLINE)
}

// The bulk deadline bounds a folder with a remote mount at or below it; any other walks inline exactly as before.
fn serve_with_deadline(line: &str, id: usize, items: &[Selected], replies: &Sender<OpMsg>, cancel: &Cancellation, deadline: Duration) -> bool {
    if id == 0 || field_usize(line, "id") != Some(id) || items.len() != 1 {
        return send(replies, line, Err(super::menu_actions::SELECTION_CHANGED.into()));
    }
    count(&items[0], cancel, deadline, &mut |result| {
        send(replies, line, result.map(|counted| {
            let fields = fields(&counted.tally, counted.done);
            if counted.cancelled { format!(r#"{},"cancelled":true"#, fields) } else { fields }
        }))
    })
}

// The same walk feeds one folder's contents or a multi-item Properties total.
pub(super) struct Counted {
    pub tally: Tally,
    pub done: bool,
    pub cancelled: bool,
}
type Report<'a> = &'a mut dyn FnMut(Result<Counted, String>) -> bool;

pub(super) fn count(item: &Selected, cancel: &Cancellation, deadline: Duration, report: Report<'_>) -> bool {
    let body = super::iomount::mount_body();
    let mounts = super::walkmounts::remote_mounts_under(&item.path, &body);
    if mounts.is_empty() {
        count_local(item, cancel, report)
    } else if super::walkmounts::stuck_among(&mounts).is_some() {
        report(Ok(Counted { tally: Tally { bytes: 0, items: 0, partial: true }, done: true, cancelled: false }))
    } else {
        serve_remote(item, cancel, deadline, &body, report)
    }
}

fn send(replies: &Sender<OpMsg>, line: &str, fields: Result<String, String>) -> bool {
    replies.send(OpMsg::Meta { line: response(line, fields) }).is_ok()
}

fn count_local(item: &Selected, cancel: &Cancellation, report: Report<'_>) -> bool {
    match item.current() {
        Ok(meta) if meta.is_dir() => (),
        Ok(_) => return report(Err("Only a folder has contents to count.".into())),
        Err(error) => return report(Err(error)),
    }
    let open = std::cell::Cell::new(true);
    // A progress line nobody takes ends the walk at once instead of counting for nobody.
    let stopped = || {
        if let Some(pace) = test_pace() { std::thread::sleep(pace); }
        cancel.check().is_err() || !open.get()
    };
    let mut progress_report = |tally: &Tally| {
        if !report(Ok(Counted { tally: *tally, done: false, cancelled: false })) { open.set(false); }
    };
    let progress = Progress { every: PROGRESS_EVERY, report: &mut progress_report, entering: None };
    let tally = walk_counted(&item.path, &stopped, Some(progress));
    // A cancelled walk is a floor and never says done; close has already told the card to leave.
    let cancelled = stopped();
    open.get() && report(Ok(Counted { tally, done: !cancelled, cancelled }))
}

// What the remote worker hands back: the walk's own tally, or the refusal before any walk started.
struct RemoteDone {
    tally: Tally,
    cancelled: bool,
}

// The worker's lines to the menu thread: throttled progress, then the one ending.
enum RemoteMsg {
    Report(Tally),
    Done(Result<RemoteDone, String>),
}

// The floor so far, the folder the walk is in and the stop all shared; the worker never touches the replies channel.
struct RemoteShared {
    stop: AtomicBool,
    floor: Mutex<Tally>,
    at: Mutex<PathBuf>,
}

// A remote walk runs beside the menu worker under the bulk deadline; past it the menu answers the floor and a watcher decides any mark.
fn serve_remote(item: &Selected, cancel: &Cancellation, deadline: Duration, body: &str, report: Report<'_>) -> bool {
    let at = Mutex::new(item.path.clone());
    let shared = Arc::new(RemoteShared { stop: AtomicBool::new(false), floor: Mutex::new(Tally { bytes: 0, items: 0, partial: true }), at });
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker_item = item.clone();
    let worker_cancel = cancel.clone();
    let worker_shared = Arc::clone(&shared);
    let pace = test_pace();
    let mount_points: HashSet<PathBuf> = super::mountinfo::mounts_in(body).into_iter().map(|(mount, _)| mount).filter(|mount| mount.starts_with(&item.path)).collect();
    std::thread::spawn(move || remote_walk(&worker_item, &worker_cancel, &worker_shared, pace, &mount_points, &done_tx));
    remote_wait(cancel, &shared, done_rx, deadline, body, report)
}

// The single syscall that decides the folder happens on the worker too, so a hung mount never holds the menu worker.
fn remote_walk(item: &Selected, cancel: &Cancellation, shared: &RemoteShared, pace: Option<Duration>, mount_points: &HashSet<PathBuf>, done: &Sender<RemoteMsg>) {
    let outcome = match item.current() {
        Ok(meta) if meta.is_dir() => {
            shared.floor.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).bytes = meta.len();
            let stop = || {
                if let Some(pace) = pace { std::thread::sleep(pace); }
                cancel.check().is_err() || shared.stop.load(Ordering::Relaxed)
            };
            let mut forwarded = Instant::now();
            let mut report = |tally: &Tally| {
                *shared.floor.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = *tally;
                if forwarded.elapsed() >= PROGRESS_EVERY {
                    forwarded = Instant::now();
                    let _ = done.send(RemoteMsg::Report(*tally));
                }
            };
            let entering = |folder: &Path| {
                *shared.at.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = folder.to_path_buf();
                if let Some(pace) = pace { std::thread::sleep(pace); }
            };
            let dev_of = |entry: &Path| mount_points.contains(entry).then_some(u64::MAX);
            let tally = walk_core(&item.path, &stop, Some(&dev_of), Some(Progress { every: Duration::ZERO, report: &mut report, entering: Some(&entering) }));
            Ok(RemoteDone { tally, cancelled: cancel.check().is_err() || shared.stop.load(Ordering::Relaxed) })
        }
        Ok(_) => Err("Only a folder has contents to count.".into()),
        Err(error) => Err(error),
    };
    let _ = done.send(RemoteMsg::Done(outcome));
}

// Every wire line leaves from the menu worker, so a progress line never lands past the final one.
fn remote_wait(cancel: &Cancellation, shared: &Arc<RemoteShared>, done: Receiver<RemoteMsg>, deadline: Duration, body: &str, report: Report<'_>) -> bool {
    let bound = Instant::now() + deadline;
    let mut progress_due = Instant::now();
    loop {
        match done.recv_timeout(Duration::from_millis(20)) {
            Ok(RemoteMsg::Report(tally)) => {
                if Instant::now() >= progress_due {
                    progress_due = Instant::now() + PROGRESS_EVERY;
                    if !report(Ok(Counted { tally, done: false, cancelled: false })) {
                        shared.stop.store(true, Ordering::Relaxed);
                        return false;
                    }
                }
            }
            Ok(RemoteMsg::Done(outcome)) => return report(outcome.map(|done| Counted { tally: done.tally, done: !done.cancelled, cancelled: done.cancelled })),
            Err(RecvTimeoutError::Timeout) => {
                // A walk blocked in a syscall never sees the cancel, so the menu thread answers it here.
                if cancel.check().is_err() {
                    shared.stop.store(true, Ordering::Relaxed);
                    return report(Ok(Counted { tally: timed_out_floor(shared), done: false, cancelled: true }));
                }
                if Instant::now() >= bound {
                    shared.stop.store(true, Ordering::Relaxed);
                    let (watched, body, silence) = (Arc::clone(shared), body.to_string(), stall_silence());
                    std::thread::spawn(move || watch_stall(&watched, &done, silence, &body));
                    return report(Ok(Counted { tally: timed_out_floor(shared), done: true, cancelled: false }));
                }
            }
            // A dead worker leaves the floor already reported, marked partial rather than exact.
            Err(RecvTimeoutError::Disconnected) => {
                return report(Ok(Counted { tally: timed_out_floor(shared), done: true, cancelled: false }));
            }
        }
    }
}

// Past the deadline a walk silent for the single call bound is hung, so its folder's mount is marked; a walk that ends or counts again marks none.
fn watch_stall(shared: &RemoteShared, done: &Receiver<RemoteMsg>, silence: Duration, body: &str) {
    let (mut counted, mut moved) = (counted_of(shared), Instant::now());
    loop {
        match done.recv_timeout(Duration::from_millis(20)) {
            Ok(RemoteMsg::Done(_)) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(RemoteMsg::Report(_)) | Err(RecvTimeoutError::Timeout) => {}
        }
        let now = counted_of(shared);
        if now != counted {
            (counted, moved) = (now, Instant::now());
        }
        if moved.elapsed() >= silence {
            let at = shared.at.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            super::walkmounts::mark_holding(&at, body);
            return;
        }
    }
}

// What the walk has counted: a pair that stops changing is a walk that stopped.
fn counted_of(shared: &RemoteShared) -> (u64, u64) {
    let floor = shared.floor.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    (floor.items, floor.bytes)
}

fn timed_out_floor(shared: &RemoteShared) -> Tally {
    let mut floor = *shared.floor.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    floor.partial = true;
    floor
}

fn fields(tally: &Tally, done: bool) -> String {
    format!(r#""bytes":{},"items":{},"partial":{},"done":{}"#, tally.bytes, tally.items, tally.partial, done)
}

#[path = "menucontents_seams.rs"]
mod seams;
use seams::{stall_silence, test_pace};
#[cfg(test)]
pub(crate) use seams::{test_hold_pace, test_hold_silence};
#[cfg(test)]
#[path = "menucontents_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "menucontents_mounts_tests.rs"]
mod mounts_tests;
