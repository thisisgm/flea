use crate::backend::proto::thumbed_line;
#[path = "thumbidentity.rs"]
pub mod identity;
use crate::backend::timing::since;
use crate::backend::state::{State, Tables};
use crate::backend::thumbcache::{Cache, Hit};
use crate::backend::thumbs::{trace, Done, Job, Outcome, Pool, Trace};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// A cancelled row is never answered, so dropping its mapping is the whole of what a later request for it needs.
pub(crate) fn forget_one(st: &mut State, path: &Path) {
    st.outstanding = st.outstanding.saturating_sub(1);
    if let Some(at) = st.asked.iter().position(|(p, _)| p == path) {
        st.asked.remove(at);
        st.thumb_mtimes.remove(path);
    }
}

// st_mode mask and symlink bits: a symlink row always takes the target-stat path below.
const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

// corner: generation happens only for the rows a client named, and nothing here walks the listing; see AGENTS.md "Thumbnail requests".
pub(crate) fn thumb_rows(
    out: &mut impl Write,
    rows: &[usize],
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    cache: &Cache,
    options: impl Into<identity::Options>,
) {
    let options = options.into();
    let cache_only = options.cache_only;
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    for &row in rows {
        if row >= st.listing.len() {
            continue;
        }
        let t = Instant::now();
        let name = st.listing.name(row);
        let path = st.base.join(name);
        let reply = options.identity.as_ref().and_then(|tag| identity::Reply::capture(tag, row, &path, st.generation));
        if options.identity.is_some() && reply.is_none() { continue; }
        let answer = |file: &str, ms| reply.as_ref().map_or_else(|| thumbed_line(row, file, ms), |reply| reply.line(file, ms));
        // The window's lstat mode and mtime stand in for a stat, except a symlink whose target the path below gates.
        if let Some((mode, mtime, target_dir)) = st.window_meta.get(&row).copied() {
            if mode & S_IFMT != S_IFLNK {
                if target_dir {
                    writeln!(out, "{}", answer("", since(t))).ok();
                    continue;
                }
                let declared = crate::backend::meta::thumbnailable(mode).then(|| {
                    tb.mime.lookup(name).filter(|m| tb.thumbs.for_mime(m, &tb.aliases).is_some()).map(str::to_string)
                        .or_else(|| super::model3d::embedded_thumbnail_supported(name).then(|| "model/x-flea-3d".to_string()))
                }).flatten();
                let mime = match declared {
                    None => {
                        writeln!(out, "{}", answer("", since(t))).ok();
                        continue;
                    }
                    Some(m) => m,
                };
                let table = if super::model3d::embedded_thumbnail_supported(name) { super::model3dreq::THUMB_TABLE } else { tb.thumbs.fingerprint() };
                match cache.lookup(&path, mtime, table) {
                    Hit::Ready(p) => {
                        writeln!(out, "{}", answer(&p.to_string_lossy(), since(t))).ok();
                    }
                    Hit::Failed => {
                        writeln!(out, "{}", answer("", since(t))).ok();
                    }
                    Hit::Miss if cache_only => {
                        writeln!(out, "{}", answer("", since(t))).ok();
                    }
                    Hit::Miss => queue_row(out, st, pool, row, path, mtime, (mime, reply)),
                }
                continue;
            }
            // A symlink row falls through to the target-stat path, so the target is gated and its mtime keys the cache.
        }
        // No window covered this row, so a cache-only ask answers none rather than statting on the loop.
        if cache_only && !st.window_meta.contains_key(&row) {
            writeln!(out, "{}", answer("", since(t))).ok();
            continue;
        }
        let name = st.listing.name(row).to_string();
        let path = st.base.join(&name);
        // corner: only a regular file is queued, so a fifo or a device node named like a video cannot block a worker; see AGENTS.md "Thumbnail requests".
        let owned = path.clone();
        let meta = match super::iomount::call(&path, &body, "thumb", move || std::fs::metadata(&owned).ok()) {
            Ok(found) => found.filter(|m| m.is_file()),
            Err(_) => {
                writeln!(out, "{}", answer("", since(t))).ok();
                continue;
            }
        };
        let declared = meta.as_ref().and_then(|_| {
            tb.mime
                .lookup(&name)
                .filter(|m| tb.thumbs.for_mime(m, &tb.aliases).is_some())
                .map(str::to_string)
                .or_else(|| super::model3d::embedded_thumbnail_supported(&name).then(|| "model/x-flea-3d".to_string()))
        });
        let mime = match declared {
            // corner: a row no thumbnailer declares is answered at once and never queued, see AGENTS.md.
            None => {
                writeln!(out, "{}", answer("", since(t))).ok();
                continue;
            }
            Some(m) => m,
        };
        let mtime = meta.map(|m| m.mtime()).unwrap_or(0);
        let table = if super::model3d::embedded_thumbnail_supported(&name) { super::model3dreq::THUMB_TABLE } else { tb.thumbs.fingerprint() };
        match cache.lookup(&path, mtime, table) {
            Hit::Ready(p) => {
                writeln!(out, "{}", answer(&p.to_string_lossy(), since(t))).ok();
            }
            Hit::Failed => {
                writeln!(out, "{}", answer("", since(t))).ok();
            }
            // A class whose switch is off is cache-only: a miss answers none without a decoder, so a NAS folder costs one stat per row and no read.
            Hit::Miss if cache_only => {
                writeln!(out, "{}", answer("", since(t))).ok();
            }
            Hit::Miss => queue_row(out, st, pool, row, path, mtime, (mime, reply)),
        }
    }
}

// One entry per queued, waiting or running job, so MAX_QUEUE, MAX_DEFERRED and the worker count bound this map too.
fn queue_row(
    out: &mut impl Write,
    st: &mut State,
    pool: &Pool,
    row: usize,
    path: PathBuf,
    mtime: i64,
    request: (String, Option<identity::Reply>),
) {
    let (mime, reply) = request;
    // A mapped path already has a job for this listing, so a repeated row costs a worker nothing; see AGENTS.md "Thumbnail requests".
    if st.asked.iter().any(|(p, _)| *p == path) {
        if let Some(reply) = reply {
            if !identity::attach(st, &path, mtime, &reply) {
                // No work for this source version can serve the tagged subscriber, so it receives an identity-bound refusal.
                writeln!(out, "{}", reply.line("", 0.0)).ok();
            }
        }
        return;
    }
    // Workers may have freed room since the last report, so pump before refusing a row at the deferred cap.
    super::thumbdefer::pump(out, st, pool);
    // corner: past MAX_DEFERRED nothing waits any longer, so the row is answered empty at once; see AGENTS.md "Thumbnail requests".
    if st.deferred.full() {
        writeln!(out, "{}", reply.as_ref().map_or_else(|| thumbed_line(row, "", 0.0), |reply| reply.line("", 0.0))).ok();
        return;
    }
    st.asked.push((path.clone(), row));
    st.thumb_mtimes.insert(path.clone(), mtime);
    st.outstanding += 1;
    let replies = Some(identity::start(st, path.clone(), mtime, reply));
    st.deferred.enqueue(Job { replies, path, mtime, trash_identity: None, mime, trace: trace(row) });
    super::thumbdefer::pump(out, st, pool);
}

// A job already inside a worker keeps its mapping; every queued or waiting job leaves with its cancellation.
pub(crate) fn cancel_all(st: &mut State, pool: &Pool) {
    identity::retire_all(st);
    for job in pool.cancel_listing().into_iter().chain(st.deferred.take_listing()) {
        forget_one(st, &job.path);
    }
}

// A job already inside a worker cannot be cancelled, so its row stays mapped until it reports.
pub(crate) fn cancel_row(out: &mut impl Write, st: &mut State, pool: &Pool, row: usize) {
    identity::retire_row(st, row);
    let at = match st.asked.iter().position(|(_, r)| *r == row) {
        Some(i) => i,
        None => return,
    };
    // A row still waiting for room is taken back from the deferred queue, and a queued one from the pool.
    let mut dropped = st.deferred.take(&st.asked[at].0, None);
    if dropped.is_empty() {
        dropped = pool.cancel(&st.asked[at].0, None);
    }
    if dropped.is_empty() {
        return;
    }
    st.outstanding = st.outstanding.saturating_sub(dropped.len());
    st.thumb_mtimes.remove(&st.asked[at].0);
    st.asked.remove(at);
    // The room a cancel frees goes to the next waiting row now, not at the next report.
    super::thumbdefer::pump(out, st, pool);
}

// A result whose path is no longer mapped belongs to a superseded listing and is dropped, never reported against the current one.
pub(crate) fn report_done(out: &mut impl Write, st: &mut State, pool: &Pool, done: Done) {
    st.outstanding = st.outstanding.saturating_sub(1);
    // Workers have taken jobs off the pool since the last pump, so the oldest waiting rows go in now.
    super::thumbdefer::pump(out, st, pool);
    let file = match done.result {
        Outcome::Ready(p) => p.to_string_lossy().into_owned(),
        Outcome::Failed => String::new(),
    };
    super::trashthumb::answer(out, st, &done.path, done.mtime, done.trash_identity.as_deref(), &file);
    if let Some(at) = st.asked.iter().position(|(p, _)| done.trash_identity.is_none() && *p == done.path && st.thumb_mtimes.get(p) == Some(&done.mtime)) {
        st.thumb_mtimes.remove(&done.path);
        let row = st.asked.remove(at).1;
        writeln!(out, "{}", thumbed_line(row, &file, done.ms)).ok();
    }
    identity::complete(out, st, &done.replies, &file, done.ms);
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

#[cfg(test)]
#[path = "thumbcancel_tests.rs"]
mod cancel_tests;

#[cfg(test)]
#[path = "thumbreq_tests.rs"]
mod tests;
