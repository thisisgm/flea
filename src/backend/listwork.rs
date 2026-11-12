// Issue 144: a remote list scans on one kept thread and answers as an event, and requests that arrive meanwhile wait in `Gate` and replay in order; a local list runs inline.
#[cfg(test)]
use crate::backend::events::Event;
use crate::backend::iomount::{ListErr, ListOut};
use crate::backend::proto::{error_line, error_line_with_mode, parse_request, Request};
use crate::backend::run::{adopt_listed, adopt_session, forget_rows};
use crate::backend::searchreq::finish_search;
use crate::backend::state::{Held, State, Tables};
use crate::backend::thumbs::Pool;
use crate::backend::watch::Watch;
use crate::backend::watchpoll::Poller;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

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

#[path = "listgate.rs"]
mod gate;
pub use gate::Gate;
#[cfg(test)]
use {gate::Work, std::thread};
#[path = "requestgen.rs"]
pub(crate) mod requestgen;

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
    super::iomount::is_remote(path, body)
        || crate::json::field_str_array(line, "outlinePaths")
            .iter()
            .map(Path::new)
            .any(|child| child.starts_with(path) && super::iomount::is_remote(child, body))
}

// Classifies once, so the local path costs no more than v0.3.8's inline call.
pub fn start(
    out: &mut impl Write,
    st: &mut State,
    pool: &Pool,
    watch: &mut Watch,
    gate: &mut Gate,
    job: Job,
    mime: Arc<super::mime::Db>,
) -> Started {
    // A new listing replaces whatever the walk was filling, so the walk ends before the scan starts.
    if finish_search(out, st, true) {
        forget_rows(st, pool);
    }
    let body = super::iomount::mount_body();
    if !threaded(&flea::bytename::view(&job.path), &job.line, &body) {
        // A local scan runs inline and a panic ends the process, as v0.3.8 did.
        let fd = watch.raw_fd();
        let tx = gate.answers();
        let result = super::iomount::list_dir(
            job.path.clone(),
            job.hidden,
            job.first,
            job.line.clone(),
            mime,
            fd,
            tx,
            &body,
        );
        return Started::Inline(Box::new(Landed {
            job,
            done: Done { result },
        }));
    }
    // Read here, because the watch lives on the loop and the worker only carries the number.
    let fd = watch.raw_fd();
    let (path, line, hidden, first) = (job.path.clone(), job.line.clone(), job.hidden, job.first);
    gate.begin(job);
    gate.scan(path.clone(), move |tx| {
        super::iomount::list_dir(path, hidden, first, line, mime, fd, tx, &body)
    });
    Started::Remote
}

// A landed scan becomes the listing and a failed one leaves it alone; true when it became the listing.
pub fn finish<W: Write>(
    out: &mut W,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    watch: &mut Watch,
    poller: &Poller,
    landed: Landed,
) -> bool {
    let out =
        &mut requestgen::Writer::new(out, crate::json::field_usize(&landed.job.line, "pickerGen"));
    let Landed { job, done } = landed;
    let Job {
        path,
        first: _,
        line: _,
        hidden: _,
        want_changed,
        session: _,
    } = job;
    if super::unmount::crossed_answers(out, watch, &flea::bytename::view(&path), &done.result) {
        out.flush().ok();
        return false;
    }
    let landed = match done.result {
        Ok(listed) => {
            watch.set_incoming(listed.watch_wd, listed.watch_mount.clone());
            watch.commit(listed.listing.outline_watches.clone());
            // Said once per listing, because a folder nobody can watch goes stale in silence.
            if watch.refused() {
                eprintln!(
                    "flea: {} will not follow outside changes, inotify refused a watch on it",
                    path
                );
            }
            adopt_listed(out, st, pool, tb, &path, listed, want_changed);
            watch.ancestry.follow(&st.base);
            out.flush().ok();
            true
        }
        Err(failed) => {
            // The listing did not move, so the loop removes the watch the failed scan armed.
            watch.abandon_wd(failed.watch_wd);
            if !super::unmount::gone_answers(
                out,
                st,
                watch,
                poller,
                &flea::bytename::view(&path),
                &failed,
            ) {
                say_failed(out, &failed);
            }
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
            let reason = st
                .search_session
                .as_ref()
                .and_then(|session| session.partial_reason());
            writeln!(
                out,
                "{}",
                super::searchreq::searched_status_line(n, n, ms, scan.gen, scan.walking, reason)
            )
            .ok();
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
