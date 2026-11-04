use super::*;
use super::super::{fixture_tests, score_cached, QueryScratch};
use crate::backend::searchmatch::Matcher;
use crate::backend::searchprior::alloc_tests::count_allocations;
use crate::backend::searchsession::FIRST_WINDOW_DEFAULT;
use crate::backend::sort::name_order;
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

const QUERIES: [&str; 5] = ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg", "archive/2024/client/reports/quarterly"];
// std's selection measured 1.9 to 2.4 compares per hit on the cut fixture; 8 leaves room for its worst-case fallback.
const SELECT_COMPARES_PER_HIT: usize = 8;
// The head sort is a merge sort, at most two compares per head row per level.
const HEAD_SORT_COMPARES_PER_LEVEL: usize = 2;
// Only the rest split and the head sort's buffer may allocate.
const CUT_ALLOCATIONS: usize = 2;
// Knuth's MMIX multiplier, a full-period 64-bit LCG for a reproducible shuffle.
const LCG_MULTIPLIER: u64 = 6364136223846793005;
// FNV-1a 64-bit, so a probe head prints as one comparable word.
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

// Q2's head: a full sort by score, path length, then the natural order of each built path.
fn q2_head(index: &Index, paths: &[Vec<u8>], hits: &[(i32, usize)], window: usize) -> Vec<(i32, usize)> {
    let mut sorted = hits.to_vec();
    sorted.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| index.path_len(a.1).cmp(&index.path_len(b.1)))
        .then_with(|| name_order(&paths[a.1], &paths[b.1])));
    sorted.truncate(window);
    sorted
}

fn assert_heads_match_q2(count: usize) {
    let (index, _) = fixture_tests::synthetic(count, false);
    let rows: Vec<_> = (0..index.len()).collect();
    let paths: Vec<_> = rows.iter().map(|&row| index.raw(row)).collect();
    let mut scratch = QueryScratch::default();
    for query in QUERIES {
        for end in 1..=query.len() {
            let text = &query[..end];
            let mut hits = score_cached(&Matcher::new(text), &rows, &index, &mut scratch);
            let window = hits.len().min(FIRST_WINDOW_DEFAULT);
            let expected = q2_head(&index, &paths, &hits, window);
            select_head(&mut hits, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
            assert_eq!(hits, expected, "{count} rows, query {text:?}");
        }
    }
}

#[test]
fn search_q3_heads_match_q2_at_100k() { assert_heads_match_q2(100_000); }

#[test]
#[ignore]
fn search_q3_heads_match_q2_at_1m() { assert_heads_match_q2(1_000_000); }

#[test]
fn search_q3_path_comparisons_match_raw_order_without_allocations() {
    let mut index = Index::new();
    for raw in [b"P01/z".as_slice(), b"p1/a", b"p1/z", b"P1/Z", b"p00/x", b"p0/x", b"a//x", b"x2/ab", b"x10/a",
        b"a/1", b"a/01", b"a/10", b"a/2", b"a/00000000000000000000000000000000000000002",
        b"a/10000000000000000000000000000000000000000", b"raw\xff/CAF\xe9", b"raw\xfe/caf\xe8", "é/10".as_bytes(), "é/2".as_bytes(), "É/3".as_bytes()] {
        assert!(index.push(OsStr::from_bytes(raw), false));
    }
    assert!(index.push(format!("{}/leaf", "deep/".repeat(80)), false));
    let mut expected = Vec::new();
    for a in 0..index.len() {
        for b in 0..index.len() { expected.push(name_order(&index.raw(a), &index.raw(b))); }
    }
    index.reconstructed.store(0, Ordering::Relaxed);
    let (_, allocations) = count_allocations(|| {
        for a in 0..index.len() {
            for b in 0..index.len() { assert_eq!(path_order(&index, a, b), expected[a * index.len() + b], "rows {a} {b}"); }
        }
    });
    println!("Q3_COMPARE rows={} allocations={allocations} raw={}", index.len(), index.reconstructed.load(Ordering::Relaxed));
    assert_eq!(index.reconstructed.load(Ordering::Relaxed), 0, "comparisons borrow bytes, never build paths");
    assert_eq!(allocations, 0, "borrowed comparisons allocate nothing");
}

#[test]
fn search_q3_cut_has_no_path_builds_and_linear_comparison_bound() {
    let mut index = Index::new();
    let count = 100_000;
    let window = FIRST_WINDOW_DEFAULT;
    let folders = 100;
    for row in 0..count { assert!(index.push(format!("d{:03}/f{row:06}.rs", row % folders), false)); }
    let mut expected: Vec<_> = (0..count).collect();
    expected.sort_unstable_by_key(|row| (row % folders, *row));
    let score = 7;
    let mut seed = 93u64;
    for variant in ["ascending", "descending", "shuffled"] {
        let mut hits: Vec<_> = (0..count).map(|row| (score, row)).collect();
        if variant == "descending" { hits.reverse(); }
        if variant == "shuffled" {
            for at in (1..hits.len()).rev() {
                seed = seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(1);
                hits.swap(at, (seed >> u32::BITS) as usize % (at + 1));
            }
        }
        index.reconstructed.store(0, Ordering::Relaxed);
        let compares = Cell::new(0usize);
        // One thread: a split length pass allocates its threads, which this gate does not count.
        CUT_THREADS_PINNED.with(|pinned| pinned.set(Some(1)));
        let (rest, allocations) = count_allocations(|| select_head(&mut hits, window,
            |hit| index.path_len(hit.1),
            |a, b| { compares.set(compares.get() + 1); rank_cmp(&index, a, b) }));
        let levels = (usize::BITS - window.leading_zeros()) as usize;
        let bound = SELECT_COMPARES_PER_HIT * count + HEAD_SORT_COMPARES_PER_LEVEL * window * levels;
        println!("Q3_CUT variant={variant} compares={} bound={bound} raw={} allocations={allocations}", compares.get(), index.reconstructed.load(Ordering::Relaxed));
        assert_eq!(index.reconstructed.load(Ordering::Relaxed), 0, "zero raw calls throughout cut and head sort");
        assert!(compares.get() <= bound, "bounded selection comparisons");
        assert!(allocations <= CUT_ALLOCATIONS, "only rest and head-sort storage may allocate: {allocations}");
        assert_eq!(hits.iter().map(|hit| hit.1).collect::<Vec<_>>(), expected[..window]);
        let mut actual: Vec<_> = hits.iter().chain(&rest).map(|hit| hit.1).collect();
        actual.sort_unstable();
        assert_eq!(actual, (0..count).collect::<Vec<_>>());
    }
}

#[test]
fn search_cut_keys_only_the_rows_tied_at_the_heads_last_score() {
    // 400 scores of 250 rows each: the head holds all 250 rows at the top score and 100 of the 250 at the next.
    let mut index = Index::new();
    let (count, scores, window) = (100_000, 400, FIRST_WINDOW_DEFAULT);
    for row in 0..count { assert!(index.push(format!("d{:03}/f{row:06}.rs", row % 100), false)); }
    let mut hits: Vec<_> = (0..count).map(|row| ((row % scores) as i32, row)).collect();
    let mut expected = hits.clone();
    expected.sort_by(|a, b| rank_cmp(&index, a, b));
    let last = expected[window - 1].0;
    assert_eq!(expected[window].0, last, "the window splits the rows of its last score, so the tied select runs");
    let (keyed_tied, keyed_elsewhere) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let rest = select_head(&mut hits, window, |hit| {
        let keyed = if hit.0 == last { &keyed_tied } else { &keyed_elsewhere };
        keyed.fetch_add(1, Ordering::Relaxed);
        index.path_len(hit.1)
    }, |a, b| rank_cmp(&index, a, b));
    assert!(keyed_tied.load(Ordering::Relaxed) > 0, "the rows tied at the head's last score were keyed");
    assert_eq!(hits, expected[..window], "the head is the first window of the full rank order");
    let mut actual: Vec<_> = hits.iter().chain(&rest).copied().collect();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected, "the head and the rest hold every hit once");
    assert_eq!(keyed_elsewhere.load(Ordering::Relaxed), 0, "only rows tied at the head's last score need a path length");
}

#[test]
fn search_cut_orders_tied_rows_past_the_counted_lengths_by_the_full_comparator() {
    // Paths both sides of the counted lengths, tied on score, with the cut landing among the longest.
    let mut index = Index::new();
    let window = FIRST_WINDOW_DEFAULT;
    let count = 4 * window;
    for row in 0..count {
        let depth = if row % 8 == 0 { 1 } else { 110 + row % 7 };
        assert!(index.push(format!("{}f{row:05}", "abcdefghi/".repeat(depth)), false));
    }
    for score in [7, -1] {
        let mut hits: Vec<_> = (0..count).map(|row| (score, row)).collect();
        let mut expected = hits.clone();
        expected.sort_by(|a, b| rank_cmp(&index, a, b));
        assert!(index.path_len(expected[window].1) > CUT_LENGTHS, "the cut lands among the longest rows");
        let rest = select_head(&mut hits, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
        assert_eq!(hits, expected[..window], "score {score}");
        let mut actual: Vec<_> = hits.iter().chain(&rest).copied().collect();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "the head and the rest hold every hit once");
    }
}

// A tie large enough to split its length pass across threads selects the same head and rest as one thread.
#[test]
fn search_fs4_split_length_pass_selects_as_one_thread() {
    let mut index = Index::new();
    let window = FIRST_WINDOW_DEFAULT;
    let count = 3 * PARALLEL_CUT_AT + 7;
    for row in 0..count {
        let name = if row % 5 == 0 { format!("caf\u{e9}_{row}") } else { format!("f{row}") };
        assert!(index.push(format!("{}/{name}", "d/".repeat(row % 13)), false));
    }
    let mut seed = 17u64;
    let mut hits: Vec<_> = (0..count).map(|row| (i32::from(row % 3 != 0), row)).collect();
    for at in (1..hits.len()).rev() {
        seed = seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(1);
        hits.swap(at, (seed >> u32::BITS) as usize % (at + 1));
    }
    let mut results = Vec::new();
    for threads in [1, 4, 5] {
        CUT_THREADS_PINNED.with(|pinned| pinned.set(Some(threads)));
        let mut head = hits.clone();
        let mut rest = select_head(&mut head, window, |hit| index.path_len(hit.1), |a, b| rank_cmp(&index, a, b));
        rest.sort_unstable();
        results.push((head, rest));
    }
    CUT_THREADS_PINNED.with(|pinned| pinned.set(None));
    let mut expected = hits.clone();
    expected.sort_by(|a, b| rank_cmp(&index, a, b));
    assert_eq!(results[0].0, expected[..window], "one thread selects the rank order's first window");
    assert!(results.iter().all(|result| *result == results[0]), "every thread count selects the same head and the same rest");
}

#[path = "searchphases_probe.rs"]
mod phases;
