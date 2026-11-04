use super::*;
use super::test_support::{counts, listing, snapshot, Fixture, ADDS, CLOSES_INLINE, MISSES, REMOVES, US_PER_SECOND};
use std::sync::{atomic::Ordering, mpsc};
use std::time::Duration;

// More directories than WATCH_CAP, so one row from each overflows it.
const DIRECTORIES: usize = 450;
const FILES_PER_DIRECTORY: usize = 4;
// Bounds a background close, so a hung close fails the case instead of hanging the suite.
const CLOSE_WAIT: Duration = Duration::from_secs(30);

#[test]
fn search_fs3ra_typed_windows_keep_both_descriptors_and_only_change_watch_differences() {
    let root = Fixture::new();
    let mut names = Vec::new();
    for dir in 0..DIRECTORIES {
        root.dir(&format!("dir{dir:03}"));
        for file in 0..FILES_PER_DIRECTORY {
            let name = format!("dir{dir:03}/file{file}");
            root.file(&name, "row");
            names.push(name);
        }
    }
    let make = |names: &[String]| listing(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let normal = make(&names);
    let sparse = make(&names.iter().step_by(FILES_PER_DIRECTORY).take(ROW_CAP).cloned().collect::<Vec<_>>());
    let mut cache = Cache::default();
    let counters = counts(&cache);
    let mut failures = Vec::new();
    // A partial first window, a half window, and a window that shares no row with the earlier ones.
    let (partial, half, disjoint) = (200, ROW_CAP / 2, 800);
    for (step, rows, start, count) in [
        ("first", &normal, 0, partial), ("grow", &normal, 0, ROW_CAP),
        ("shrink", &normal, 0, half), ("disjoint", &normal, disjoint, ROW_CAP),
        ("over-cap", &sparse, 0, ROW_CAP), ("zero", &normal, 0, 0),
        ("back", &normal, 0, ROW_CAP),
    ] {
        let old_dirs: HashSet<_> = cache.watches.keys().cloned().collect();
        let old_rows = cache.requested.clone();
        let before = snapshot(&counters);
        // A window that moves past ARM_LIMIT reads cold with no watch call, so each step asks twice to arm.
        cache.window(root.path(), rows, start, count, "");
        cache.window(root.path(), rows, start, count, "");
        let after = snapshot(&counters);
        if !matches!(step, "over-cap" | "zero") {
            assert_eq!(cache.requested.len(), count);
            assert_eq!(cache.entries.len(), count);
            assert!(!cache.watches.is_empty());
        }
        let new_dirs: HashSet<_> = cache.watches.keys().cloned().collect();
        let difference = old_dirs.symmetric_difference(&new_dirs).count()
            + old_rows.symmetric_difference(&cache.requested).count();
        let calls = after[ADDS] - before[ADDS] + after[REMOVES] - before[REMOVES];
        println!("FS3RA_STEP step={step} changes={difference} adds={} removes={} misses={} inline={}",
            after[ADDS] - before[ADDS], after[REMOVES] - before[REMOVES], after[MISSES] - before[MISSES], after[CLOSES_INLINE]);
        if calls > difference { failures.push(format!("{step}: {calls} watch calls > {difference} changed paths")); }
        if after[CLOSES_INLINE] != 0 { failures.push(format!("{step}: {} inline closes", after[CLOSES_INLINE])); }
        for (kind, counter) in ["directory", "inode"].into_iter().zip(&counters) {
            if counter.opens.load(Ordering::Relaxed) > 1 { failures.push(format!("{step}: {kind} reopened")); }
        }
        if step == "over-cap" || step == "zero" {
            assert!(cache.watches.is_empty() && cache.requested.is_empty() && cache.entries.is_empty());
            assert_eq!(cache.inodes.len(), 0);
            assert_eq!(cache.bytes, 0);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[test]
fn search_fs3ra_own_removal_preserves_intersecting_rows_through_the_next_drain() {
    let root = Fixture::new();
    for dir in ["a", "b"] { root.dir(dir); root.file(&format!("{dir}/row"), "row"); }
    let mut cache = Cache::default();
    let counters = counts(&cache);
    cache.window(root.path(), &listing(&["a/row", "b/row"]), 0, 2, "");
    let before = snapshot(&counters);
    cache.window(root.path(), &listing(&["a/row"]), 0, 1, "");
    cache.window(root.path(), &listing(&["a/row"]), 0, 1, "");
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES], "own IN_IGNORED must preserve the carried row");
    assert_eq!(snapshot(&counters)[CLOSES_INLINE], 0);
}

#[test]
fn search_fs3ra_session_drop_closes_both_descriptors_on_background_threads() {
    let root = Fixture::new();
    root.file("row", "row");
    let mut cache = Cache::default();
    let counters = counts(&cache);
    cache.window(root.path(), &listing(&["row"]), 0, 1, "");
    let receivers: Vec<_> = counters.iter().map(|counter| {
        let (tx, rx) = mpsc::channel();
        *counter.closed.lock().unwrap() = Some(tx);
        rx
    }).collect();
    let caller = std::thread::current().id();
    let start = Instant::now();
    drop(cache);
    let elapsed = start.elapsed();
    let reports: Vec<_> = receivers.into_iter().map(|rx| rx.recv_timeout(CLOSE_WAIT).unwrap()).collect();
    println!("FS3RA_DROP caller_us={:.3} directory_close_us={:.3} inode_close_us={:.3}",
        elapsed.as_secs_f64() * US_PER_SECOND, reports[0].1.as_secs_f64() * US_PER_SECOND, reports[1].1.as_secs_f64() * US_PER_SECOND);
    assert!(reports.iter().all(|(thread, _)| *thread != caller), "channel is sent only after the actual close");
    for counter in counters {
        assert_eq!(counter.closes_inline.load(Ordering::Relaxed), 0);
        assert_eq!(counter.closes_background.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn search_t11_a_window_moved_past_the_arm_limit_reads_cold_and_leaves_the_armed_rows_to_the_next_warm_window() {
    let root = Fixture::new();
    let names: Vec<String> = (0..120).map(|row| format!("row{row:03}")).collect();
    for name in &names { root.file(name, "row"); }
    let rows = listing(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let mut cache = Cache::default();
    let counters = counts(&cache);
    cache.window(root.path(), &rows, 0, 40, "");
    cache.window(root.path(), &rows, 0, 40, "");
    assert_eq!(cache.entries.len(), 40, "the window asked twice arms");
    // Rows 20..60 move 20 rows on and 20 off, past ARM_LIMIT; an armed row written first is read fresh, never from its entry.
    let before_cold = "written before the cold read";
    root.file("row035", before_cold);
    let before = snapshot(&counters);
    let metas = cache.window(root.path(), &rows, 20, 40, "").0;
    let after = snapshot(&counters);
    assert_eq!(metas.len(), 40);
    assert_eq!(metas[15].size, before_cold.len() as u64, "a cold window reads every row");
    assert_eq!(after[ADDS] + after[REMOVES], before[ADDS] + before[REMOVES], "a cold window makes no watch call");
    assert_eq!(after[MISSES] - before[MISSES], 40, "a cold window reads every row in one pass");
    assert_eq!(cache.entries.len(), 40, "a cold window keeps the armed rows and caches none of the new ones");
    let written = "written after the cold read";
    root.file("row030", written);
    let metas = cache.window(root.path(), &rows, 20, 40, "").0;
    assert_eq!(metas[10].size, written.len() as u64, "an armed row written after a cold window is fresh in the next warm one");
    assert_eq!(metas[15].size, before_cold.len() as u64, "an armed row written before a cold window is fresh in the next warm one");
    assert!(cache.requested.contains(&root.join("row059")), "the same window asked again arms");
    // A small move arms at once.
    let before = snapshot(&counters);
    cache.window(root.path(), &rows, 25, 40, "");
    let after = snapshot(&counters);
    assert!(after[ADDS] > before[ADDS] && cache.requested.contains(&root.join("row064")), "a move within ARM_LIMIT arms at once");
    assert_eq!(after[MISSES] - before[MISSES], 5);
}

#[test]
fn search_t11_a_window_close_to_a_cold_one_arms_however_far_it_is_from_the_armed_rows() {
    let root = Fixture::new();
    let names: Vec<String> = (0..120).map(|row| format!("row{row:03}")).collect();
    for name in &names { root.file(name, "row"); }
    let rows = listing(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let mut cache = Cache::default();
    let counters = counts(&cache);
    cache.window(root.path(), &rows, 0, 40, "");
    cache.window(root.path(), &rows, 0, 40, "");
    let before = snapshot(&counters);
    cache.window(root.path(), &rows, 60, 40, "");
    assert_eq!(snapshot(&counters)[ADDS], before[ADDS], "a window far from the last one reads cold");
    // A steady answer after a cold one: rows 61..101 differ from 60..100 by two, so they arm though none of them was armed.
    cache.window(root.path(), &rows, 61, 40, "");
    assert!(snapshot(&counters)[ADDS] > before[ADDS], "a window close to the last one arms");
    assert!(cache.requested.contains(&root.join("row100")) && !cache.requested.contains(&root.join("row000")));
    let before = snapshot(&counters);
    cache.window(root.path(), &rows, 61, 40, "");
    assert_eq!(snapshot(&counters)[MISSES], before[MISSES], "the steady answer reuses every row");
}

#[test]
fn search_t11_a_small_window_reads_cold_instead_of_unarming_a_bigger_armed_set() {
    let root = Fixture::new();
    let names: Vec<String> = (0..120).map(|row| format!("row{row:03}")).collect();
    for name in &names { root.file(name, "row"); }
    let rows = listing(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let mut cache = Cache::default();
    let counters = counts(&cache);
    cache.window(root.path(), &rows, 0, 80, "");
    cache.window(root.path(), &rows, 0, 80, "");
    assert_eq!(cache.requested.len(), 80);
    let before = snapshot(&counters);
    for _ in 0..3 {
        assert_eq!(cache.window(root.path(), &rows, 0, 10, "").0.len(), 10);
    }
    let after = snapshot(&counters);
    assert_eq!(after[ADDS] + after[REMOVES], before[ADDS] + before[REMOVES], "a ten-row window never unarms seventy rows");
    assert_eq!(after[MISSES] - before[MISSES], 30, "each ask reads its ten rows");
    // A window of more than ARM_LIMIT rows close to the last one still arms.
    cache.window(root.path(), &rows, 0, 40, "");
    cache.window(root.path(), &rows, 0, 40, "");
    assert_eq!(cache.requested.len(), 40);
}
