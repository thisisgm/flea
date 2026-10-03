use crate::backend::meta::stat_range;
use crate::backend::archivereq::{formats_line, start_archive, start_convert};
use crate::backend::convert;
use crate::backend::peek::peek_line;
use crate::backend::metareq::spawn as spawn_meta;
use crate::backend::opsdispatch::{cancel_transfer, do_mkdir, do_newfile, do_rename, do_undo, report_op, resolve_rows, start_duplicate, start_trash, start_transfer, start_menu_transfer, start_redo, Ops};
use crate::backend::opsreq::OpMsg;
use crate::backend::dirsizereq::{queue_dirsizes, seed_answered, start_next, report_done as report_dirsize};
use crate::backend::dirsize::DirSize;
use crate::backend::events::{spawn_forwarder, spawn_op_forwarder, spawn_reader, Event};
use crate::backend::fsinfo::fsinfo_line;
use crate::backend::fsinfo::dev_of;
use crate::backend::fsinforeq::FsInfo;
use crate::backend::listpaths;
use crate::backend::listwork::{self, Gate};
use crate::backend::proto::{error_line, listed_line, listed_line_anchor, parse_request, paths_line, thumbed_line, Request};
use crate::backend::rows::rows_line;
use crate::backend::sandbox;
use crate::backend::listing::Listing;
use crate::backend::search::Search;
use crate::backend::state::{State, Tables};
use crate::backend::searchreq::{finish_search, step_search};
use crate::backend::ordering;
use crate::backend::thumbcache::{default_root, Cache};
use crate::backend::thumbreq::{cancel_row, forget_one, report_done, thumb_rows};
use crate::backend::thumbs::{Done, Pool};
use crate::backend::thumbwrite::sweep_own_temps;
use crate::backend::watch::{changed_line, Watch};
use crate::error::FleaError;
use crate::heap;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Wider pools settle sooner and answer input later: 6 settles the media fixture ahead of strata for 5.5 ms of input under a full pool; see AGENTS.md "Thumbnail requests".
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
    // Before the first listing, because a threshold glibc has already ratcheted strands the next arena on the heap.
    heap::pin_mmap_threshold();
    // Recorded when the first rows go out, the next launch's prefetch list; see src/prefetch.rs.
    crate::prefetch::record_after_first_rows();
    let mut out = BufWriter::new(io::stdout());
    let tb = Tables::load();
    let (tx, rx) = channel::<Event>();
    let mut st = State::new(super::dirsizeworker::Worker::new(tx.clone()));
    let (results, done) = channel::<Done>();
    let (op_tx, op_rx) = channel::<OpMsg>();
    let mut ops = Ops::new(op_tx);
    let pool = Pool::new(THUMB_WORKERS, results, default_root(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    // Every thumbnail job fails closed without these two, so the reason is said once here rather than never; see AGENTS.md "Thumbnail sandbox".
    if !sandbox::available() {
        eprintln!("flea: thumbnails are disabled, bwrap or prlimit is not on PATH");
    }
    // The workers hold senders too, so no exit can come from a disconnect and every exit is an explicit event; see AGENTS.md "Thumbnail requests".
    spawn_forwarder(done, tx.clone());
    spawn_op_forwarder(op_rx, tx.clone());
    spawn_reader(tx.clone(), Arc::clone(&ops.live));
    let mut fsinfo = FsInfo::new(tx.clone());
    // Armed before the first request, so no listing is ever answered with nothing watching it.
    let mut watch = Watch::start(tx.clone());
    // Issue 144: a list scans off the loop, so the requests that land while it is out wait here and
    // replay in the order they arrived once it answers; see src/backend/listwork.rs.
    let mut gate = Gate::new();
    'main: loop {
        start_next(&mut st);
        // Size results wake this receiver; only search still needs idle ticks.
        let event = if st.search.is_none() {
            match rx.recv() {
                Ok(e) => e,
                Err(_) => break,
            }
        } else {
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
        match event {
            Event::Request(line) => {
                // A scan is out, so a later request cannot be answered against a listing it will replace.
                if gate.holds() {
                    gate.defer(line);
                    continue;
                }
                if handle_line(&line, &mut out, &mut st, &tb, &pool, &cache, &mut ops, &mut watch, &mut fsinfo, &tx, &mut gate) == Control::Quit {
                    break;
                }
            }
            // The scan answered: it becomes the listing, then every request that waited replays in order.
            Event::List(done) => {
                if let Some(job) = gate.take() {
                    listwork::finish(&mut out, &mut st, &tb, &pool, &mut watch, &mut fsinfo, listwork::Landed { job, done });
                    // A change seen during the scan is answered only if it was in the directory that
                    // became current, exactly as a scan that held the loop would have answered it.
                    if gate.take_changes().iter().any(|&wd| watch.is_current(wd)) {
                        say(&mut out, &changed_line(&st.base));
                    }
                }
                while !gate.holds() {
                    match gate.next() {
                        None => break,
                        Some(line) => {
                            if handle_line(&line, &mut out, &mut st, &tb, &pool, &cache, &mut ops, &mut watch, &mut fsinfo, &tx, &mut gate) == Control::Quit {
                                break 'main;
                            }
                        }
                    }
                }
                // stdin closed while the scan was out: the listing was answered first, so now leave.
                if gate.closed() && gate.idle() {
                    break;
                }
            }
            Event::Thumb(d) => report_done(&mut out, &mut st, d),
            Event::DirSize(d) => report_dirsize(&mut out, &mut st, d),
            // A slow mount's figures, printed only for the directory on screen now.
            Event::FsInfo(d) => {
                if let Some(info) = fsinfo.finish(d, &st.base) {
                    say(&mut out, &fsinfo_line(&info, &st.base.to_string_lossy(), crate::backend::fsinforeq::slow_class(&st.base)));
                }
            }
            // The one line no client asked for, and only ever for the directory being listed now.
            Event::Changed(wd) => {
                // While a scan is out the descriptor that will be current is not known yet, so the
                // burst waits and is answered once the scan says which directory it listed.
                if gate.holds() {
                    gate.note_change(wd);
                } else if watch.is_current(wd) {
                    say(&mut out, &changed_line(&st.base));
                }
            }
            Event::Op(m) => report_op(&mut out, &mut ops, m),
            Event::ReadError(e) => {
                // The framing cannot be trusted past a decode failure, so this reports and stops, as before.
                writeln!(out, "{}", error_line(&e)).ok();
                out.flush().ok();
                break;
            }
            // A scan still out is answered before the process leaves, so no client loses its listing.
            Event::Closed => {
                if gate.holds() {
                    gate.close();
                    continue;
                }
                break;
            }
        }
    }
    st.dirsize_worker.cancel();
    drain(&mut out, &mut st, &mut ops, &rx, &pool, &cache);
    0
}

// One answer, written and flushed: the four read-only requests below differ only in what they say.
fn say(out: &mut BufWriter<io::Stdout>, line: &str) {
    writeln!(out, "{}", line).ok();
    out.flush().ok();
}

fn handle_line(
    line: &str,
    out: &mut BufWriter<io::Stdout>,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    cache: &Cache,
    ops: &mut Ops,
    watch: &mut Watch,
    fsinfo: &mut FsInfo,
    tx: &Sender<Event>,
    gate: &mut Gate,
) -> Control {
    // Rows read from a numbering this listing has already replaced name other files, so they are refused.
    if let Some(refused) = super::rowguard::refusal(line, st.generation) {
        say(out, &refused);
        return Control::Continue;
    }
    match parse_request(line) {
        Request::Permissions { line } => say(out, &ops.permissions.handle(&line)),
        Request::Picker { line } => {
            let replies = ops.tx.clone();
            ops.picker.get_or_insert_with(|| super::picker::Picker::new(replies)).request(line);
        }
        Request::MenuAction { line, rows } => {
            let paths = resolve_rows(Vec::new(), &rows, &st.base, &st.listing);
            let cursor = crate::json::field_usize(&line, "cursor").map(|index|
                resolve_rows(Vec::new(), &[index], &st.base, &st.listing).into_iter().next().unwrap_or_default());
            super::opsdispatch::request_menu_action(out, ops, line, paths, cursor);
        }
        // Directive 71: a CLI run of a second or more, so it answers on its own thread.
        Request::LocalSend { op, peer, paths, id } => super::localsend::request(op, peer, paths, id, ops.tx.clone()),
        Request::TrashBrowse { line } => {
            let replies = ops.tx.clone();
            ops.trashbrowser.get_or_insert_with(|| super::trashbrowse::TrashBrowser::new(replies)).request(line);
        }
        // The scan runs off the loop and answers as an event, so a slow mount never holds it; see src/backend/listwork.rs.
        Request::List { path, first, hidden } => {
            let job = listwork::Job { path, first, line: line.to_string() };
            gate.begin(listwork::start(out, st, pool, watch, tx, job, hidden));
        }
        // A set of named paths is not a directory, so the watch stops rather than following its base.
        Request::ListPaths { paths, first } => {
            watch.stop();
            listpaths::answer(out, st, pool, tb, &paths, first, line)
        }
        Request::Window { start, count } => {
            write_window(out, st, start, count, tb);
            out.flush().ok();
        }
        Request::Search { path, query, hidden } => {
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            st.base = PathBuf::from(&path);
            st.listing = Listing::new();
            // A walk's matches are not a directory either, so nothing is watched until list asks again.
            watch.stop();
            forget_rows(st, pool);
            // The client is told at once that its old rows are gone, then the count grows as matches arrive.
            writeln!(out, "{}", listed_line(0, 0.0, 0.0, dev_of(&st.base), &st.base.to_string_lossy())).ok();
            st.search = Some(Search::new(&path, &query, hidden));
            st.search_reported = Instant::now();
            out.flush().ok();
        }
        Request::SearchCancel => {
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
        }
        Request::Sort { by, desc: _, anchor } => {
            // The walk owns the listing sort would reorder, so it ends first rather than racing it.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            // A key that names no order is refused by name, so a client's sort mark can only describe the order it got.
            match ordering::request(&mut st.listing, &st.base, &tb.mime, line) {
                Err(msg) => {
                    let e = FleaError { where_: "sort".to_string(), path: by.clone(), msg: msg.to_string() };
                    writeln!(out, "{}", error_line(&e)).ok();
                }
                Ok((pass_ms, sort_ms, sized)) => {
                    forget_rows(st, pool);
                    // After forget_rows, which clears the very map this seeds.
                    seed_answered(st, &sized);
                    let line = match anchor.as_deref() {
                        // The listing is a snapshot, so only a path it never held answers -1.
                        Some(anchor) => listed_line_anchor(
                            st.listing.len(), pass_ms, sort_ms, dev_of(&st.base),
                            &st.base.to_string_lossy(), anchor,
                            st.listing.index_of(&st.base, Path::new(anchor)).map(|index| index as isize).unwrap_or(-1),
                        ),
                        None => listed_line(st.listing.len(), pass_ms, sort_ms, dev_of(&st.base), &st.base.to_string_lossy()),
                    };
                    writeln!(out, "{}", line).ok();
                }
            }
            out.flush().ok();
        }
        Request::Thumb { rows, cache_only } => {
            thumb_rows(out, &rows, st, tb, pool, cache, cache_only);
            out.flush().ok();
        }
        Request::ThumbCancel { rows } => {
            if rows.is_empty() {
                // An empty rows cancels everything queued, and every job it drops has to leave the map with it; see AGENTS.md "Thumbnail requests".
                for job in pool.cancel_all() {
                    forget_one(st, &job.path);
                }
            } else {
                for row in &rows {
                    cancel_row(st, pool, *row);
                }
            }
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
        Request::Collisions { id, paths, rows, dest, menu_id } => super::collide::ask_beside(ops, id, menu_id, resolve_rows(paths, &rows, &st.base, &st.listing), &dest, &tb.mime, &tb.icons),
        Request::TransferCancel { id } => cancel_transfer(ops, id),
        Request::Trash { paths, rows, menu_id } => {
            let named = resolve_rows(paths, &rows, &st.base, &st.listing);
            start_trash(out, ops, named, menu_id)
        }
        Request::Rename { path, to, menu_id } => {
            if menu_id == 0 { do_rename(out, ops, &path, &to); }
            else { super::opsdispatch::do_menu_rename(out, ops, &path, &to, menu_id); }
        }
        Request::MkDir { path, name } => do_mkdir(out, ops, &path, &name),
        Request::NewFile { path, name, id } => do_newfile(out, ops, &path, &name, id),
        Request::Duplicate { path, menu_id } => start_duplicate(out, ops, &path, menu_id),
        Request::Undo => do_undo(out, ops),
        Request::Redo => start_redo(out, ops),
        // Never touches st.listing, which is the whole point: a column is not the pane's own listing.
        Request::Peek { path, first, hidden, hidden_last, focus } =>
            say(out, &peek_line(&path, first, hidden, hidden_last, &focus, &tb.mime, &tb.icons)),
        // A compress names absolute paths and no path; an extract names the one archive in path.
        Request::Archive { op, paths, path, dest, format, menu_id } => start_archive(
            out, ops, Arc::clone(&tb.formats), &op,
            paths, format, PathBuf::from(&path), PathBuf::from(&dest), menu_id),
        Request::Convert { path, dest, strip, menu_id, request_id, check } =>
            start_convert(out, ops, PathBuf::from(&path), PathBuf::from(&dest), strip, menu_id, request_id, check),
        Request::Formats { id } => {
            let mut line = formats_line(&tb.formats, convert::available());
            line.insert_str(line.len() - 1, &format!(r#", "id":{},"providers":{}"#, id, super::providers::facts()));
            say(out, &line);
        }
        // The class rides beside the figures once per directory change; a slow mount's fresh figures follow as a second line.
        Request::FsInfo => {
            let (info, class) = fsinfo.answer(&st.base);
            say(out, &fsinfo_line(&info, &st.base.to_string_lossy(), class));
        }
        // One row, only when a client asked: the same no-sweep rule thumb and dirsize already follow.
        Request::Meta { row, text, media, archive, token } => {
            if row < st.listing.len() {
                let want = if archive { Some(Arc::clone(&tb.formats)) } else { None };
                spawn_meta(row, st.base.join(st.listing.name(row)), text, media, want, token, ops.tx.clone())
            }
        }
        Request::Paths { rows } =>
            say(out, &paths_line(&resolve_rows(Vec::new(), &rows, &st.base, &st.listing))),
        Request::Locate { path } => {
            let index = st.listing.index_of(&st.base, Path::new(&path));
            say(out, &super::proto::located_line(&st.base.to_string_lossy(), &path, index));
        }
        Request::LocateMany { paths, id, menu_id, transfer_id } => {
            let mut matches = st.listing.indices_of(&st.base, &paths);
            let error = if transfer_id > 0 {
                if ops.transfer_retry.0 != transfer_id {
                    Some("Transfer retry identities expired; select the items again.".to_string())
                } else {
                    super::opsreq::retain_retry(&ops.transfer_retry.1, &mut matches);
                    None
                }
            } else if menu_id == 0 { None } else {
                ops.menuactions.as_ref().ok_or_else(|| "Deletion survivor identities expired; select the items again.".to_string())
                    .and_then(|menu| menu.retain_survivors(menu_id, &mut matches)).err()
            };
            if error.is_some() { matches.clear(); }
            say(out, &super::proto::located_many_line(&st.base.to_string_lossy(), id, transfer_id, &matches, error.as_deref()));
        }
        Request::Jump { id, favourites, recent } => super::jump::request(id, favourites, recent, ops.tx.clone()),
        Request::Quit => return Control::Quit,
        // corner: an unrecognised line is answered with silence, see AGENTS.md.
        Request::Unknown => {}
    }
    Control::Continue
}

// A new row order invalidates every outstanding index, so the queue goes and no result can be reported against the new listing.
pub fn forget_rows(st: &mut State, pool: &Pool) {
    st.generation += 1;
    st.outstanding = st.outstanding.saturating_sub(pool.cancel_all().len());
    st.asked.clear();
    // A list or a sort changes which row an index names, the same reason thumbnails clear their map.
    st.dirsizes.clear();
    st.dirsize_queue.clear();
    st.dirsize_worker.cancel();
}

// A list's scanned and ordered result becomes the listing and is answered: its listed line, then its first rows.
pub(crate) fn adopt(out: &mut impl Write, st: &mut State, pool: &Pool, tb: &Tables, path: &str, l: Listing, (read_ms, sort_ms): (f64, f64), sized: &[Option<DirSize>], first: usize) {
    // base and listing only move together, so a failed list cannot mix them.
    st.base = PathBuf::from(path);
    st.listing = l;
    forget_rows(st, pool);
    // After forget_rows, which clears the very map this seeds.
    seed_answered(st, sized);
    writeln!(out, "{}", listed_line(st.listing.len(), read_ms, sort_ms, dev_of(&st.base), &st.base.to_string_lossy())).ok();
    // Rides along unasked: asking costs a 60 ms round trip at first paint.
    write_window(out, st, 0, first, tb);
}

// Search advances only when the request channel is idle.
fn tick_walkers(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool) {
    // A finished walk hands back its rows in ranked order, which renames every outstanding index.
    if step_search(out, st) {
        forget_rows(st, pool);
    }
}

// A worker inside a child owns a temp file in the shared cache that only its own return publishes or removes; see AGENTS.md "Thumbnail requests".
pub(crate) fn drain(
    out: &mut impl Write,
    st: &mut State,
    ops: &mut Ops,
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
    while st.outstanding > 0 || ops.live.running().is_some() || !ops.detached.is_empty() {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Thumb(d)) => report_done(out, st, d),
            Ok(Event::Op(m)) => report_op(out, ops, m),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    pool.cancel_all();
    // The queue is empty now, so no worker can start a new job and the temps still on disk are exactly the abandoned ones.
    if st.outstanding > 0 {
        sweep_own_temps(&cache.large_dir());
    }
    // corner: a row the deadline cut short is answered empty rather than left unanswered, see AGENTS.md "Thumbnail requests".
    for (_, row) in std::mem::take(&mut st.asked) {
        writeln!(out, "{}", thumbed_line(row, "", 0.0)).ok();
    }
    out.flush().ok();
}

pub fn write_window(out: &mut impl Write, st: &State, start: usize, count: usize, tb: &Tables) {
    let (metas, ms) = stat_range(&st.base, &st.listing, start, count);
    let start = start.min(st.listing.len());
    let mut kinds = tb.kinds.borrow_mut();
    let line = rows_line(&st.listing, &metas, start, ms, &tb.mime, &tb.icons, &tb.aliases, &tb.thumbs, &mut kinds);
    writeln!(out, "{}", super::rowguard::stamped(line, st.generation)).ok();
}
