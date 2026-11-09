// Issue 144: a remote list scans on one kept thread and answers as an event, and requests that arrive meanwhile wait in `Gate` and replay in order; a local list runs inline.
use crate::backend::events::Event;
use crate::backend::iomount::{internal_failure, ListErr, ListOut};
use crate::backend::proto::{error_line, error_line_with_mode, parse_request, Request};
use crate::backend::run::{adopt_listed, adopt_session, forget_rows};
use crate::backend::searchreq::finish_search;
use crate::backend::state::{Held, State, Tables};
use crate::backend::thumbs::Pool;
use crate::backend::watch::Watch;
use std::collections::VecDeque;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{channel, SendError, Sender};
use std::sync::Arc;
use std::thread;

// One scan the loop has started and not yet adopted, with the request it answers.
pub struct Job {
    pub path: String,
    pub first: usize,
    // The request line, because ordering and the picker filter read it on the scan's own thread.
    pub line: String,
    pub hidden: bool,
    // The reload count rides the landed listing, so it travels with the job rather than the line.
    pub want_changed: bool,
    // Set when the scan answers a search session's empty query rather than replacing the listing.
    pub session: Option<ForSession>,
}

// The session gen an empty query claimed, and whether its walk was running at the claim.
#[derive(Clone, Copy)]
pub struct ForSession {
    pub gen: usize,
    pub walking: bool,
}

// The scan's own answer, carried back on the loop's one channel like a thumbnail's or a size's.
pub struct Done {
    // A failure carries the mode the worker read, because that stat can hang on the same stalled mount.
    pub result: Result<ListOut, ListErr>,
}

// A job and the answer that closes it, so the loop hands the pair over in one argument.
pub struct Landed {
    pub job: Job,
    pub done: Done,
}

type Work = Box<dyn FnOnce() + Send>;

// What became of the scan out (reading, answered with its Event::List, or let go for a newer list); worker and loop each try to leave READING and the first decides, so a let-go scan never also answers.
const READING: u8 = 0;
const ANSWERED: u8 = 1;
const LET_GO: u8 = 2;

// What waits for a scan: the job, the requests behind it, and the watch bursts and unmounts seen before the new descriptor is known.
pub struct Gate {
    // The loop's channel: the scan answers on it and its worker hands a late watch descriptor back on it.
    answers: crate::backend::events::Sender<Event>,
    // The one thread every remote scan runs on, started by the first remote list, so no later list pays a thread start.
    scanner: Option<Sender<Work>>,
    // The newest scan's fate, shared with its worker.
    fate: Arc<AtomicU8>,
    job: Option<Job>,
    deferred: VecDeque<String>,
    changes: Vec<i32>,
    unmounts: Vec<i32>,
    // A poll's change or a probe's figures for the folder being left, which wait like a burst and are judged against the folder that lands.
    held: Vec<Event>,
    // Descriptors a let-go scan handed back while another scan was out; inotify gives the same one for the same folder.
    late: Vec<i32>,
    closed: bool,
}

impl Gate {
    pub fn new(answers: crate::backend::events::Sender<Event>) -> Gate {
        Gate { answers, scanner: None, fate: Arc::new(AtomicU8::new(ANSWERED)), job: None, deferred: VecDeque::new(), changes: Vec::new(), unmounts: Vec::new(), held: Vec::new(), late: Vec::new(), closed: false }
    }

    // True while a scan is out, which is when a request cannot be answered against the listing it will replace.
    pub fn holds(&self) -> bool {
        self.job.is_some()
    }

    // The loop's channel, so an inline scan names the same answer path its worker would.
    pub fn answers(&self) -> crate::backend::events::Sender<Event> {
        self.answers.clone()
    }

    pub fn begin(&mut self, job: Job) {
        self.job = Some(job);
    }

    // Lets a plain list scan of another folder go so a newer list starts at once; false when none is out, it answers a search session, or its answer is already on the way.
    pub fn let_go(&mut self, path: &str) -> bool {
        let plain = self.job.as_ref().is_some_and(|job| job.session.is_none() && job.path != path);
        if !plain || self.fate.compare_exchange(READING, LET_GO, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return false;
        }
        self.job = None;
        // The let-go thread ends with its read, and the next remote list starts a fresh one.
        self.scanner = None;
        true
    }

    // Runs the scan on the kept thread; it answers exactly once, a panic included, or the gate would hold for good.
    fn scan(&mut self, path: String, work: impl FnOnce(crate::backend::events::Sender<Event>) -> Result<ListOut, ListErr> + Send + 'static) {
        let (answers, loop_tx) = (self.answers.clone(), self.answers.clone());
        let fate = Arc::new(AtomicU8::new(READING));
        self.fate = Arc::clone(&fate);
        let run: Work = Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(|| work(loop_tx))).unwrap_or_else(|_| {
                Err(ListErr {
                    error: internal_failure("scan", &flea::bytename::view(&path)),
                    mode: 0,
                    watch_wd: -1,
                })
            });
            if fate
                .compare_exchange(READING, ANSWERED, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let _ = answers.send(Event::List(Done { result }));
                return;
            }
            if let Ok(listed) = &result {
                for &wd in &listed.listing.outline_watches { let _ = answers.send(Event::AbandonWatch(wd)); }
            }
            // Let go for a newer list: nothing is answered, and the watch this scan armed goes back to the loop.
            let wd = match &result { Ok(listed) => listed.watch_wd, Err(failed) => failed.watch_wd };
            if wd >= 0 {
                let _ = answers.send(Event::AbandonWatch(wd));
            }
        });
        let scanner = self.scanner.get_or_insert_with(|| {
            let (jobs, queue) = channel::<Work>();
            thread::spawn(move || queue.into_iter().for_each(|run| run()));
            jobs
        });
        // A scanner that is gone leaves this scan to the caller and is started afresh next time, so the gate still answers.
        if let Err(SendError(run)) = scanner.send(run) {
            self.scanner = None;
            run();
        }
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

    // An unmount waits like a change does, but is replayed one by one, in the order it arrived.
    pub fn note_unmount(&mut self, wd: i32) {
        self.unmounts.push(wd);
    }

    pub fn take_unmounts(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.unmounts)
    }

    pub fn hold(&mut self, event: Event) {
        self.held.push(event);
    }

    pub fn take_held(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.held)
    }

    // A late descriptor comes back at once with the ones that waited, or waits while a scan is out that may have armed the same one.
    pub fn park_late(&mut self, wd: i32) -> Vec<i32> {
        self.late.push(wd);
        self.release_late()
    }

    // The descriptors that waited, once no scan is out.
    pub fn release_late(&mut self) -> Vec<i32> {
        if self.holds() { Vec::new() } else { std::mem::take(&mut self.late) }
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

// The folder a `list` request names, None for any other line.
pub fn list_path(line: &str) -> Option<String> {
    match parse_request(line) {
        Request::List { path, .. } => Some(path),
        _ => None,
    }
}

// Replays the requests that waited, in order, until one starts a scan or none is left; true when one asked the loop to quit.
pub fn replay(gate: &mut Gate, mut handle: impl FnMut(&str, &mut Gate) -> bool) -> bool {
    while !gate.holds() {
        let Some(line) = gate.next() else { break };
        if handle(&line, gate) {
            return true;
        }
    }
    false
}

// How start answered: inline runs list_dir on the loop, remote on the kept scan thread.
pub enum Started {
    Inline(Box<Landed>),
    Remote,
}

// Sample input: {"c":"list","path":"/home/gm","outlinePaths":["/home/gm/NAS/share"]}
// An open child mount can block even when its outline root is local.
pub(crate) fn threaded(path: &Path, line: &str, body: &str) -> bool {
    super::iomount::is_remote(path, body) || crate::json::field_str_array(line, "outlinePaths")
        .iter().map(Path::new).any(|child| child.starts_with(path) && super::iomount::is_remote(child, body))
}

// Classifies once, so the local path costs no more than v0.3.8's inline call.
pub fn start(out: &mut impl Write, st: &mut State, pool: &Pool, watch: &mut Watch, gate: &mut Gate,
             job: Job, mime: Arc<super::mime::Db>) -> Started {
    // A new listing replaces whatever the walk was filling, so the walk ends before the scan starts.
    if finish_search(out, st, true) {
        forget_rows(st, pool);
    }
    let body = super::iomount::mount_body();
    if !threaded(&flea::bytename::view(&job.path), &job.line, &body) {
        // A local scan runs inline and a panic ends the process, as v0.3.8 did.
        let fd = watch.raw_fd();
        let tx = gate.answers();
        let result = super::iomount::list_dir(job.path.clone(), job.hidden, job.first, job.line.clone(), mime, fd, tx, &body);
        return Started::Inline(Box::new(Landed { job, done: Done { result } }));
    }
    // Read here, because the watch lives on the loop and the worker only carries the number.
    let fd = watch.raw_fd();
    let (path, line, hidden, first) = (job.path.clone(), job.line.clone(), job.hidden, job.first);
    gate.begin(job);
    gate.scan(path.clone(), move |tx| super::iomount::list_dir(path, hidden, first, line, mime, fd, tx, &body));
    Started::Remote
}

// A landed scan becomes the listing and a failed one leaves it alone; true when it became the listing.
pub fn finish<W: Write>(out: &mut W, st: &mut State, tb: &Tables, pool: &Pool, watch: &mut Watch, landed: Landed) -> bool {
    let Landed { job, done } = landed;
    let Job { path, first: _, line: _, hidden: _, want_changed, session: _ } = job;
    let landed = match done.result {
        Ok(listed) => {
            watch.set_incoming(listed.watch_wd, listed.watch_mount.clone());
            watch.commit(listed.listing.outline_watches.clone());
            // Said once per listing, because a folder nobody can watch goes stale in silence.
            if watch.refused() {
                eprintln!("flea: {} will not follow outside changes, inotify refused a watch on it", path);
            }
            adopt_listed(out, st, pool, tb, &path, listed, want_changed);
            out.flush().ok();
            true
        }
        Err(failed) => {
            // The listing did not move, so the loop removes the watch the failed scan armed.
            watch.abandon_wd(failed.watch_wd);
            say_failed(out, &failed);
            false
        }
    };
    out.flush().ok();
    crate::prefetch::first_rows_sent();
    landed
}

// An empty query's folder answer, written only while its session still stands at the gen it claimed.
pub fn finish_session<W: Write>(
    out: &mut W,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    watch: &mut Watch,
    landed: Landed,
    scan: ForSession,
) {
    let Landed { job, done } = landed;
    let current = st.search_session.as_ref().is_some_and(|session| {
        session.last_answered_gen == scan.gen && session.folder == *flea::bytename::view(&job.path)
    });
    match done.result {
        Ok(listed) => {
            // The folder's own listing answers, so the armed watch the scan added goes back.
            watch.abandon_wd(listed.watch_wd);
            if !current {
                return;
            }
            let n = listed.listing.len();
            if let Some(session) = st.search_session.as_mut() {
                session.note_folder_count(n);
            }
            let ms = listed.read_ms + listed.sort_ms;
            adopt_session(out, st, pool, tb, &job.path, listed, scan.gen);
            st.held = Held::Walk;
            let reason = st.search_session.as_ref().and_then(|session| session.partial_reason());
            writeln!(out, "{}", super::searchreq::searched_status_line(n, n, ms, scan.gen, scan.walking, reason)).ok();
        }
        Err(failed) => {
            watch.abandon_wd(failed.watch_wd);
            if current {
                say_failed(out, &failed);
            }
        }
    }
    out.flush().ok();
}

// The denial's mode rides the answer from the worker, because a stat here would hang the loop on a stalled mount.
fn say_failed(out: &mut impl Write, failed: &ListErr) {
    if failed.mode == 0 {
        writeln!(out, "{}", error_line(&failed.error)).ok();
    } else {
        writeln!(out, "{}", error_line_with_mode(&failed.error, failed.mode)).ok();
    }
}

#[cfg(test)]
#[path = "listwork_tests.rs"]
mod tests;
