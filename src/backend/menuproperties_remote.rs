// One remote bound covers metadata, every folder walk and the final selected-identity checks.
use super::{response, Selected, Summary};
use crate::backend::dirsize::{walk_core, Progress, Tally};
use crate::backend::iomount;
use crate::backend::menucontents::Counted;
use crate::backend::opsreq::OpMsg;
use crate::backend::trashmanifest::Cancellation;
use crate::backend::walkmounts;
use crate::json::field_bool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CANCEL_POLL: Duration = Duration::from_millis(20);

struct Status {
    floor: Summary,
    at: PathBuf,
    moved: Instant,
    finished: bool,
}

struct Finished(Arc<State>);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.status.lock().unwrap().finished = true;
    }
}

pub(super) struct State {
    status: Mutex<Status>,
    body: String,
    #[cfg(test)]
    panic_on_progress: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl State {
    pub(super) fn before(&self, path: &Path) {
        let mut state = self.status.lock().unwrap();
        state.at = path.to_path_buf();
        state.moved = Instant::now();
    }

    pub(super) fn seed(&self, bytes: u64) {
        let mut state = self.status.lock().unwrap();
        state.floor.bytes = bytes;
        state.moved = Instant::now();
    }

    pub(super) fn metadata(&self, summary: &Summary) {
        self.status.lock().unwrap().floor = summary.clone();
    }

    pub(super) fn progress(&self, summary: &Summary) {
        let mut state = self.status.lock().unwrap();
        state.floor.bytes = summary.bytes;
        state.floor.partial = summary.partial;
        state.moved = Instant::now();
    }

    fn floor(&self, cancelled: bool) -> String {
        let mut floor = self.status.lock().unwrap().floor.clone();
        floor.partial = true;
        floor.fields(!cancelled, cancelled)
    }
}

pub(super) fn serve(
    line: &str,
    items: &[Selected],
    replies: &Sender<OpMsg>,
    cancel: &Cancellation,
    body: &str,
) -> bool {
    let mounts = items
        .iter()
        .flat_map(|item| walkmounts::remote_mounts_under(&item.path, body))
        .collect::<Vec<_>>();
    if walkmounts::stuck_among(&mounts).is_some() {
        let floor = Summary {
            partial: true,
            ..Summary::default()
        };
        return replies
            .send(OpMsg::Meta {
                line: response(line, Ok(floor.fields(true, cancel.check().is_err()))),
            })
            .is_ok();
    }
    serve_bounded(line, items, replies, cancel, body, iomount::BULK_DEADLINE)
}

fn serve_bounded(
    line: &str,
    items: &[Selected],
    replies: &Sender<OpMsg>,
    cancel: &Cancellation,
    body: &str,
    deadline: Duration,
) -> bool {
    let bound = Instant::now() + deadline;
    let shared = Arc::new(State {
        status: Mutex::new(Status {
            floor: Summary::default(),
            at: items[0].path.clone(),
            moved: Instant::now(),
            finished: false,
        }),
        body: body.into(),
        #[cfg(test)]
        panic_on_progress: tests::PANIC_ON_PROGRESS.with(|slot| slot.borrow_mut().take()),
    });
    let (request, selected, work) = (line.to_owned(), items.to_vec(), Arc::clone(&shared));
    let stop = Cancellation::default().next();
    let worker_stop = stop.clone();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _finished = Finished(Arc::clone(&work));
        super::serve_local(&request, &selected, &tx, &worker_stop, Some(&work));
    });
    let ending = loop {
        if cancel.check().is_err() {
            break Ok(shared.floor(true));
        }
        let remaining = bound.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            if !mark_if_stalled(&shared) {
                let watching = Arc::clone(&shared);
                std::thread::spawn(move || watch_stall(&watching));
            }
            break Ok(shared.floor(false));
        }
        match rx.recv_timeout(remaining.min(CANCEL_POLL)) {
            Ok(OpMsg::Meta { line }) => {
                // Recheck after receiving too, so cancellation cannot be followed by a queued exact ending.
                if cancel.check().is_err() || Instant::now() >= bound {
                    continue;
                }
                let terminal = !field_bool(&line, "ok")
                    || field_bool(&line, "done")
                    || field_bool(&line, "cancelled");
                if replies.send(OpMsg::Meta { line }).is_err() {
                    stop.next();
                    return false;
                }
                if terminal {
                    return true;
                }
            }
            Ok(_) => unreachable!("Properties emits only menu replies"),
            Err(RecvTimeoutError::Timeout) => (),
            Err(RecvTimeoutError::Disconnected) => {
                break Err("The selected items stopped answering.".into())
            }
        }
    };
    stop.next();
    replies
        .send(OpMsg::Meta {
            line: response(line, ending),
        })
        .is_ok()
}

// Only a worker still silent inside a syscall marks its current remote mount; counting or completed work marks none.
fn mark_if_stalled(shared: &State) -> bool {
    let state = shared.status.lock().unwrap();
    if state.finished {
        return true;
    }
    if state.moved.elapsed() >= iomount::CALL_DEADLINE {
        walkmounts::mark_holding(&state.at, &shared.body);
        return true;
    }
    false
}

fn watch_stall(shared: &State) {
    while !mark_if_stalled(shared) {
        std::thread::sleep(CANCEL_POLL);
    }
}

// This walk belongs to the aggregate supervisor, so it has no second deadline or waiting worker.
pub(super) fn count(
    item: &Selected,
    cancel: &Cancellation,
    work: &State,
    report: &mut dyn FnMut(Result<Counted, String>) -> bool,
) -> bool {
    work.before(&item.path);
    let meta = match item.current() {
        Ok(meta) if meta.is_dir() => meta,
        Ok(_) => return report(Err("Only a folder has contents to count.".into())),
        Err(error) => return report(Err(error)),
    };
    if !report(Ok(Counted {
        tally: Tally {
            bytes: meta.len(),
            items: 0,
            partial: false,
            linked: false,
        },
        done: false,
        cancelled: false,
    })) {
        return false;
    }
    let mounts: HashSet<PathBuf> = crate::backend::mountinfo::mounts_in(&work.body)
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    let open = std::cell::Cell::new(true);
    let stopped = || cancel.check().is_err() || !open.get();
    let mut progress = |tally: &Tally| {
        #[cfg(test)]
        if let Some(reached) = &work.panic_on_progress {
            reached.store(true, std::sync::atomic::Ordering::SeqCst);
            panic!("test: Properties worker panicked during directory walk");
        }
        if !report(Ok(Counted {
            tally: *tally,
            done: false,
            cancelled: false,
        })) {
            open.set(false);
        }
    };
    let entering = |path: &Path| work.before(path);
    let dev_of = |path: &Path| mounts.contains(path).then_some(u64::MAX);
    let tally = walk_core(
        &item.path,
        &stopped,
        Some(&dev_of),
        Some(Progress {
            every: Duration::ZERO,
            report: &mut progress,
            entering: Some(&entering),
        }),
    );
    let cancelled = cancel.check().is_err();
    open.get()
        && report(Ok(Counted {
            tally,
            done: !cancelled,
            cancelled,
        }))
}

#[cfg(test)]
#[path = "menuproperties_remote_tests.rs"]
mod tests;
