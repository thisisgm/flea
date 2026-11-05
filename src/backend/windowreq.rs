use super::meta::Meta;
use super::proto::error_line;
use super::run::say;
use super::state::{Held, State, Tables};
use crate::error::FleaError;
use std::io::Write;

pub fn write_window(out: &mut impl Write, st: &mut State, start: usize, count: usize, tb: &Tables) {
    write_window_gen(out, st, start, count, tb, None);
}

// A session answer's window, stamped with the query gen beside the listing stamp.
pub fn write_window_gen(
    out: &mut impl Write,
    st: &mut State,
    start: usize,
    count: usize,
    tb: &Tables,
    gen: Option<usize>,
) {
    let start = st.window_start(start);
    let (metas, ms, failure) = match window_metas(st, start, count) {
        Ok((metas, ms)) => (metas, ms, None),
        // A failed stat still answers its rows, each with the zeroes a lost stat renders, so a listing hold never waits on rows that cannot come.
        Err(error) => (super::meta::lost_window(&st.listing, start, count), 0.0, Some(error)),
    };
    let end = start.saturating_add(metas.len());
    // Zeroes from a failed stat are never cached, so the next window over these rows asks the mount again.
    if failure.is_none() {
        for (i, m) in metas.iter().enumerate() {
            st.window_meta.insert(start + i, (m.mode, m.mtime, m.target_is_dir, m.size));
        }
        // Sample window: start 0 with 3 metas keeps keys 0..3, so a scrolled-past window never grows the map.
        st.window_meta.retain(|&row, _| row >= start && row < end);
    }
    let line = window_line(st, tb, &metas, start, ms);
    match gen {
        Some(gen) => writeln!(out, "{}", super::answersizes::with_sizes(super::rowguard::stamped_gen(line, st.generation, gen), &st.dirsizes, start..end)).ok(),
        None => writeln!(out, "{}", super::rowguard::stamped(line, st.generation)).ok(),
    };
    // The rows end the listing hold first, then the error names the mount once.
    if let Some(error) = failure {
        say(out, &error_line(&error));
    }
}

// Cached search rows share the same serializer, including response-local kind indices; an empty window demands no row service.
pub(crate) fn window_line(st: &mut State, tb: &Tables, metas: &[Meta], start: usize, ms: f64) -> String {
    if st.held == Held::Walk && !metas.is_empty() {
        if let Some(session) = st.search_session.as_mut() {
            if st.base == session.scope {
                return super::rows::rows_line_cached(
                    &st.base,
                    &session.body,
                    &st.listing,
                    metas,
                    start,
                    ms,
                    &tb.mime,
                    &tb.icons,
                    &tb.aliases,
                    &tb.thumbs,
                    &mut tb.kinds.borrow_mut(),
                    &mut session.row_cache,
                );
            }
        }
    }
    tb.rows_line(&st.base, &st.listing, metas, start, ms)
}

// One window's metas: remoteness is decided before anything is moved, so a local window stats inline with no clone and a remote one moves only its rows to the worker.
pub(crate) fn window_metas(st: &mut State, start: usize, count: usize) -> Result<(Vec<Meta>, f64), FleaError> {
    if st.held == Held::Walk {
        if let Some(session) = st.search_session.as_mut() {
            if st.base == session.scope && !super::iomount::is_remote(&st.base, &session.body) {
                return Ok(session.row_cache.window(&st.base, &st.listing, start, count, &session.body));
            }
        }
    }
    if count == 0 {
        return Ok((Vec::new(), 0.0));
    }
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    if super::iomount::is_remote(&st.base, &body) {
        let rows = super::meta::window_rows(&st.listing, start, count);
        let threaded = st.listing.threaded_cached_with(&body);
        let base = st.base.clone();
        super::iomount::call(&base.clone(), &body, "window", move || super::meta::stat_window_rows(&base, &rows, threaded))
    } else {
        Ok(super::meta::stat_range(&st.base, &st.listing, start, count))
    }
}
