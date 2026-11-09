#[cfg(test)]
#[path = "trashthumb_protocol_tests.rs"]
mod trashthumb_protocol_tests;
#[cfg(test)]
#[path = "contents_tests.rs"]
mod contents_tests;
#[cfg(test)]
#[path = "contents_roundtrip_tests.rs"]
mod contents_roundtrip_tests;
use crate::backend::archivereq::{formats_line, start_archive, start_convert};
use crate::backend::convert;
use crate::backend::peekwatch::PeekWatch;
use crate::backend::metareq::spawn as spawn_meta;
use crate::backend::opsdispatch::{cancel_transfer, do_mkdir, do_newfile, do_permissions_batch, do_rename, do_undo, report_op, resolve_rows, start_duplicate, start_trash, start_transfer, start_menu_transfer, start_redo, Ops};
use crate::backend::opsreq::OpMsg;
use crate::backend::dirsizereq::{queue_dirsizes, seed_answered, start_next, report_done as report_dirsize};
use crate::backend::events::{spawn_op_forwarder, Event, Sender};
use crate::backend::{flashop, flashslot};
use crate::backend::fsinfo::fsinfo_line;
use crate::backend::fsinforeq::FsInfo;
use crate::backend::listpaths;
use crate::backend::listwork::{self, Gate};
use crate::backend::proto::{error_line, listed_line, parse_request, paths_line, thumbed_line, Request};
use crate::backend::proto::with_anchor;
pub use super::windowreq::{write_window, write_window_gen};
pub(crate) use super::windowreq::window_metas;
use super::windowreq::window_line;
use crate::backend::listing::Listing;
use crate::backend::search::Search;
use crate::backend::searchjobs::route_delta;
use crate::backend::searchsession::{Session, FIRST_WINDOW_DEFAULT};
use crate::backend::state::{Held, State, Tables};
use crate::backend::searchreq::{collapse_query_burst, emit_session_answer, finish_search, is_search_query_line,
    op_tag_paths, op_write_paths, searchclosed_line, searchopened_line, step_search};
use crate::backend::thumbcache::Cache;
use crate::backend::thumbreq::{cancel_all, cancel_row, report_done, thumb_rows};
use crate::backend::thumbs::Pool;
#[cfg(test)]
use crate::backend::thumbs::Done;
use crate::backend::thumbwrite::sweep_own_temps;
use crate::backend::unmount::leave_mount;
use crate::backend::watch::{changed_line, Watch};
use crate::error::FleaError;
use crate::heap;
use std::collections::VecDeque;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Wider pools settle sooner and answer input later: 6 settles the media fixture sooner than strata for 5.5 ms of input under a full pool; see AGENTS.md "Thumbnail requests".
const THUMB_WORKERS: usize = 6;
// The whole shutdown budget: a running job is killed at the pool's own 20 s deadline, so waiting longer than that can never cut one short.
pub(crate) const DRAIN_LIMIT: Duration = Duration::from_secs(25);
// The UI gives up on a silent child after this; the budget above stays under it (ui/Backend.qml quitDeadline).
pub(crate) const UI_QUIT_DEADLINE_SECS: u64 = 30;
const _: () = assert!(DRAIN_LIMIT.as_secs() < UI_QUIT_DEADLINE_SECS);
const _: () = assert!(crate::backend::archivework::CANCEL_DRAIN_SECS < DRAIN_LIMIT.as_secs());

// The loop stops on Quit; every other request continues it, because errors are responses.
#[derive(PartialEq)]
enum Control {
    Continue,
    Quit,
}

// Errors are responses, so the loop never exits on a bad request.
pub fn run() -> i32 {
    if let Some(status) = super::model3dreq::worker_entry() { return status }
    // Before the first listing, because a threshold glibc has already ratcheted strands the next arena on the heap.
    heap::pin_mmap_threshold();
    let mut out = BufWriter::new(io::stdout());
    let tb = Tables::load();
    let (tx, rx, inbox) = match super::inbox::open() { Ok(open) => open, Err(e) => { say(&mut out, &error_line(&e)); return 1 } };
    let mut st = State::new(super::dirsizeworker::Worker::new(tx.clone()));
    let (op_tx, op_rx) = channel::<OpMsg>();
    let mut ops = Ops::new_shared(op_tx.clone());
    let op_events = tx.clone();
    ops.tx = super::lazy::DeferredSender::new(move || {
        spawn_op_forwarder(op_rx, op_events);
        op_tx
    });
    let pool = Pool::lazy(THUMB_WORKERS, &tb, super::thumbcache::default_root(), tx.clone());
    let cache = Cache::new();
    // One flash at a time beside transfers; its worker reports on its own channel, joined the same way.
    let (flash_tx, flash_rx) = channel::<flashslot::FlashMsg>();
    let mut flash = flashslot::Ctx::new(flash_tx.clone());
    let flash_events = tx.clone();
    flash.tx = super::lazy::DeferredSender::new(move || {
        flashslot::spawn_forwarder(flash_rx, flash_events);
        flash_tx
    });
    let mut inbox = inbox.start(super::inbox::STDIN, tx.clone(), Arc::clone(&ops.live));
    let mut fsinfo = FsInfo::new(tx.clone());
    let loop_tx = tx.clone();
    // The descriptor starts on first use, still armed before the listing scan.
    let mut watch = Watch::start(tx.clone());
    let mut peeks = PeekWatch::start(tx.clone());
    let poller = super::watchpoll::Poller::new(tx.clone());
    // Issue 144: requests that land while a list scan is out wait here and replay in order; see src/backend/listwork.rs.
    let mut gate = Gate::new(loop_tx.clone());
    // Drained events wait here in order, so a collapsed query burst loses no sibling line.
    let mut buffered: VecDeque<Event> = VecDeque::new();
    let mut search_jobs = super::searchjobs::Gate::default();
    loop {
        start_next(&mut st);
        // A request or any send wakes the loop here, with no hop through another thread; only search still needs idle ticks.
        let event = if let Some(ev) = buffered.pop_front() {
            ev
        } else if st.search.is_none() {
            match inbox.next(&rx) { Some(e) => e, None => break }
        } else {
            inbox.hand_over();
            match rx.try_recv() {
                Ok(e) => e,
                // Search takes one bounded step before checking requests again.
                Err(TryRecvError::Empty) => {
                    tick_walkers(&mut out, &mut st, &pool);
                    continue;
                }
                Err(TryRecvError::Disconnected) => break,
            }
        };
        let Some(event) = collapse_burst(&rx, event, &mut buffered, st.search_session.is_some() || search_jobs.busy(), gate.holds(), &mut search_jobs) else { continue };
        let Some(event) = search_jobs.route(event, &mut st, &mut out, &tb, &pool, &loop_tx, &mut buffered, gate.holds()) else { continue };
        if dispatch_inner(event, &mut out, &mut st, &tb, &pool, &cache, &mut ops, &mut flash, &mut watch, &mut peeks, &mut fsinfo, &poller, &loop_tx, &mut gate, Some(&mut search_jobs)) == Control::Quit {
            break;
        }
    }
    st.nearby.close();
    st.contents.clear();
    st.dirsize_worker.cancel();
    drain(&mut out, &mut st, &mut ops, &mut flash, &rx, &pool, &cache);
    0
}

// A burst answers only its newest query, in its own place; a held burst collapses too, as the session a deferred searchopen opens is not visible yet.
fn collapse_burst(rx: &Receiver<Event>, event: Event, buffered: &mut VecDeque<Event>, session: bool, held: bool, jobs: &mut super::searchjobs::Gate) -> Option<Event> {
    match event {
        Event::Request(line) if is_search_query_line(&line) && (session || held) => {
            let (now, gen) = collapse_query_burst(rx, line, buffered);
            jobs.supersede(gen);
            now.map(Event::Request)
        },
        other => Some(other),
    }
}

// One event against everything the loop owns, so a test drives the arms the loop runs; Quit ends the loop.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn dispatch<W: Write>(event: Event, out: &mut W, st: &mut State, tb: &Tables, pool: &Pool, cache: &Cache, ops: &mut Ops,
                      flash: &mut flashslot::Ctx, watch: &mut Watch, peeks: &mut PeekWatch, fsinfo: &mut FsInfo,
                      poller: &super::watchpoll::Poller, loop_tx: &Sender<Event>, gate: &mut Gate) -> Control {
    dispatch_inner(event, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate, None)
}

#[allow(clippy::too_many_arguments)]
fn dispatch_inner<W: Write>(event: Event, out: &mut W, st: &mut State, tb: &Tables, pool: &Pool, cache: &Cache, ops: &mut Ops,
                      flash: &mut flashslot::Ctx, watch: &mut Watch, peeks: &mut PeekWatch, fsinfo: &mut FsInfo,
                      poller: &super::watchpoll::Poller, loop_tx: &Sender<Event>, gate: &mut Gate,
                      mut search_jobs: Option<&mut super::searchjobs::Gate>) -> Control {
    match event {
        Event::Contents(done) => super::contentsreq::done(out, st, tb, pool, loop_tx, done, (watch, poller)),
        Event::Request(line) => {
            if super::nearby::is_request(&line) {
                return handle_line(&line, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate);
            }
            // A list for another folder takes the place of a plain list scan still out, after the requests that waited replay against the listing still current.
            while gate.holds() && listwork::list_path(&line).is_some_and(|path| gate.let_go(&path)) {
                let quit = listwork::replay(gate, |waiting, gate| handle_with_jobs(waiting, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate, search_jobs.as_deref_mut()) == Control::Quit);
                gate.release_late().into_iter().for_each(|late| watch.abandon_wd(late)); // a local list answers inline, with no Event::List to release them
                for held in gate.take_held() { dispatch_inner(held, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate, search_jobs.as_deref_mut()); }
                if quit { return Control::Quit; }
            }
            // A scan is out, so a later request cannot be answered against a listing it will replace.
            if gate.holds() {
                gate.defer(line);
                return Control::Continue;
            }
            return handle_with_jobs(&line, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate, search_jobs.as_deref_mut());
        }
        // The scan answered: it becomes the listing, then every request that waited replays in order.
        Event::List(done) => {
            if let Some(job) = gate.take() {
                land(out, st, tb, pool, watch, fsinfo, poller, listwork::Landed { job, done });
                // A change or an unmount seen during the scan is answered only if it was in the directory that became current.
                if gate.take_changes().iter().any(|&wd| watch.is_current(wd)) {
                    say_changed(out, st);
                }
                gate.take_unmounts().into_iter().for_each(|wd| leave_mount(out, &st.base, watch, poller, wd));
                // A poll or a probe seen during the scan is judged against the folder that landed.
                for held in gate.take_held() { dispatch_inner(held, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate, search_jobs.as_deref_mut()); }
                // An empty query's landing carries no scored rows and follows its folder answer immediately.
                let empty = st.search_session.as_ref().is_some_and(|session| session.held.is_none()
                    && session.standing.as_ref().is_some_and(|(_, query, _)| query.is_empty()));
                if empty { on_search_walked(out, st, tb, pool); }
                else if let Some(jobs) = search_jobs.as_deref_mut() {
                    jobs.route(Event::SearchWalked, st, out, tb, pool, loop_tx, &mut VecDeque::new(), false);
                } else { on_search_walked(out, st, tb, pool); }
            }
            gate.release_late().into_iter().for_each(|late| watch.abandon_wd(late)); // judged against the watch that landed
            if listwork::replay(gate, |waiting, gate| handle_with_jobs(waiting, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate, search_jobs.as_deref_mut()) == Control::Quit) {
                return Control::Quit;
            }
            // stdin closed while the scan was out: the listing was answered first, so now leave.
            if gate.closed() && gate.idle() {
                if let Some(jobs) = search_jobs.filter(|jobs| jobs.busy()) { jobs.close_after_waiting(); }
                else { return Control::Quit; }
            }
        }
        Event::Git(line) => say(out, &line),
        Event::Thumb(d) => report_done(out, st, pool, d),
        Event::Nearby { ticket, line } => if st.nearby.accept(ticket) { say(out, &line) },
        Event::Flash(m) => flash.report(out, &m),
        Event::DirSize(d) => report_dirsize(out, st, d),
        // A slow mount's figures, printed only for the directory on screen now.
        Event::FsInfo(d) if gate.holds() => gate.hold(Event::FsInfo(d)),
        Event::FsInfo(d) => {
            if let Some(info) = fsinfo.finish(d, &st.base) {
                say(out, &fsinfo_line(&info, &st.base.to_string_lossy(), crate::backend::fsinforeq::slow_class(&st.base), fsinfo.tagable_shown(&st.base), false));
            }
        }
        // The one line no client asked for, and only ever for the directory being listed now.
        Event::Changed(wd) => {
            #[cfg(test)]
            super::renametiming::mark("watch_begin");
            // The current descriptor is unknown while a scan is out, so the burst waits for the scan's answer.
            if gate.holds() {
                gate.note_change(wd);
            } else if watch.is_current(wd) {
                say_changed(out, st);
            }
            #[cfg(test)]
            super::renametiming::mark("watch_end");
        }
        // The open mount went away; while a scan is out the current descriptor is unknown, so it waits for the scan's answer.
        Event::Unmounted(wd) => if gate.holds() { gate.note_unmount(wd) } else { leave_mount(out, &st.base, watch, poller, wd) },
        // A network folder's mtime moved, which inotify never delivers; only the open path counts, and while a scan is out it waits for the scan's answer.
        Event::PollChanged(path) if gate.holds() => gate.hold(Event::PollChanged(path)),
        Event::PollChanged(path) => {
            if super::outline::watched(&st.listing, &st.base, &path) {
                say(out, &changed_line(&st.base));
            }
        }
        // A column's directory changed: the line the listed folder gets, for a path the pane is not on.
        Event::PeekArmed(wd, path) => peeks.register(wd, path),
        Event::PeekChanged(wd) => if let Some(path) = peeks.path_of(wd) { say(out, &changed_line(path)) },
        Event::PeekGone(wd) => peeks.forget(wd),
        // A late list worker's descriptor goes unless a re-list aliased it onto the live watch, which a scan still out may not have committed yet.
        Event::AbandonWatch(wd) => gate.park_late(wd).into_iter().for_each(|late| watch.abandon_wd(late)),
        Event::Op(m) => {
            #[cfg(test)]
            if matches!(&m, crate::backend::opsreq::OpMsg::RenameDone { .. }) { super::renametiming::mark("patch_begin"); }
            if let Some(session) = st.search_session.as_mut() {
                // An own write under the scope patches the index, and a due batch re-runs the standing query.
                session.row_cache.clear();
                let patched = session.patch_after_writes(&op_write_paths(&m));
                // A late or redone tag write changes names only: the cache swaps them, and the landing re-runs the query.
                session.note_tags_written(&op_tag_paths(&m));
                if patched && session.batch_due() {
                    let _ = loop_tx.send(Event::SearchRefresh);
                }
            }
            #[cfg(test)]
            if matches!(&m, crate::backend::opsreq::OpMsg::RenameDone { .. }) { super::renametiming::mark("patch_end"); }
            report_op(out, ops, m);
        }
        // A session walk landed; the standing query is re-run once when it is still the newest.
        Event::SearchWalked => {
            // A scan out with a session open is that session's folder answer, so the landing announces the walk.
            if !gate.holds() {
                on_search_walked(out, st, tb, pool);
            }
        }
        Event::SearchHeld => super::searchhold::on_search_held(out, st, tb, pool),
        Event::SearchDelta(token) if gate.holds() => gate.hold(Event::SearchDelta(token)),
        // A delta replayed from the listing gate enters the execution gate at once, so the requests replayed after it queue behind it.
        Event::SearchDelta(token) => route_delta(search_jobs, token, st, out, tb, pool, loop_tx),
        Event::SearchRefresh if gate.holds() => gate.hold(Event::SearchRefresh),
        Event::SearchRefresh => super::searchjobs::route_refresh(search_jobs, st, out, tb, pool, loop_tx),
        Event::SearchReady => {},
        Event::ReadError(e) => {
            st.nearby.close();
            // The framing cannot be trusted past a decode failure, so this reports at once and the reader reads no further.
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            // The requests read before the failure are sound, so a scan still out is answered first, as for Closed.
            if gate.holds() {
                gate.close();
                return Control::Continue;
            }
            return Control::Quit;
        }
        // A scan still out is answered before the process leaves, so no client loses its listing.
        Event::Closed => {
            st.nearby.close();
            if gate.holds() {
                gate.close();
                return Control::Continue;
            }
            return Control::Quit;
        }
    }
    Control::Continue
}

// One landing for the inline list and the answered scan: finish, then figures and the poll for a listing that landed.
#[allow(clippy::too_many_arguments)]
fn land<W: Write>(out: &mut W, st: &mut State, tb: &Tables, pool: &Pool, watch: &mut Watch, fsinfo: &mut FsInfo,
                  poller: &super::watchpoll::Poller, landed: listwork::Landed) {
    if let Some(scan) = landed.job.session {
        listwork::finish_session(out, st, tb, pool, watch, landed, scan);
        return;
    }
    // Fresh figures and the poll belong to a listing that landed, never one that failed.
    let path = landed.job.path.clone();
    if listwork::finish(out, st, tb, pool, watch, landed) {
        // After the rows, because a statfs beside gio's own listing slows it on the share.
        fsinfo.list_arrived(Path::new(&path), &st.listing);
        // Every open outline folder uses the same remote poll path as its root.
        poller.set(PathBuf::from(path));
        poller.set_children(st.listing.outline_open.iter().map(|name| st.base.join(name)));
    }
}

// The listed folder's change line, after an open search session patches that folder's own entries.
fn say_changed(out: &mut impl Write, st: &mut State) {
    if let Some(session) = st.search_session.as_mut() {
        session.row_cache.clear();
        let folder = session.folder.clone();
        session.patch_open_dir(&folder);
    }
    say(out, &changed_line(&st.base));
}

// One answer, written and flushed: the four read-only requests below differ only in what they say.
pub(crate) fn say(out: &mut impl Write, line: &str) {
    writeln!(out, "{}", line).ok();
    out.flush().ok();
}

// Listing-gate replays enter the same execution gate as requests read by the main loop.
#[allow(clippy::too_many_arguments)]
fn handle_with_jobs(line: &str, out: &mut impl Write, st: &mut State, tb: &Tables, pool: &Pool, cache: &Cache, ops: &mut Ops,
    flash: &mut flashslot::Ctx, watch: &mut Watch, peeks: &mut PeekWatch, fsinfo: &mut FsInfo,
    poller: &super::watchpoll::Poller, loop_tx: &Sender<Event>, gate: &mut Gate,
    search_jobs: Option<&mut super::searchjobs::Gate>) -> Control {
    if let Some(jobs) = search_jobs {
        let mut buffered = VecDeque::new();
        if jobs.route(Event::Request(line.to_string()), st, out, tb, pool, loop_tx, &mut buffered, gate.holds()).is_none() {
            return Control::Continue;
        }
    }
    handle_line(line, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate)
}

fn handle_line(
    line: &str,
    out: &mut impl Write,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    cache: &Cache,
    ops: &mut Ops,
    flash: &mut flashslot::Ctx,
    watch: &mut Watch,
    peeks: &mut PeekWatch,
    fsinfo: &mut FsInfo,
    poller: &super::watchpoll::Poller,
    loop_tx: &Sender<Event>,
    gate: &mut Gate,
) -> Control {
    // Rows read from a numbering this listing has already replaced name other files, so they are refused.
    if let Some(refused) = super::rowguard::refusal(line, st.generation) {
        say(out, &refused);
        return Control::Continue;
    }
    let request = parse_request(line);
    if super::contentsreq::intercept(line, &request, out, st, tb, pool) {
        return Control::Continue;
    }
    // A session answer adopts only its first window, so rows past the head join the listing before any arm reads them.
    extend_search_listing(st, rows_needed(&request));
    match request {
        Request::Nearby { op, id } => say(out, &st.nearby.request(&op, id, loop_tx.clone())),
        Request::NetworkAddress { address, id } => say(out, &super::networkaddress::answer(id, &address)),
        Request::Git { line } => st
            .git
            .get_or_insert_with(|| super::gitreq::Worker::new(loop_tx.clone()))
            .request(line),
        Request::Plugins { line } => super::plugins::request(line, ops.tx.clone()),
        Request::Permissions { line } => say(out, &ops.permissions.handle(&line)),
        Request::Picker { line } => {
            let replies = ops.tx.clone();
            ops.picker.get_or_insert_with(|| super::picker::Picker::new(replies)).request(line);
        }
        Request::MenuAction { line, rows } => {
            if matches!(crate::json::field_str(&line, "op").as_deref(), Some("tagstates" | "settags")) {
                let changed = super::tagsreq::menu(out, ops, st, &line);
                note_tag_writes(st, &changed);
                return Control::Continue;
            }
            let paths = resolve_rows(crate::json::field_str_array(&line, "paths"), &rows, &st.base, &st.listing);
            let cursor = crate::json::field_str(&line, "cursorPath").or_else(|| crate::json::field_usize(&line, "cursor").map(|index|
                resolve_rows(Vec::new(), &[index], &st.base, &st.listing).into_iter().next().unwrap_or_default()));
            super::opsdispatch::request_menu_action(out, ops, line, paths, cursor);
        }
        // Directive 71: a CLI run of a second or more, so it answers on its own thread.
        Request::Rclone { line } => super::rclone::request(out, ops, &line),
        Request::Jobs { line } => {
            let reached = super::jobevents::request(out, ops, &line);
            patch_session_paths(st, reached.dirs);
            note_tag_writes(st, &reached.tags);
        }
        Request::Actions { line } => {
            super::actions::request(line, Arc::clone(&tb.mime), ops.tx.clone())
        }
        Request::LocalSend {
            op,
            peer,
            paths,
            id,
        } => {
            if op == "send" {
                let job = ops.claim_id();
                ops.jobs.submit(
                    out,
                    job,
                    super::jobwork::Spec { kind: "send", label: "Sending", target: None },
                    super::jobwork::Work::Send {
                        peer,
                        paths,
                        request: id,
                        selection: None,
                        airdrop: false,
                    },
                    &ops.detached,
                    &ops.tx,
                );
            } else {
                super::localsend::request(op, peer, paths, id, ops.tx.clone());
            }
        }
        Request::TrashBrowse { line } => {
            super::trashbrowse::request(out, ops, line);
        }
        // An AirDrop send is a job like LocalSend's, so it shows in the jobs list with its progress and its Cancel.
        Request::AirDrop { peer, paths, id } => {
            let job = ops.claim_id();
            let (spec, work) = (
                super::jobwork::Spec { kind: "send", label: "Sending", target: None },
                super::jobwork::Work::Send { peer, paths, request: id, selection: None, airdrop: true },
            );
            ops.jobs.submit(out, job, spec, work, &ops.detached, &ops.tx);
        }
        Request::Outline { rows, anchor, open, enabled, first } => {
            if let Some(enabled) = enabled.filter(|_| st.held != Held::List) {
                st.outline.disabled = !enabled;
                return Control::Continue;
            }
            match super::outline::request(st, &rows, anchor, open, enabled, first) {
                Ok(line) => return handle_line(&line, out, st, tb, pool, cache, ops, flash, watch, peeks, fsinfo, poller, loop_tx, gate),
                Err(msg) => say(out, &error_line(&FleaError { where_: "outline".into(), path: st.base.to_string_lossy().into_owned(), msg })),
            }
        }
        // A local list lands inline here, a remote one as Event::List; both land through finish.
        Request::List { path, first, hidden, want_changed, keep } => {
            let tabs = keep.into_iter().map(PathBuf::from).collect();
            st.settled.keep(tabs);
            #[cfg(test)]
            super::renametiming::mark("dispatch_list");
            // The session goes first; start then ends the walk and forgets its rows before the scan.
            close_search_session(st);
            let line = st.outline.list(line);
            let job = listwork::Job { path, first, line, hidden, want_changed, session: None };
            if let listwork::Started::Inline(landed) = listwork::start(out, st, pool, watch, gate, job, Arc::clone(&tb.mime)) {
                land(out, st, tb, pool, watch, fsinfo, poller, *landed);
            }
            // Desktop priors load on their own worker after the first listing, so an empty window starts no thread.
            super::searchprior::start();
            #[cfg(test)]
            super::renametiming::mark("list_complete");
        }
        // A set of named paths is not a directory, so the watch stops rather than following its base.
        Request::ListPaths { paths, first } => {
            close_search_session(st);
            watch.stop();
            poller.clear();
            listpaths::answer(out, st, pool, tb, &paths, first, line)
        }
        Request::Filter { query, id } => {
            let line = super::findnames::requested(&st.listing, &query, id, line);
            say(out, &super::rowguard::stamped(line, st.generation));
        }
        Request::Window { start, count } => {
            let start = st.window_start(start);
            if count == 0 {
                say(out, &super::rowguard::stamped(super::rows::empty_line(start, 0.0), st.generation));
                return Control::Continue;
            }
            match window_metas(st, start, count) {
                Ok((metas, ms)) => {
                    let line = window_line(st, tb, &metas, start, ms);
                    writeln!(out, "{}", super::rowguard::stamped(line, st.generation)).ok();
                }
                Err(e) => {
                    writeln!(out, "{}", error_line(&e)).ok();
                }
            }
            out.flush().ok();
        }
        Request::Search { path, query, hidden } => {
            close_search_session(st);
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            st.base = PathBuf::from(&path);
            st.listing = Listing::new();
            // A walk holds no directory listing, so neither re-read counts over it.
            st.held = Held::Walk;
            // A walk's matches are not a directory either, so nothing is watched until list asks again.
            watch.stop();
            poller.clear();
            forget_rows(st, pool);
            // The client is told at once that its old rows are gone, then the count grows as matches arrive.
            match search_root_info(&st.base) {
                // A root that answers nothing starts no walk, so no terminal line can strand it.
                Err(e) => { writeln!(out, "{}", error_line(&e)).ok(); }
                Ok((dev, writable)) => {
                    writeln!(out, "{}", listed_line(0, 0.0, 0.0, dev, &st.base.to_string_lossy(), writable)).ok();
                    st.search = Some(Search::new(&path, &query, hidden));
                    st.search_reported = Instant::now();
                }
            }
            out.flush().ok();
        }
        Request::SearchContents { line } => super::contentsreq::start(&line, out, st, loop_tx),
        Request::SearchCancel => {
            if st.search_session.is_some() {
                // The walk stops, the index stays and the standing query re-answers once; with no walk, silence.
                if let Some(session) = st.search_session.as_mut() {
                    if let Some(cancelled) = session.cancel_walk() {
                        match cancelled.answer {
                            Some(answer) => super::searchreq::emit_session_answer_cancelled(
                                out, st, tb, pool, &answer, true,
                            ),
                            None => {
                                writeln!(
                                    out,
                                    "{}",
                                    super::searchreq::searched_cancelled_line(
                                        0, cancelled.indexed, 0.0, cancelled.gen
                                    )
                                )
                                .ok();
                            }
                        }
                        out.flush().ok();
                    }
                }
            } else {
                close_search_session(st);
                if finish_search(out, st, true) {
                    forget_rows(st, pool);
                }
            }
        }
        Request::SearchOpen { path, hidden, folder } => {
            // Opening a session cancels today's walk; the listing it was filling stays as it stands.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            close_search_session(st);
            let folder = if folder.is_empty() { path.clone() } else { folder };
            st.search_session = Some(Session::open_async(&path, &folder, hidden, loop_tx.clone()));
            say(out, &searchopened_line(&path));
            out.flush().ok();
        }
        Request::SearchQuery { query, gen, count, sizes } => {
            let count = count.unwrap_or(FIRST_WINDOW_DEFAULT);
            if let Some(session) = st.search_session.as_mut() {
                session.wish_sizes(sizes);
            }
            if query.is_empty() {
                // The folder answers itself: one readdir in its own sort, the session and its index kept; a remote one scans off the loop.
                let claimed = st.search_session.as_mut().is_some_and(|session| session.claim_gen(gen, &query, count));
                if claimed {
                    let folder = st.search_session.as_ref().map(|session| session.folder.clone()).unwrap_or_default();
                    let hidden = st.search_session.as_ref().is_some_and(|session| session.hidden);
                    let walking = st.search_session.as_ref().is_some_and(|session| session.walking());
                    let session = Some(listwork::ForSession { gen, walking });
                    let job = listwork::Job { path: folder.to_string_lossy().to_string(), first: count, line: line.to_string(), hidden, want_changed: false, session };
                    if let listwork::Started::Inline(landed) = listwork::start(out, st, pool, watch, gate, job, Arc::clone(&tb.mime)) {
                        land(out, st, tb, pool, watch, fsinfo, poller, *landed);
                    }
                }
            } else if let Some(session) = st.search_session.as_mut() {
                if let Some(answer) = session.ask(gen, &query, count, loop_tx) {
                    emit_session_answer(out, st, tb, pool, &answer);
                }
            }
            out.flush().ok();
        }
        Request::SearchClose => {
            if let Some(session) = st.search_session.take() {
                let scope = session.scope.clone();
                let _ = session.close();
                say(out, &searchclosed_line(&scope.to_string_lossy()));
                out.flush().ok();
            }
        }
        Request::Sort { by, desc: _, anchor } => {
            st.outline.sort(line);
            close_search_session(st);
            // The walk owns the listing sort would reorder, so it ends first rather than racing it.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
            let base = st.base.clone();
            let listing = st.listing.clone();
            let mime = std::sync::Arc::clone(&tb.mime);
            let line_owned = line.to_string();
            // A key that names no order is refused by name, so a client's sort mark can only describe the order it got.
            match super::iomount::call_bulk(&base.clone(), &body, "sort", move || {
                let mut l = listing;
                match super::ordering::request(&mut l, &base, &mime, &line_owned) {
                    Err(msg) => Err(msg.to_string()),
                    Ok((pass_ms, sort_ms, sized)) => {
                        let dev = super::fsinfo::dev_of(&base);
                        let writable = super::ops::dir_writable(&base);
                        Ok((l, pass_ms, sort_ms, sized, dev, writable))
                    }
                }
            }) {
                Err(e) => {
                    writeln!(out, "{}", error_line(&e)).ok();
                }
                Ok(Err(msg)) => {
                    let e = FleaError { where_: "sort".to_string(), path: by.clone(), msg };
                    writeln!(out, "{}", error_line(&e)).ok();
                }
                Ok(Ok((l, pass_ms, sort_ms, sized, dev, writable))) => {
                    st.listing = l;
                    forget_rows(st, pool);
                    // After forget_rows, which clears the very map this seeds.
                    seed_answered(st, &sized);
                    let line = match anchor.as_deref() {
                        // The listing is a snapshot, so only a path it never held answers -1.
                        Some(anchor) => {
                            let base = listed_line(st.listing.len(), pass_ms, sort_ms, dev,
                                &st.base.to_string_lossy(), writable);
                            with_anchor(&base, anchor,
                                st.listing.index_of(&st.base, Path::new(anchor)).map(|index| index as isize).unwrap_or(-1))
                        }
                        None => listed_line(st.listing.len(), pass_ms, sort_ms, dev, &st.base.to_string_lossy(), writable),
                    };
                    writeln!(out, "{}", line).ok();
                }
            }
            out.flush().ok();
        }
        Request::Thumb { rows, cache_only, identity } => {
            thumb_rows(out, &rows, st, tb, pool, cache, super::thumbreq::identity::Options { cache_only, identity });
            out.flush().ok();
        }
        Request::ThumbCancel { rows } => {
            if rows.is_empty() {
                cancel_all(st, pool);
            } else {
                for row in &rows {
                    cancel_row(out, st, pool, *row);
                }
            }
        }
        Request::TrashThumb { line } => {
            super::trashthumb::request(out, st, tb, pool, cache, &line);
            out.flush().ok();
        }
        Request::TrashThumbCancel { ids } => {
            super::trashthumb::cancel(out, st, pool, &ids);
        }
        Request::DirSize { rows } => {
            queue_dirsizes(out, st, &rows);
        }
        // No rows form: a stale row from a scrolled-past viewport would delay the rows the new one wants, see docs/protocol.md "dirsizecancel".
        Request::DirSizeCancel => {
            st.dirsize_queue.clear();
            st.dirsize_worker.cancel();
        }
        Request::Transfer { op, paths, rows, dest, menu_id, shelf, collide } => {
            if !shelf.is_empty() {
                crate::backend::shelfdrop::start(out, ops, &shelf, &dest, collide)
            } else if menu_id != 0 {
                start_menu_transfer(out, ops, &op, menu_id, &dest, collide)
            } else {
                let named = resolve_rows(paths, &rows, &st.base, &st.listing);
                start_transfer(out, ops, &op, named, &dest, collide)
            }
        }
        Request::Collisions { id, paths, rows, dest, menu_id, merge: true, .. } => super::merge::ask_beside(ops, id, menu_id, resolve_rows(paths, &rows, &st.base, &st.listing), &dest, &tb.mime, &tb.icons),
        Request::Collisions { id, paths, rows, dest, menu_id, fat, .. } => super::collide::ask_beside(ops, id, menu_id, resolve_rows(paths, &rows, &st.base, &st.listing), &dest, fat, (&tb.mime, &tb.icons)),
        Request::PasteImage(ask) => super::pasteimage::start(out, ops, &ask),
        Request::ClipSet { op, paths } => super::clipreq::request_set(ops.tx.clone(), op, paths),
        Request::ClipGet => super::clipreq::request_get(ops.tx.clone()),
        Request::ClipClear { token, cut } => super::clipreq::request_clear(ops.tx.clone(), token, cut),
        Request::ClipWatch => super::clipreq::request_watch(ops.tx.clone(), &mut st.clip_watching, None),
        Request::Link { held, op, paths, rows, dest, collide } => {
            let named = resolve_rows(paths, &rows, &st.base, &st.listing);
            super::linkheld::start(out, ops, held, &op, named, &dest, collide)
        }
        Request::LinkTarget { path, id, expected } => super::linkheld::target(ops, &path, id, expected),
        Request::PermissionsBatch { paths, modes, id } => do_permissions_batch(out, ops, paths, modes, id),
        Request::TransferCancel { id } => cancel_transfer(out, ops, id),
        Request::Trash { paths, rows, menu_id, request } => {
            let named = resolve_rows(paths, &rows, &st.base, &st.listing);
            start_trash(out, ops, named, menu_id, request)
        }
        Request::Rename { path, to, menu_id } => {
            #[cfg(test)]
            super::renametiming::begin();
            // The worker result patches the search index using the rename's owned paths.
            let touched = if menu_id == 0 { do_rename(out, ops, &path, &to) }
                else { super::opsdispatch::do_menu_rename(out, ops, &path, &to, menu_id) };
            patch_session_paths(st, touched);
        }
        Request::BulkRename { paths, names } => patch_session_paths(st, super::bulkrename::do_bulkrename(out, ops, paths, names)),
        Request::MkDir { path, name, default_base } => {
            let request = crate::json::field_usize(line, "request").unwrap_or(0);
            let default_base = default_base.as_deref();
            let touched = if request == 0 { do_mkdir(out, ops, &path, &name, default_base) }
                else { super::opsdispatch::do_mkdir_requested(out, ops, &path, &name, default_base, request) };
            patch_session_paths(st, touched);
        }
        Request::NewFile { path, name, id } => patch_session_paths(st, do_newfile(out, ops, &path, &name, id)),
        Request::Duplicate { path, menu_id } => {
            let request = crate::json::field_usize(line, "request").unwrap_or(0);
            if request == 0 { start_duplicate(out, ops, &path, menu_id) }
            else { super::opsdispatch::created_request::duplicate(out, ops, &path, menu_id, request) }
        }
        Request::Undo => {
            // An undo is an own write: its reversal patches the index with no walk.
            let reached = do_undo(out, ops);
            patch_session_paths(st, reached.dirs);
            note_tag_writes(st, &reached.tags);
        }
        Request::Redo => start_redo(out, ops),
        // Never touches st.listing, which is the whole point: a column is not the pane's own listing.
        Request::Peek { path, first, details, hidden, hidden_last, focus, watch, keep, by, desc } => {
            if watch && !keep.is_empty() { peeks.keep(keep.into_iter().map(PathBuf::from).collect()) }
            let order = match super::peek::Order::wire(hidden_last, by.as_deref(), desc) {
                Ok(order) => order,
                // A key this wire never defined answers the sort's own error, never a name-order column.
                Err(msg) => { say(out, &error_line(&FleaError { where_: "sort".to_string(), path: by.unwrap_or_default(), msg: msg.to_string() })); return Control::Continue }
            };
            let line = super::peek::answer(&path, (first, details), hidden, order, focus, watch.then(|| (peeks.raw_fd(), loop_tx.clone())), tb);
            say(out, &line)
        }
        // A compress names absolute paths and no path; an extract names the one archive in path.
        // The password leaves this wire only on the tool's own stdin pipe, never in argv or a log.
        Request::Archive { op, paths, path, dest, format, menu_id, password, encrypt_names } => start_archive(
            out, ops, Arc::clone(&tb.formats), &op,
            paths, format, PathBuf::from(&path), PathBuf::from(&dest), menu_id, password, encrypt_names),
        Request::Convert { path, dest, strip, menu_id, request_id, check } =>
            start_convert(out, ops, PathBuf::from(&path), PathBuf::from(&dest), strip, menu_id, request_id, check),
        Request::PrintConvert(ask) => super::printreq::serve(out, ops, ask, Arc::clone(&tb.mime), Arc::clone(&tb.aliases)),
        Request::Formats { id } => {
            let mut line = formats_line(&tb.formats, convert::available());
            line.insert_str(line.len() - 1, &format!(r#", "id":{},"providers":{}"#, id, super::providers::facts()));
            say(out, &line);
        }
        // The class rides beside the figures once per directory change; a slow mount's fresh figures follow as a second line.
        Request::FsInfo => {
            let (info, class, system) = fsinfo.answer(&st.base);
            let tagable = fsinfo.tagable(&st.base, class);
            say(out, &fsinfo_line(&info, &st.base.to_string_lossy(), class, tagable, system));
        }
        Request::PdfCopy { id, slot, path } => {
            super::pdfcopy::spawn(id, slot, PathBuf::from(&path), ops.tx.clone());
        }
        Request::OfficePdf { id, path, sheets } => super::officepdf::serve(out, ops, &tb.mime, &tb.aliases, id, &path, sheets),
        Request::FontLook(ask) => super::fontinfo::render::spawn(ask, Arc::clone(&tb.mime), Arc::clone(&tb.aliases), ops.tx.clone()),
        // One row, only when a client asked: the same no-sweep rule thumb and dirsize already follow.
        Request::Owner { row, token } => {
            let path = (row < st.listing.len()).then(|| st.base.join(st.listing.name(row)));
            super::owner::spawn(row, path, token, ops.tx.clone());
        }
        Request::PreviewMeta { line } => super::previewmeta::spawn(&line, Arc::clone(&tb.formats), ops.tx.clone()),
        Request::Meta { row, text, media, archive, token } => {
            if row < st.listing.len() {
                let want = if archive { Some(Arc::clone(&tb.formats)) } else { None };
                spawn_meta(row, st.base.join(st.listing.name(row)), text, media, want, token, ops.tx.clone())
            }
        }
        // The Make executable row's own probe, answered on a thread: an open on a hung mount never returns.
        Request::Shebang { path, id } => {
            super::shebang::spawn(PathBuf::from(&path), id, ops.tx.clone())
        }
        // Answered on a thread, since bat runs in a jail and a read on a hung mount never returns.
        Request::Code(ask) => super::codereq::spawn(ask, ops.tx.clone()),
        Request::Html(ask) => super::htmlreq::spawn(ask, ops.tx.clone()),
        Request::Model3d(ask) => super::model3dreq::spawn(ask, ops.tx.clone()),
        // Answered on a thread: diff runs in a jail and a read on a hung mount never returns.
        Request::Compare(ask) => super::compare::spawn(ask, ops.tx.clone()),
        tags @ (Request::SetTags { .. } | Request::TagStates { .. }) => {
            let written = super::tagsreq::handle(out, ops, st, tags);
            note_tag_writes(st, &written);
        }
        Request::Paths { rows } => {
            let paths = resolve_rows(Vec::new(), &rows, &st.base, &st.listing);
            if crate::json::field_bool(line, "hold") {
                super::linkheld::request(out, ops.tx.clone(), paths, st.generation, true, crate::json::field_usize(line, "id").unwrap_or(0));
            } else {
                say(out, &paths_line(&paths));
            }
        }
        Request::ImagePaths(ask) => say(
            out,
            &paths_line(&super::viewerpaths::ordered(
                &st.base,
                &st.listing,
                &tb.mime,
                &ask,
            )),
        ),
        Request::Locate { path } => {
            let index = st.listing.index_of(&st.base, Path::new(&path));
            say(out, &super::proto::located_line(&st.base.to_string_lossy(), &path, index));
        }
        Request::LocateMany { paths, id, menu_id, transfer_id } => {
            let mut matches = st.listing.indices_of(&st.base, &paths);
            let error = if transfer_id > 0 {
                if ops.transfer_retry.0 != transfer_id {
                    Some("The items changed; select them again.".to_string())
                } else {
                    super::opsreq::retain_retry(&ops.transfer_retry.1, &mut matches);
                    None
                }
            } else if menu_id == 0 { None } else {
                ops.menuactions.as_ref().ok_or_else(|| "The items changed; select them again.".to_string())
                    .and_then(|menu| menu.retain_survivors(menu_id, &mut matches)).err()
            };
            if error.is_some() { matches.clear(); }
            say(out, &super::proto::located_many_line(&st.base.to_string_lossy(), id, transfer_id, &matches, error.as_deref()));
        }
        Request::Jump { id, ranking, favourites, recent } => super::jump::request(id, ranking, favourites, recent, ops.tx.clone()),
        Request::Devs { paths, id } => super::devsreq::request(paths, id, ops.tx.clone()),
        Request::Flash(req) => flashop::request(out, flash, req),
        Request::Quit => return Control::Quit,
        // corner: an unrecognised line is answered with silence, see AGENTS.md.
        Request::Unknown => {}
    }
    super::renamework::flush_pending(ops);
    ops.jobs.refresh_undo(out, &ops.journal);
    Control::Continue
}

// An own write's landed paths patch the session index; out-of-scope paths patch nothing.
fn patch_session_paths(st: &mut State, touched: Vec<PathBuf>) {
    if let Some(session) = st.search_session.as_mut() {
        session.patch_after_writes(&touched);
    }
}

// An own tag write's files: an open tag pass re-reads exactly their names and the standing query is re-run once they are in.
fn note_tag_writes(st: &mut State, written: &[PathBuf]) {
    if let Some(session) = st.search_session.as_mut() {
        session.note_tags_written(written);
    }
}

#[path = "run_rows.rs"]
mod row_needs;
use row_needs::rows_needed;

// Appends held rest rows in the answer's order and never renumbers; true when rows joined, so a miss can retry.
fn extend_search_listing(st: &mut State, end: usize) -> bool {
    let adopted = st.listing.len();
    if adopted >= end {
        return false;
    }
    match st.search_session.as_mut().and_then(|session| session.take_rest_chunk(end, adopted)) {
        Some(rows) if !rows.is_empty() => {
            for (path, is_dir) in &rows {
                st.listing.push(path, *is_dir);
            }
            true
        }
        _ => false,
    }
}

// A navigation drops the session and frees its arena; Esc reaches this through searchclose.
pub(crate) fn close_search_session(st: &mut State) {
    if let Some(session) = st.search_session.take() {
        let _ = session.close();
    }
}

// A session walk landed; a held query answers from the whole index, else the standing query is re-run once when it is still the newest.
fn on_search_walked(out: &mut impl Write, st: &mut State, tb: &Tables, pool: &Pool) {
    if let Some(session) = st.search_session.as_mut() {
        if session.poll_walked() {
            if let Some(answer) = session.answer_held().or_else(|| session.re_answer_standing()) {
                emit_session_answer(out, st, tb, pool, &answer);
            } else if let Some((gen, n, scanned, ms)) = session.landing_empty() {
                writeln!(out, "{}", super::searchreq::searched_status_line(n, scanned, ms, gen, false, session.partial_reason())).ok();
                out.flush().ok();
            }
        }
    }
}

// A new row order invalidates every outstanding index, so the queue goes and no result can be reported against the new listing.
pub fn forget_rows(st: &mut State, pool: &Pool) {
    st.contents.clear();
    st.generation += 1;
    cancel_all(st, pool);
    st.asked.clear();
    st.thumb_mtimes.clear();
    st.thumb_replies.clear();
    st.window_meta.clear();
    // A list or a sort changes which row an index names, the same reason thumbnails clear their map.
    st.dirsizes.clear();
    st.dirsize_queue.clear();
    st.dirsize_worker.cancel();
}

// A worker-built listing lands without a syscall, carrying its own dev and writability.
pub(crate) fn adopt_listed(out: &mut impl Write, st: &mut State, pool: &Pool, tb: &Tables, path: &str, done: super::iomount::ListOut, want_changed: bool) {
    adopt_listed_as(out, st, pool, tb, path, done, Adopt::Listing { want_changed });
}

// A session's empty query lands its folder listing stamped with the gen it claimed.
pub(crate) fn adopt_session(out: &mut impl Write, st: &mut State, pool: &Pool, tb: &Tables, path: &str, done: super::iomount::ListOut, gen: usize) {
    adopt_listed_as(out, st, pool, tb, path, done, Adopt::Session(gen));
}

// A plain listing may count its changed rows; a session's empty query stamps its gen instead, so the client drops a stale folder answer.
enum Adopt { Listing { want_changed: bool }, Session(usize) }

fn adopt_listed_as(out: &mut impl Write, st: &mut State, pool: &Pool, tb: &Tables, path: &str, done: super::iomount::ListOut, how: Adopt) {
    let (want_changed, gen) = match how { Adopt::Listing { want_changed } => (want_changed, None), Adopt::Session(gen) => (false, Some(gen)) };
    // A same-path re-list names added plus removed rows, so a rename counts 2 against a net delta of 0.
    let same = st.held == Held::List && Path::new(path) == st.base.as_path();
    let changed = if same && want_changed { crate::backend::listing::changed_count(&st.listing, &done.listing) } else { 0 };
    st.base = PathBuf::from(path);
    st.settled.listed(&st.base);
    st.listing = done.listing;
    // A list holds a directory now, so only its own re-read counts.
    st.held = Held::List;
    forget_rows(st, pool);
    seed_answered(st, &done.sized);
    if gen.is_some() {
        super::answersizes::fill(st, done.first_start, done.first_metas.len());
    }
    let base = st.base.to_string_lossy().to_string();
    let listed = match gen {
        Some(gen) => super::proto::listed_gen_line(st.listing.len(), done.read_ms, done.sort_ms, done.dev, &base, done.writable, gen),
        None => super::proto::say_listed(st.listing.len(), done.read_ms, done.sort_ms, done.dev, &base, done.writable),
    };
    let listed = if same && want_changed { crate::backend::proto::with_changed(&listed, changed) } else { listed };
    writeln!(out, "{}", listed).ok();
    let start = st.window_start(done.first_start);
    let line = tb.rows_line(&st.base, &st.listing, &done.first_metas, start, done.first_ms);
    let line = match gen {
        Some(gen) => super::answersizes::with_sizes(super::rowguard::stamped_gen(line, st.generation, gen), &st.dirsizes, start..start + done.first_metas.len()),
        None => super::rowguard::stamped(line, st.generation),
    };
    writeln!(out, "{}", line).ok();
}

// Search advances only when the request channel is idle.
fn tick_walkers(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool) {
    // A finished walk hands back its rows in ranked order, which renames every outstanding index.
    if step_search(out, st) {
        forget_rows(st, pool);
    }
}

// A thread stuck in a dead mount's call holds the exit and the pipes with it, so the client is let go of first.
pub fn let_go_of_client() {
    use std::os::unix::io::AsRawFd;
    extern "C" {
        fn dup2(old: i32, new: i32) -> i32;
    }
    let _ = io::stdout().flush();
    if let Ok(null) = std::fs::OpenOptions::new().read(true).write(true).open("/dev/null") {
        for fd in [0, 1] {
            unsafe { dup2(null.as_raw_fd(), fd) };
        }
    }
}

// A worker inside a child owns a temp file in the shared cache that only its own return publishes or removes; see AGENTS.md "Thumbnail requests".
pub(crate) fn drain(
    out: &mut impl Write,
    st: &mut State,
    ops: &mut Ops,
    flash: &mut flashslot::Ctx,
    rx: &Receiver<Event>,
    pool: &Pool,
    cache: &Cache,
) {
    let deadline = Instant::now() + DRAIN_LIMIT;
    // A clean shutdown cancels the slot operation and each detached job, so a cancelled copy removes its partial destination and each Work cleanup runs.
    if let Some(id) = ops.live.running() {
        ops.live.cancel(id);
    }
    ops.detached.cancel_all();
    // A closing window cancels the flash and waits for its flashdone; the worker wipes first unless verify keeps the proven image.
    if let Some(id) = flash.slot.running() {
        flash.slot.cancel(id);
    }
    // A trash batch past its announce has freed the slot but still owes its journal entry, so the drain waits for it too; a journal no request started owes none.
    while st.outstanding > 0 || ops.live.running().is_some() || !ops.pending.is_empty() || ops.journal.get().is_some_and(|journal| journal.deferred.is_some()) || !ops.settling.is_empty() || !ops.detached.is_empty() || flash.slot.running().is_some() {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Thumb(d)) => report_done(out, st, pool, d),
            Ok(Event::Op(m)) => report_op(out, ops, m),
            Ok(Event::Flash(m)) => flash.report(out, &m),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    if let Some(journal) = ops.journal.get_mut() {
        journal.release_held();
    }
    let thumb_replies: Vec<_> = st.thumb_replies.iter().filter_map(|weak| weak.upgrade()).collect();
    pool.cancel_all();
    // The queue is empty now, so no worker can start a new job and the temps still on disk are exactly the abandoned ones.
    if st.outstanding > 0 {
        sweep_own_temps(&cache.large_dir());
    }
    // corner: a row the deadline cut short is answered empty rather than left unanswered, see AGENTS.md "Thumbnail requests".
    for (_, row) in std::mem::take(&mut st.asked) {
        writeln!(out, "{}", thumbed_line(row, "", 0.0)).ok();
    }
    st.thumb_replies.clear();
    for shared in thumb_replies {
        super::thumbreq::identity::complete(out, st, &Some(shared), "", 0.0);
    }
    st.thumb_mtimes.clear();
    super::trashthumb::drain(out, st);
    out.flush().ok();
}

// The walk root's figures, statted through the bound so a dead server answers instead of hanging.
pub(crate) fn search_root_info(base: &Path) -> Result<(u64, bool), FleaError> {
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let owned = base.to_path_buf();
    let key = owned.clone();
    super::iomount::call(&key, &body, "search", move || (super::fsinfo::dev_of(&owned), super::ops::dir_writable(&owned)))
}

#[cfg(test)]
#[path = "run_tags_tests.rs"]
mod tags_tests;

#[cfg(test)]
#[path = "run_empty_tests.rs"]
mod empty_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    const REPLY_DEADLINE: Duration = Duration::from_secs(5);
    const THUMB_TEST_WORKERS: usize = 1;

    #[test]
    fn a_remote_window_error_names_window_and_the_exact_listed_path() {
        const REMOTE_PATH: &str = "/run/user/flea-window-test/gvfs/smb-share:server=test,share=wire/folder \"café\"\\name/../";
        const NO_WATCH: i32 = -1;
        let tb = Tables::load();
        let (events, _events_rx) = crate::backend::events::channel();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        let (results, _done) = channel();
        let cache_root = std::env::temp_dir().join("flea-window-error-cache");
        let pool = Pool::new(THUMB_TEST_WORKERS, results, cache_root.clone(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let cache = Cache::at(cache_root);
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let (flash_tx, _flash_rx) = channel();
        let mut flash = flashslot::Ctx::new(flash_tx);
        let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
        let mut fsinfo = FsInfo::new(events.clone());
        let poller = super::super::watchpoll::Poller::new(events.clone());
        let done = super::super::iomount::ListOut {
            listing: Listing::new(),
            read_ms: 0.0,
            sort_ms: 0.0,
            sized: Vec::new(),
            dev: 0,
            writable: true,
            first_metas: Vec::new(),
            first_start: 0,
            first_ms: 0.0,
            watch_wd: NO_WATCH,
            watch_mount: PathBuf::new(),
        };
        let mut out = Vec::new();
        adopt_listed(&mut out, &mut st, &pool, &tb, REMOTE_PATH, done, false);
        let listed = String::from_utf8(std::mem::take(&mut out)).expect("the listing wire is UTF-8");
        let header = listed.lines().next().expect("the listing has a header");
        assert_eq!(crate::json::field_str(header, "t").as_deref(), Some("listed"));
        let listed_path = crate::json::field_str(header, "path").expect("the header names its directory");
        assert_eq!(listed_path.as_bytes(), REMOTE_PATH.as_bytes(), "the listing preserves the input spelling");
        assert!(super::super::iomount::is_remote(&st.base, ""), "the fixture takes the remote window branch");
        let mount = super::super::iomount::mount_key(&st.base, "");
        let _stuck = super::super::iomount::test_hold_stuck(mount);
        let calls_before = super::super::iomount::test_calls();
        assert!(handle_line(r#"{"c":"window","start":0,"count":1}"#, &mut out,
            &mut st, &tb, &pool, &cache, &mut ops, &mut flash, &mut watch, &mut peeks, &mut fsinfo, &poller,
            &events, &mut Gate::new(events.clone())) == Control::Continue);
        assert!(super::super::iomount::test_calls() > calls_before, "the window reaches the real mount bound");
        let response = String::from_utf8(out).expect("the error wire is UTF-8");
        let mut lines = response.lines();
        let error = lines.next().expect("the refused window emits an error");
        assert_eq!(crate::json::field_str(error, "t").as_deref(), Some("error"), "{error}");
        assert_eq!(crate::json::field_str(error, "where").as_deref(), Some("window"), "the window emitter identifies its operation: {error}");
        let error_path = crate::json::field_str(error, "path").expect("the error names its directory");
        assert_eq!(error_path.as_bytes(), listed_path.as_bytes(), "the window error path must be byte-equal to the listed path; error={error_path:?}, listed={listed_path:?}");
        assert!(crate::json::field_str(error, "msg").expect("the error carries its cause").contains("not responding"));
        assert!(lines.next().is_none(), "a refused window emits no rows: {response}");
    }

    #[test]
    fn a_held_link_keeps_requests_responsive_and_holds_the_operation_slot() {
        let d = TestDir::new("heldlink");
        let src = d.dir("src");
        let a = d.file("src/a.txt", "a");
        let b = d.file("src/b.txt", "b");
        let dest = d.dir("dest");
        let (release, entered) = super::super::opsdispatch::test_hold_link(&dest);
        let (answered, answers) = channel();
        let dest_path = dest.clone();
        let cache_root = d.join("cache");
        let backend = std::thread::spawn(move || {
            let tb = Tables::load();
            let (events, _events_rx) = crate::backend::events::channel();
            let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
            st.base = src;
            st.listing.push("a.txt", false);
            st.listing.push("b.txt", false);
            let (results, _done) = channel();
            let pool = Pool::new(THUMB_TEST_WORKERS, results, cache_root.clone(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
            let cache = Cache::at(cache_root);
            let (tx, rx) = channel();
            let mut ops = Ops::new(tx);
            let (flash_tx, _flash_rx) = channel();
            let mut flash = flashslot::Ctx::new(flash_tx);
            let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
            let mut fsinfo = FsInfo::new(events.clone());
            let poller = super::super::watchpoll::Poller::new(events.clone());
            let link = format!(r#"{{"c":"link","rows":[0,1],"dest":"{}","op":"relative"}}"#, crate::json::escape(&dest_path.to_string_lossy()));
            let mut out = Vec::new();
            for line in [&link, r#"{"c":"paths","rows":[0,1]}"#, &link] {
                assert!(handle_line(line, &mut out, &mut st, &tb, &pool, &cache, &mut ops, &mut flash,
                    &mut watch, &mut peeks, &mut fsinfo, &poller, &events, &mut Gate::new(events.clone())) == Control::Continue);
            }
            answered.send((std::mem::take(&mut out), ops.live.running().is_some(), ops.journal.is_empty(), ops.pending.is_empty())).unwrap();
            let landed = rx.recv_timeout(REPLY_DEADLINE).expect("the released link reports through the op channel");
            assert!(matches!(landed, OpMsg::Linked { .. }), "the production link sends Linked");
            report_op(&mut out, &mut ops, landed);
            assert_eq!(String::from_utf8_lossy(&out).trim(), r#"{"t":"linked","ok":2,"failed":0,"skipped":0}"#);
            assert_eq!(ops.journal.len(), 1, "one journal entry covers both links");
            assert!(ops.live.running().is_none(), "Linked releases the operation slot");
            super::super::opsdispatch::do_undo(&mut out, &mut ops);
            assert!(String::from_utf8_lossy(&out).contains(r#"{"t":"undone","op":"link","ok":true}"#));
        });
        entered.recv_timeout(REPLY_DEADLINE).expect("run_link reached the handshake");
        let responsive = answers.recv_timeout(REPLY_DEADLINE);
        drop(release);
        backend.join().expect("the backend finishes after releasing the held link");
        let (out, running, unjournalled, no_pending) = responsive.expect("a held link must leave Request::Paths responsive");
        let lines = String::from_utf8(out).unwrap();
        let mut replies = lines.lines();
        assert_eq!(replies.next(), Some(paths_line(&[a.to_string_lossy().to_string(), b.to_string_lossy().to_string()]).as_str()));
        let refused = replies.next().expect("the second link is refused while the slot is held");
        assert!(refused.contains(r#""t":"error""#) && refused.contains("an operation is already running"), "{refused}");
        assert!(replies.next().is_none(), "a held link emits neither slow nor a premature linked line: {lines}");
        assert!(running && unjournalled && no_pending, "the link holds the slot, with no slow claim or journal before completion");
        assert!(std::fs::symlink_metadata(dest.join("a.txt")).is_err(), "undo removed the first link");
        assert!(std::fs::symlink_metadata(dest.join("b.txt")).is_err(), "undo removed the second link");
        assert!(a.exists() && b.exists(), "undo preserves both sources");
    }

    // A local window stats inline: an O(directory) copy per scroll breaks load-bearing rule 1.
    #[test]
    fn a_local_window_stats_inline_with_no_bound_call() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("windowlocal");
        d.file("a.txt", "a");
        d.file("b.txt", "b");
        let (tx, _rx) = crate::backend::events::channel::<Event>();
        let mut st = State::new(crate::backend::dirsizeworker::Worker::new(tx));
        st.base = d.path().to_path_buf();
        st.listing.push("a.txt", false);
        st.listing.push("b.txt", false);
        let (metas, _) = window_metas(&mut st, 0, 10).expect("a local window answers");
        assert_eq!(metas.len(), 2, "the window carries its rows");
        assert_eq!(crate::backend::iomount::test_calls(), 0, "a local window takes no bound call and clones no listing");
    }

    // The search root's figures go through the bound, so a dead server answers instead of hanging.
    #[test]
    fn a_search_root_is_statted_through_the_bound() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("searchroot");
        let (dev, writable) = search_root_info(d.path()).expect("a live root answers");
        assert_eq!(dev, crate::backend::fsinfo::dev_of(d.path()), "the bound answers the root's own figures");
        assert!(writable, "the sandbox is writable");
        assert_eq!(crate::backend::iomount::test_calls(), 1, "the root's stat takes exactly one bound call");
    }
}

// Issue 144's loop half: the gated arms of dispatch, driven one event at a time against a held remote scan.
#[cfg(test)]
#[path = "run_gate_tests.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "run_jobs_tests.rs"]
mod jobs_tests;

#[cfg(test)]
#[path = "run_session_tests.rs"]
mod session_wire_tests;

#[cfg(test)]
#[path = "run_hold_tests.rs"]
mod hold_wire_tests;

#[cfg(test)]
#[path = "run_search_landing_tests.rs"]
mod search_landing_tests;

#[cfg(test)]
#[path = "viewerorder_tests.rs"]
mod viewerorder_tests;

#[cfg(test)]
#[path = "run_git_tests.rs"]
mod git_wire_tests;

#[cfg(test)]
#[path = "run_emptytrash_tests.rs"]
mod emptytrash_tests;

#[cfg(test)]
#[path = "run_searchorder_tests.rs"]
mod searchorder_tests;

#[cfg(test)]
#[path = "run_menu_targets_tests.rs"]
mod menu_targets_tests;
