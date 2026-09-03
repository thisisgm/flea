use crate::backend::aliases::Aliases;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::meta::stat_range;
use crate::backend::archive::Formats;
use crate::backend::archivereq::{formats_line, start_archive, start_convert};
use crate::backend::convert;
use crate::backend::peek::peek_line;
use crate::backend::metareq::spawn as spawn_meta;
use crate::backend::opsdispatch::{cancel_transfer, do_mkdir, do_rename, do_undo, report_op, resolve_rows, start_duplicate, start_trash, start_transfer, Ops};
use crate::backend::opsreq::{op_err, OpMsg};
use crate::backend::mime::Db;
use crate::backend::dirsizereq::{queue_dirsizes, walk_one_dirsize};
use crate::backend::fsinfo::{fsinfo_line, read as read_fsinfo};
use crate::backend::fsinfo::dev_of;
use crate::backend::proto::{error_line, error_line_with_mode, listed_line, parse_request, paths_line, thumbed_line, Request};
use crate::backend::rows::rows_line;
use crate::backend::sandbox;
use crate::backend::scan::{mode_of, scan};
use crate::backend::listing::Listing;
use crate::backend::search::Search;
use crate::backend::state::{State, Tables};
use crate::backend::searchreq::{finish_search, step_search};
use crate::backend::sort::{parse_sort_by, sort_by_name, sort_listing};
use crate::backend::thumbcache::{default_root, Cache};
use crate::backend::thumbreq::{cancel_row, forget_one, report_done, thumb_rows};
use crate::backend::thumbs::{Done, Pool};
use crate::backend::thumbwrite::sweep_own_temps;
use crate::backend::thumbspec::Thumbnailers;
use crate::error::{from_io, FleaError};
use crate::heap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// Wider pools settle sooner and answer input later, and 4 is the widest that costs neither the first thumbnail nor the scroll; see AGENTS.md "Thumbnail requests".
const THUMB_WORKERS: usize = 4;
// The whole shutdown budget: a running job is killed at the pool's own 20 s deadline, so waiting longer than that can never cut one short.
const DRAIN_LIMIT: Duration = Duration::from_secs(25);

// std has no select, so every source of work reaches the loop as one of these.
enum Event {
    Request(String),
    Thumb(Done),
    // A write operation's own thread reports here, so the loop stays the only writer of stdout.
    Op(OpMsg),
    ReadError(FleaError),
    Closed,
}

// The loop stops on Quit; every other request continues it, because errors are responses.
#[derive(PartialEq)]
enum Control {
    Continue,
    Quit,
}

// stdin blocks, so reading it is a thread and the loop only ever waits on the channel.
fn spawn_reader(tx: Sender<Event>) {
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
fn spawn_op_forwarder(results: Receiver<OpMsg>, tx: Sender<Event>) {
    thread::spawn(move || {
        for msg in results {
            if tx.send(Event::Op(msg)).is_err() {
                return;
            }
        }
    });
}

// The pool answers on its own channel, so one thread joins the two onto the single receiver the loop waits on.
fn spawn_forwarder(results: Receiver<Done>, tx: Sender<Event>) {
    thread::spawn(move || {
        for done in results {
            if tx.send(Event::Thumb(done)).is_err() {
                return;
            }
        }
    });
}

// Errors are responses, so the loop never exits on a bad request.
pub fn run() -> i32 {
    // Before the first listing, because a threshold glibc has already ratcheted strands the next arena on the heap.
    heap::pin_mmap_threshold();
    let mut out = BufWriter::new(io::stdout());
    let aliases = Arc::new(Aliases::load());
    let thumbs = Arc::new(Thumbnailers::load(&aliases));
    let tb = Tables {
        mime: Db::load(),
        icons: Names::load(),
        aliases,
        thumbs,
        kinds: RefCell::new(Kinds::new()),
        formats: Arc::new(Formats::probe()),
    };
    let mut st = State {
        listing: Listing::new(),
        base: PathBuf::new(),
        asked: Vec::new(),
        outstanding: 0,
        dirsizes: HashMap::new(),
        dirsize_queue: Vec::new(),
        search: None,
        search_reported: Instant::now(),
    };

    let (tx, rx) = channel::<Event>();
    let (results, done) = channel::<Done>();
    let (op_tx, op_rx) = channel::<OpMsg>();
    let mut ops = Ops::new(op_tx);
    // The pool shares this process's one parse of both tables rather than reading the same two files again.
    let pool = Pool::new(THUMB_WORKERS, results, default_root(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    // Every thumbnail job fails closed without these two, so the reason is said once here rather than never; see AGENTS.md "Thumbnail sandbox".
    if !sandbox::available() {
        eprintln!("flea: thumbnails are disabled, bwrap or prlimit is not on PATH");
    }
    // The workers hold senders too, so no exit can come from a disconnect and every exit is an explicit event; see AGENTS.md "Thumbnail requests".
    spawn_forwarder(done, tx.clone());
    spawn_op_forwarder(op_rx, tx.clone());
    spawn_reader(tx);
    loop {
        // Idle (nothing queued and no walk running) this is exactly the old blocking recv, see docs/protocol.md "dirsize".
        let event = if st.dirsize_queue.is_empty() && st.search.is_none() {
            match rx.recv() {
                Ok(e) => e,
                Err(_) => break,
            }
        } else {
            match rx.try_recv() {
                Ok(e) => e,
                // No event waiting, so it is the walker's turn; looping back lets a meanwhile dirsizecancel be seen before the next row.
                Err(TryRecvError::Empty) => {
                    tick_walkers(&mut out, &mut st, &pool);
                    continue;
                }
                Err(TryRecvError::Disconnected) => break,
            }
        };
        match event {
            Event::Request(line) => {
                if handle_line(&line, &mut out, &mut st, &tb, &pool, &cache, &mut ops) == Control::Quit {
                    break;
                }
            }
            Event::Thumb(d) => report_done(&mut out, &mut st, d),
            Event::Op(m) => report_op(&mut out, &mut ops, m),
            Event::ReadError(e) => {
                // The framing cannot be trusted past a decode failure, so this reports and stops, as before.
                writeln!(out, "{}", error_line(&e)).ok();
                out.flush().ok();
                break;
            }
            Event::Closed => break,
        }
    }
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
) -> Control {
    match parse_request(line) {
        Request::List { path, first, hidden } => {
            // A new listing replaces whatever the walk was filling, so the walk ends before the scan starts.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            match scan(&path, hidden) {
                Ok((mut l, read_ms)) => {
                    let sort_ms = sort_by_name(&mut l, false);
                    // base and listing only move together, so a failed list cannot mix them.
                    st.base = PathBuf::from(&path);
                    st.listing = l;
                    forget_rows(st, pool);
                    writeln!(out, "{}", listed_line(st.listing.len(), read_ms, sort_ms, dev_of(&st.base))).ok();
                    // Rides along unasked: asking costs a 60 ms round trip at first paint.
                    write_window(out, st, 0, first, tb);
                }
                Err(e) => {
                    // A typed path reaches the denial with no parent row to remember the mode from,
                    // so the stat that survives the refused read is the pane's only source for it.
                    writeln!(out, "{}", error_line_with_mode(&e, mode_of(&path))).ok();
                }
            }
            out.flush().ok();
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
            forget_rows(st, pool);
            // The client is told at once that its old rows are gone, then the count grows as matches arrive.
            writeln!(out, "{}", listed_line(0, 0.0, 0.0, dev_of(&st.base))).ok();
            st.search = Some(Search::new(&path, &query, hidden));
            st.search_reported = Instant::now();
            out.flush().ok();
        }
        Request::SearchCancel => {
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
        }
        Request::Sort { by, desc } => {
            // The walk owns the listing sort would reorder, so it ends first rather than racing it.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            // A key that names no order is refused by name, so a client's sort mark can only describe the order it got.
            match parse_sort_by(&by) {
                Err(msg) => {
                    let e = FleaError { where_: "sort".to_string(), path: by.clone(), msg: msg.to_string() };
                    writeln!(out, "{}", error_line(&e)).ok();
                }
                Ok(order) => {
                    // read carries the metadata pass here, 0.0 for name; see docs/protocol.md "listed".
                    let (pass_ms, sort_ms) = sort_listing(&mut st.listing, &st.base, order, desc);
                    forget_rows(st, pool);
                    writeln!(out, "{}", listed_line(st.listing.len(), pass_ms, sort_ms, dev_of(&st.base))).ok();
                }
            }
            out.flush().ok();
        }
        Request::Thumb { rows } => {
            thumb_rows(out, &rows, st, tb, pool, cache);
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
        }
        Request::Transfer { op, paths, rows, dest } => {
            let named = resolve_rows(paths, &rows, &st.base, &st.listing);
            start_transfer(out, ops, &op, named, &dest)
        }
        Request::TransferCancel { id } => cancel_transfer(ops, id),
        Request::Trash { paths, rows } => {
            let named = resolve_rows(paths, &rows, &st.base, &st.listing);
            start_trash(out, ops, named)
        }
        Request::Rename { path, to } => do_rename(out, ops, &path, &to),
        Request::MkDir { path, name } => do_mkdir(out, ops, &path, &name),
        Request::Duplicate { path } => start_duplicate(out, ops, &path),
        Request::Undo => do_undo(out, ops),
        // Never touches st.listing, which is the whole point: a column is not the pane's own listing.
        Request::Peek { path, first, hidden } =>
            say(out, &peek_line(&path, first, hidden, &tb.mime, &tb.icons)),
        // A compress names absolute paths and no path; an extract names the one archive in path.
        Request::Archive { op, paths, path, dest, format } => {
            // A typo would otherwise fall through to extract; refuse an op that names neither.
            if op != "compress" && op != "extract" {
                let e = op_err("archive", &op, "op must be compress or extract");
                writeln!(out, "{}", error_line(&e)).ok();
                out.flush().ok();
            } else {
                start_archive(
                    out, ops, Arc::clone(&tb.formats), op == "compress",
                    paths, format, PathBuf::from(&path), PathBuf::from(&dest));
            }
        }
        Request::Convert { path, dest, strip } =>
            start_convert(out, ops, PathBuf::from(&path), PathBuf::from(&dest), strip),
        Request::Formats => say(out, &formats_line(&tb.formats, convert::available())),
        Request::FsInfo => say(out, &fsinfo_line(&read_fsinfo(&st.base))),
        // One row, only when a client asked: the same no-sweep rule thumb and dirsize already follow.
        Request::Meta { row, text, media, archive } => {
            if row < st.listing.len() {
                let want = if archive { Some(Arc::clone(&tb.formats)) } else { None };
                spawn_meta(row, st.base.join(st.listing.name(row)), text, media, want, ops.tx.clone())
            }
        }
        Request::Paths { rows } =>
            say(out, &paths_line(&resolve_rows(Vec::new(), &rows, &st.base, &st.listing))),
        Request::Quit => return Control::Quit,
        // corner: an unrecognised line is answered with silence, see AGENTS.md.
        Request::Unknown => {}
    }
    Control::Continue
}

// A new row order invalidates every outstanding index, so the queue goes and no result can be reported against the new listing.
fn forget_rows(st: &mut State, pool: &Pool) {
    st.outstanding = st.outstanding.saturating_sub(pool.cancel_all().len());
    st.asked.clear();
    // A list or a sort changes which row an index names, the same reason thumbnails clear their map.
    st.dirsizes.clear();
    st.dirsize_queue.clear();
}

// dirsize first: its rows are on screen now, while a search walk is work the client asked for and can wait a tick.
fn tick_walkers(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool) {
    if !st.dirsize_queue.is_empty() {
        walk_one_dirsize(out, st);
        return;
    }
    // A finished walk hands back its rows in ranked order, which renames every outstanding index.
    if step_search(out, st) {
        forget_rows(st, pool);
    }
}

// A worker inside a child owns a temp file in the shared cache that only its own return publishes or removes; see AGENTS.md "Thumbnail requests".
fn drain(
    out: &mut BufWriter<io::Stdout>,
    st: &mut State,
    ops: &mut Ops,
    rx: &Receiver<Event>,
    pool: &Pool,
    cache: &Cache,
) {
    let deadline = Instant::now() + DRAIN_LIMIT;
    // A clean shutdown cancels the operation rather than abandoning it: a cancelled copy removes its own
    // partial destination, a file by copy_file and a tree by copy_dir, so quitting leaves nothing behind.
    if ops.running.is_some() {
        ops.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    while st.outstanding > 0 || ops.running.is_some() {
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

pub fn since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn write_window(out: &mut impl Write, st: &State, start: usize, count: usize, tb: &Tables) {
    let (metas, ms) = stat_range(&st.base, &st.listing, start, count);
    let start = start.min(st.listing.len());
    let mut kinds = tb.kinds.borrow_mut();
    let line = rows_line(&st.listing, &metas, start, ms, &tb.mime, &tb.icons, &tb.aliases, &tb.thumbs, &mut kinds);
    writeln!(out, "{}", line).ok();
}
