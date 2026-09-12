// Viewport size requests, worker scheduling and generation-checked result publication.
use crate::backend::dirsizeworker::Done;
use crate::backend::proto::dirsized_line;
use crate::backend::state::State;
use std::io::{self, BufWriter, Write};

// Answered rows are re-answered at once, matching thumb's own cache-hit shape; only a directory can be asked for.
pub fn queue_dirsizes(out: &mut BufWriter<io::Stdout>, st: &mut State, rows: &[usize]) {
    for &row in rows {
        if row >= st.listing.len() || !st.listing.is_dir(row) {
            continue;
        }
        if let Some(&(bytes, partial)) = st.dirsizes.get(&row) {
            writeln!(out, "{}", dirsized_line(row, bytes, partial, 0.0)).ok();
            continue;
        }
        if st.dirsize_queue.contains(&row) || st.dirsize_worker.contains(row) {
            continue;
        }
        st.dirsize_queue.push(row);
    }
    out.flush().ok();
}

// The event loop submits at most one job; cancellation never adds another worker.
pub fn start_next(st: &mut State) {
    while !st.dirsize_worker.busy() && !st.dirsize_queue.is_empty() {
        let row = st.dirsize_queue.remove(0);
        if row < st.listing.len() {
            st.dirsize_worker.start(row, st.base.join(st.listing.name(row)));
        }
    }
}

pub fn report_done(out: &mut impl Write, st: &mut State, done: Done) {
    if !st.dirsize_worker.accept(&done) { return; }
    let result = done.result;
    st.dirsizes.insert(done.row, (result.bytes, result.partial));
    writeln!(out, "{}", dirsized_line(done.row, result.bytes, result.partial, done.ms)).ok();
    out.flush().ok();
}
