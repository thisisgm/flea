use super::*;
use super::test_support::{counts, snapshot, Fixture, ADDS, CLOSES_INLINE, FIELDS, MISSES, OPENS, REMOVES, US_PER_SECOND};
use crate::backend::searchindex::fixture_tests;

const PROBE_ROWS: usize = 100_000;

#[test]
#[ignore]
fn search_fs3ra_typed_100k_probe() {
    let root = Fixture::new();
    let (index, _) = fixture_tests::synthetic(PROBE_ROWS, false);
    // Materialize the synthetic inventory before arming watches. Every inode is inside this fixture.
    for row in 0..index.len() {
        let path = root.path().join(index.path(row));
        if index.is_dir(row) { std::fs::create_dir_all(path).unwrap(); }
        else { std::fs::write(path, "fixture\n").unwrap(); }
    }
    let scope = root.path().to_str().unwrap();
    let mut session = super::super::searchsession::Session::open_sync(scope, scope, false);
    *session.index.lock().unwrap() = index;
    let counters = counts(&session.row_cache);
    let mut gen = 0;
    for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"] {
        for end in 1..=query.len() {
            gen += 1;
            let prefix = &query[..end];
            let before = snapshot(&counters);
            let start = Instant::now();
            let answer = session.query(gen, prefix, ROW_CAP).unwrap();
            let mut listing = Listing::new();
            for (path, directory) in &answer.rows { listing.push(path, *directory); }
            let row_start = Instant::now();
            session.row_cache.window(root.path(), &listing, 0, ROW_CAP, "");
            let row_us = row_start.elapsed().as_secs_f64() * US_PER_SECOND;
            let full_us = start.elapsed().as_secs_f64() * US_PER_SECOND;
            let after = snapshot(&counters);
            let delta: [usize; FIELDS] = std::array::from_fn(|i| after[i] - before[i]);
            println!("FS3RA_PREFIX prefix={prefix:?} total={} rows={} opens={} closes_inline={} adds={} removes={} misses={} row_us={row_us:.3} full_us={full_us:.3}",
                answer.total, answer.rows.len(), delta[OPENS], delta[CLOSES_INLINE], delta[ADDS], delta[REMOVES], delta[MISSES]);
        }
    }
}
