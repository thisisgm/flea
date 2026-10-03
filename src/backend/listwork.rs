// Issue 144: `list` used to scan inline on the event loop, so a `read_dir` on an MTP or FUSE mount
// held the loop for as long as the device took. Transfer progress piled up unwritten and a
// `transfercancel` sat unread behind the scan, so the copy ran on and the window looked hung. The
// scan runs on its own thread here and answers as an event, and every request that arrived while it
// was out waits in `Gate` and replays in the order it arrived, so the wire's request ordering is
// exactly what it was when the scan was inline.
use crate::backend::events::Event;
use crate::backend::fsinforeq::FsInfo;
use crate::backend::listing::Listing;
use crate::backend::ordering;
use crate::backend::proto::{error_line, error_line_with_mode};
use crate::backend::run::{adopt, forget_rows};
use crate::backend::scan::{mode_of, scan};
use crate::backend::searchreq::finish_search;
use crate::backend::state::{State, Tables};
use crate::backend::thumbs::Pool;
use crate::backend::watch::Watch;
use crate::error::FleaError;
use std::collections::VecDeque;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

// One scan the loop has started and not yet adopted, with the request it answers.
pub struct Job {
    pub path: String,
    pub first: usize,
    // The request line, because ordering and the picker filter are read from it after the scan lands.
    pub line: String,
}

// The scan's own answer, carried back on the loop's one channel like a thumbnail's or a size's.
pub struct Done {
    pub result: Result<(Listing, f64), FleaError>,
    // The failed scan's st_mode, read on the scan's own thread. The stat a denied listing answers
    // with can hang on the same stalled mount the read_dir did, so it is read where the scan was,
    // before this answer is sent, and never here on the loop after it lands. None on success.
    pub mode: Option<u32>,
}

// A job and the answer that closes it, so the loop hands the pair over in one argument.
pub struct Landed {
    pub job: Job,
    pub done: Done,
}

// Everything that waits for a scan: the job in flight, the requests that arrived behind it, and the
// watch bursts seen while the descriptor that will be current is not yet known.
pub struct Gate {
    job: Option<Job>,
    deferred: VecDeque<String>,
    changes: Vec<i32>,
    closed: bool,
}

impl Gate {
    pub fn new() -> Gate {
        Gate { job: None, deferred: VecDeque::new(), changes: Vec::new(), closed: false }
    }

    // True while a scan is out, which is when a request cannot be answered against the listing it will replace.
    pub fn holds(&self) -> bool {
        self.job.is_some()
    }

    pub fn begin(&mut self, job: Job) {
        self.job = Some(job);
    }

    pub fn take(&mut self) -> Option<Job> {
        self.job.take()
    }

    pub fn defer(&mut self, line: String) {
        self.deferred.push_back(line);
    }

    pub fn next(&mut self) -> Option<String> {
        self.deferred.pop_front()
    }

    pub fn note_change(&mut self, wd: i32) {
        self.changes.push(wd);
    }

    pub fn take_changes(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.changes)
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    // Nothing outstanding and nothing queued: the point at which a closed stdin can leave.
    pub fn idle(&self) -> bool {
        self.job.is_none() && self.deferred.is_empty()
    }
}

// A list request starts the scan and returns at once with what it will need to answer: the walk ends
// and the watch is armed here, on the loop, exactly as it was before the scan moved off it.
pub fn start(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool, watch: &mut Watch, tx: &Sender<Event>,
             job: Job, hidden: bool) -> Job {
    // A new listing replaces whatever the walk was filling, so the walk ends before the scan starts.
    if finish_search(out, st, true) {
        forget_rows(st, pool);
    }
    // Before the scan, because a change readdir raced is missing from the rows this answers with.
    watch.begin(Path::new(&job.path));
    spawn_scan(tx.clone(), job.path.clone(), hidden, scan, mode_of);
    job
}

// The scan landed: it becomes the listing and is answered, or it failed and the listing does not move.
pub fn finish<W: Write>(out: &mut W, st: &mut State, tb: &Tables, pool: &Pool, watch: &mut Watch,
              fsinfo: &mut FsInfo, landed: Landed) {
    let Landed { job, done } = landed;
    let Job { path, first, line } = job;
    let Done { result, mode } = done;
    match result {
        Ok((mut l, read_ms)) => {
            super::picker::filter_listing(&mut l, &tb.mime, &line);
            let (pass_ms, sort_ms, sized) = match ordering::request(&mut l, Path::new(&path), &tb.mime, &line) {
                Ok(timing) => timing,
                Err(msg) => {
                    watch.abandon();
                    writeln!(out, "{}", error_line(&FleaError { where_: "sort".into(), path: path.clone(), msg: msg.into() })).ok();
                    out.flush().ok();
                    return;
                }
            };
            watch.commit();
            // Said once per listing, because a folder nobody can watch goes stale in silence.
            if watch.refused() {
                eprintln!("flea: {} will not follow outside changes, inotify refused a watch on it", path);
            }
            adopt(out, st, pool, tb, &path, l, (read_ms + pass_ms, sort_ms), &sized, first);
            out.flush().ok();
            // After the rows, because a statfs beside gio's own listing slows it on the share.
            fsinfo.list_arrived(Path::new(&path));
        }
        Err(e) => {
            // The listing did not move, so neither does its watch.
            watch.abandon();
            // A typed path reaches the denial with no parent row to remember the mode from, so the
            // stat that survives the refused read is the pane's only source for it. It was read on
            // the scan's own thread, because on a stalled mount it hangs exactly as the read did.
            writeln!(out, "{}", error_line_with_mode(&e, mode.unwrap_or(0))).ok();
        }
    }
    out.flush().ok();
    crate::prefetch::first_rows_sent();
}

// The scan runs off the loop and answers on the loop's own channel, the way a thumbnail or a size does.
// The mode a failure answers with is read here too: a test can hand in a stat it can hold open, which
// is the only way to prove from outside that the loop never reads it.
fn spawn_scan(tx: Sender<Event>, path: String, hidden: bool,
              run: impl FnOnce(&str, bool) -> Result<(Listing, f64), FleaError> + Send + 'static,
              stat: impl FnOnce(&str) -> u32 + Send + 'static) {
    thread::spawn(move || {
        let result = run(&path, hidden);
        // Only a failure answers a mode, so a successful scan never pays for the stat.
        let mode = match &result {
            Ok(_) => None,
            Err(_) => Some(stat(&path)),
        };
        let _ = tx.send(Event::List(Done { result, mode }));
    });
}

#[cfg(test)]
#[path = "listwork_tests.rs"]
mod tests;
