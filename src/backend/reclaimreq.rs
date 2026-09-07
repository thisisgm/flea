// The wire side of a reclaim: the loop hands it a slice of walking and it decides what to say,
// the way searchreq answers search and dirsizereq answers dirsize.
use crate::backend::proto::{reclaimed_line, reclaiming_line};
use crate::backend::run::{forget_rows, since};
use crate::backend::state::State;
use crate::backend::thumbs::Pool;
use std::io::{self, BufWriter, Write};
use std::time::{Duration, Instant};

// A running reclaim announces its counts no more often than this, so a fast walk cannot flood
// the client's parser, the same throttle a search's stream runs on.
const RECLAIM_REPORT: Duration = Duration::from_millis(100);

// One tick of the walk: a discovery slice, or one sized match in the Size phase.
pub fn step_reclaim(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool) {
    let done = match st.reclaim.as_mut() {
        Some(r) => r.step(&mut st.listing),
        None => return,
    };
    if done {
        end_reclaim(out, st, pool, false);
        return;
    }
    if st.reclaim_reported.elapsed() >= RECLAIM_REPORT {
        st.reclaim_reported = Instant::now();
        let (scanned, bytes, ms) = match st.reclaim.as_ref() {
            Some(r) => (r.scanned, r.bytes, since(r.started)),
            None => return,
        };
        writeln!(out, "{}", reclaiming_line(st.listing.len(), scanned, bytes, ms)).ok();
        out.flush().ok();
    }
}

// The terminal line, and the cache seeding only the caller of a finished walk can do honestly.
// The rows are ranked before the line goes out, so every row index the client is still holding
// names a different file the moment this arrives, the same rule a search's terminal line follows.
// A cancelled walk keeps whatever it found, in discovery order when its sizes never covered every
// row, and the client's own dirsize requests size the rest on demand. Seeds the dirsize cache in
// row order afterwards, so a settled row's Size answer comes from the cache instead of walking
// again a tree this walk already paid for.
pub fn end_reclaim(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool, cancelled: bool) -> bool {
    let mut r = match st.reclaim.take() {
        Some(r) => r,
        None => return false,
    };
    let reordered = r.rank(&mut st.listing);
    let ms = since(r.started);
    writeln!(out, "{}", reclaimed_line(st.listing.len(), r.scanned, r.bytes, ms, cancelled)).ok();
    out.flush().ok();
    if reordered {
        forget_rows(st, pool);
    }
    for (row, &(bytes, partial)) in r.sizes().iter().enumerate() {
        st.dirsizes.insert(row, (bytes, partial));
    }
    // From here every window's directory rows carry these bytes in s, until the next list or
    // search replaces the listing with one the walk did not measure.
    st.reclaim_sizes = true;
    reordered
}
