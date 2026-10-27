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
