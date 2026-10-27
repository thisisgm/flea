// Viewport size requests, worker scheduling and generation-checked result publication.
use crate::backend::dirsizeworker::Done;
use crate::backend::proto::dirsized_line;
use crate::backend::state::State;
use std::io::Write;

// Answered rows are re-answered at once, matching thumb's own cache-hit shape; only a directory can be asked for.
pub fn queue_dirsizes(out: &mut impl Write, st: &mut State, rows: &[usize]) {
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

// The event loop submits one bounded batch; cancellation never adds another worker.
pub fn start_next(st: &mut State) {
    if st.dirsize_worker.busy() || st.dirsize_queue.is_empty() {
        return;
    }
    let queued = std::mem::take(&mut st.dirsize_queue);
    let rows: Vec<_> = queued.into_iter()
        .filter(|&row| row < st.listing.len() && st.listing.is_dir(row))
        .map(|row| (row, st.base.join(st.listing.name(row))))
        .collect();
    st.dirsize_worker.start(rows);
}

// corner: a folder the sort budget cut off seeds nothing, so the worker still walks it properly.
pub fn seed_answered(st: &mut State, sized: &[Option<crate::backend::dirsize::DirSize>]) {
    for (row, size) in sized.iter().enumerate() {
        if let Some(size) = size {
            if !size.partial {
                st.dirsizes.insert(row, (size.bytes, size.partial));
            }
        }
    }
}

pub fn report_done(out: &mut impl Write, st: &mut State, done: Done) {
    if !st.dirsize_worker.accept(&done) { return; }
    let result = done.result;
    st.dirsizes.insert(done.row, (result.bytes, result.partial));
    crate::backend::answersizes::learn(st, done.row, result);
    writeln!(out, "{}", dirsized_line(done.row, result.bytes, result.partial, done.ms)).ok();
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{events::Event, listing::Listing, testdir::TestDir};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    fn state(base: PathBuf, events: std::sync::mpsc::Sender<Event>) -> State {
        let mut listing = Listing::new();
        listing.push("a", true);
        listing.push("b", true);
        listing.push("file", false);
        State {
            listing,
            base,
            asked: Vec::new(),
            outstanding: 0,
            deferred: Default::default(),
            window_meta: HashMap::new(),
            dirsizes: HashMap::new(),
            dirsize_queue: Vec::new(),
            dirsize_worker: crate::backend::dirsizeworker::Worker::new(events),
            search: None,
            search_session: None,
            search_reported: Instant::now(),
            generation: 0,
            held: crate::backend::state::Held::List,
            clip_watching: false, filter: None,
        }
    }

    #[test]
    fn a_floor_is_not_seeded_so_the_worker_still_walks_that_folder() {
        use crate::backend::dirsize::DirSize;
        let sandbox = TestDir::new("dirsize-seed");
        sandbox.dir("a");
        sandbox.dir("b");
        let (events, _rx) = channel();
        let mut st = state(sandbox.path().to_path_buf(), events);
        let sized = vec![
            Some(DirSize { bytes: 40, partial: false }),
            Some(DirSize { bytes: 9, partial: true }),
            None,
        ];
        seed_answered(&mut st, &sized);
        assert_eq!(st.dirsizes.get(&0), Some(&(40, false)), "a completed walk answers from the cache");
        assert_eq!(st.dirsizes.get(&1), None, "a floor seeds nothing, so the worker walks that row");
        assert_eq!(st.dirsizes.get(&2), None, "a file row seeds nothing");
        assert_eq!(st.dirsizes.len(), 1);
    }

    #[test]
    fn start_next_drains_unique_current_directories_into_one_batch() {
        let sandbox = TestDir::new("dirsize-queue");
        sandbox.dir("a");
        sandbox.dir("b");
        let (events, rx) = channel();
        let mut st = state(sandbox.path().to_path_buf(), events);
        st.dirsize_queue.extend([0, 0, 1, 2]);
        start_next(&mut st);
        assert!(st.dirsize_queue.is_empty());
        assert!(st.dirsize_worker.busy());

        let mut out = Vec::new();
        for (index, row) in [0, 1].into_iter().enumerate() {
            let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
            assert_eq!(done.row, row);
            report_done(&mut out, &mut st, done);
            assert_eq!(st.dirsize_worker.busy(), index == 0);
        }
        assert_eq!(st.dirsizes.len(), 2);
    }
}
