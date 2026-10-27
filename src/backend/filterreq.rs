// The wire side of the filter (issue 315): a filter or filterwindow request in, a filtered line and
// one frows line out, the way thumbreq answers thumb. The matching is src/backend/filter.rs; this
// stats only the matched rows one view window names, local inline and remote on the mount's worker,
// exactly as run.rs window_metas does for a listing window, so rule 1 holds under a filter too.
use crate::backend::filter;
use crate::backend::meta::{self, Meta};
use crate::backend::proto::error_line;
use crate::backend::rowguard::stamped_gen;
use crate::backend::rows::frows_line;
use crate::backend::state::{State, Tables};
use crate::error::FleaError;
use std::io::Write;

// One keystroke: the folder's answer as runs, then the first window of its rows, placed around the
// client's cursor, in that order so the client sets the shown model before the rows that fill it land.
pub fn query(out: &mut impl Write, st: &mut State, tb: &Tables, query: &str, gen: usize, cursor: filter::Cursor, count: usize) {
    if let Some((n, runs, ms)) = filter::answer(&mut st.filter, &st.listing, st.generation, query, gen) {
        writeln!(out, "{}", filter::filtered_line(gen, n, &runs, ms, st.generation)).ok();
        let start = filter::start_around(&st.filter, st.generation, cursor);
        window(out, st, tb, start, count);
    }
    out.flush().ok();
}

// One view window of the standing matches, stat'd now; silent when no query stands over this numbering.
pub fn window(out: &mut impl Write, st: &State, tb: &Tables, start: usize, count: usize) {
    let Some((indices, gen, start)) = filter::window(&st.filter, st.generation, start, count) else { return };
    match metas_at(st, &indices) {
        Ok((metas, ms)) => {
            let mut kinds = tb.kinds.borrow_mut();
            let line = frows_line(&st.base, &st.listing, &metas, &indices, start, ms, &tb.mime, &tb.icons, &tb.aliases, &tb.thumbs, &mut kinds);
            writeln!(out, "{}", stamped_gen(line, st.generation, gen)).ok();
        }
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
        }
    }
    out.flush().ok();
}

// The matched rows' metas: remoteness is decided before anything is moved, so a local set stats inline and a remote one moves only its rows to the worker.
// The mount table is read once and handed down, so a slow local pass does not read it again for its thread gate.
fn metas_at(st: &State, indices: &[usize]) -> Result<(Vec<Meta>, f64), FleaError> {
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    if super::iomount::is_remote(&st.base, &body) {
        let rows = meta::window_rows_at(&st.listing, indices);
        let threaded = st.listing.threaded_cached_with(&body);
        let base = st.base.clone();
        super::iomount::call(&base.clone(), &body, "filterwindow", move || meta::stat_window_rows(&base, &rows, threaded))
    } else {
        Ok(meta::stat_at(&st.base, &st.listing, indices, &body))
    }
}
