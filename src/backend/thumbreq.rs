use crate::backend::proto::thumbed_line;
use crate::backend::run::since;
use crate::backend::state::{State, Tables};
use crate::backend::thumbcache::{Cache, Hit};
use crate::backend::thumbs::{trace, Done, Job, Outcome, Pool, Trace};
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// A cancelled row is never answered, so dropping its mapping is the whole of what a later request for it needs.
pub(crate) fn forget_one(st: &mut State, path: &Path) {
    st.outstanding = st.outstanding.saturating_sub(1);
    if let Some(at) = st.asked.iter().position(|(p, _, _)| p == path) {
        st.asked.remove(at);
    }
}

// corner: generation happens only for the rows a client named, and nothing here walks the listing; see AGENTS.md "Thumbnail requests".
pub(crate) fn thumb_rows(
    out: &mut BufWriter<io::Stdout>,
    rows: &[usize],
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    cache: &Cache,
) {
    for &row in rows {
        if row >= st.listing.len() {
            continue;
        }
        let t = Instant::now();
        let name = st.listing.name(row);
        let path = st.base.join(name);
        // corner: only a regular file is queued, so a fifo or a device node named like a video cannot block a worker; see AGENTS.md "Thumbnail requests".
        let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file());
        let declared = meta.as_ref().and_then(|_| {
            tb.mime
                .lookup(name)
                .filter(|m| tb.thumbs.for_mime(m, &tb.aliases).is_some())
                .map(str::to_string)
        });
        let mime = match declared {
            // corner: a row no thumbnailer declares is answered at once and never queued, see AGENTS.md.
            None => {
                writeln!(out, "{}", thumbed_line(row, "", since(t))).ok();
                continue;
            }
            Some(m) => m,
        };
        let mtime = meta.map(|m| m.mtime()).unwrap_or(0);
        match cache.lookup(&path, mtime) {
            Hit::Ready(p) => {
                writeln!(out, "{}", thumbed_line(row, &p.to_string_lossy(), since(t))).ok();
            }
            Hit::Failed => {
                writeln!(out, "{}", thumbed_line(row, "", since(t))).ok();
            }
            Hit::Miss => queue_row(out, st, pool, row, path, mtime, mime),
        }
    }
}

// One entry per queued or running job, so the pool's own queue bound plus the worker count bounds this map too.
fn queue_row(
    out: &mut BufWriter<io::Stdout>,
    st: &mut State,
    pool: &Pool,
    row: usize,
    path: PathBuf,
    mtime: i64,
    mime: String,
) {
    // A mapped path already has a job for this listing, so a repeated row costs a worker nothing; see AGENTS.md "Thumbnail requests".
    if st.asked.iter().any(|(p, _, _)| *p == path) {
        return;
    }
    let epoch = pool.next_epoch();
    st.asked.push((path.clone(), row, epoch));
    st.outstanding += 1;
    // A job the pool dropped to make room will never report, so its row is unmapped and answered here rather than at shutdown.
    for job in pool.submit(Job { path, mtime, mime, trace: trace(row), epoch }) {
        st.outstanding = st.outstanding.saturating_sub(1);
        if let Some(at) = st.asked.iter().position(|(p, _, e)| *p == job.path && *e == job.epoch) {
            let dropped_row = st.asked.remove(at).1;
            writeln!(out, "{}", thumbed_line(dropped_row, "", 0.0)).ok();
        }
    }
}

// Queued jobs are unmapped here and never report. A running child is killed and still reports Done,
// which report_done drops because the row is already gone.
pub(crate) fn cancel_row(st: &mut State, pool: &Pool, row: usize) {
    let at = match st.asked.iter().position(|(_, r, _)| *r == row) {
        Some(i) => i,
        None => return,
    };
    let path = st.asked[at].0.clone();
    let dropped = pool.cancel(&path);
    st.asked.remove(at);
    if !dropped.is_empty() {
        st.outstanding = st.outstanding.saturating_sub(dropped.len());
    }
}

// A result whose path is no longer mapped belongs to a superseded listing and is dropped, never reported against the current one.
pub(crate) fn report_done(out: &mut BufWriter<io::Stdout>, st: &mut State, done: Done) {
    st.outstanding = st.outstanding.saturating_sub(1);
    if matches!(done.result, Outcome::Cancelled) {
        if let Some(at) = st.asked.iter().position(|(p, _, e)| *p == done.path && *e == done.epoch) {
            st.asked.remove(at);
        }
        return;
    }
    let row = match st.asked.iter().position(|(p, _, e)| *p == done.path && *e == done.epoch) {
        Some(i) => st.asked.remove(i).1,
        None => return,
    };
    let file = match done.result {
        Outcome::Ready(p) => p.to_string_lossy().into_owned(),
        Outcome::Failed | Outcome::Cancelled => String::new(),
    };
    writeln!(out, "{}", thumbed_line(row, &file, done.ms)).ok();
    out.flush().ok();
    if let Some(t) = &done.trace {
        trace_line(t);
    }
}

// stderr, never stdout, because stdout is the wire and a client parses every line of it; see AGENTS.md "Thumbnail trace".
fn trace_line(t: &Trace) {
    let whole = t.at.elapsed();
    // A job that failed before any child has no spawn and no exit mark, so its whole life after the pop is charged to setup.
    let spawned = if t.spawned.is_zero() { whole } else { t.spawned };
    let exited = if t.exited.is_zero() { whole } else { t.exited };
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    eprintln!(
        "flea: trace row={} depth={} queued={:.2} setup={:.2} child={:.2} after={:.2} total={:.2}",
        t.row, t.depth, ms(t.popped), ms(spawned.saturating_sub(t.popped)),
        ms(exited.saturating_sub(spawned)), ms(whole.saturating_sub(exited)), ms(whole)
    );
}
