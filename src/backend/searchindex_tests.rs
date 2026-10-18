use super::*;
use crate::backend::searchwalk::{walk, WalkProbes};
use crate::backend::testdir::TestDir;

// Hard rule 9: every path below is inside a sandbox TestDir made and removes itself.
fn root(d: &TestDir) -> &std::path::Path {
    d.path()
}

#[test]
fn a_local_walk_indexes_every_row_with_one_mountinfo_read() {
    crate::backend::mountinfo::test_reset();
    let d = TestDir::new("sessionwalk");
    d.dir("tools/benches");
    d.file("tools/benches/bench-run.sh", "");
    d.file("unrelated.txt", "");
    let body = crate::backend::mountinfo::read_body();
    let probes = WalkProbes::new();
    let (index, dirs) = walk(root(&d), false, false, &body, &probes);
    assert_eq!(dirs, 3, "the scope, tools and tools/benches over 3 directories walked");
    assert_eq!(index.len(), 4, "4 rows indexed over 3 directories walked");
    assert_eq!(crate::backend::mountinfo::test_reads(), 1, "1 mountinfo read over 3 directories walked");
}

#[test]
fn a_local_walk_takes_no_bound_call() {
    crate::backend::iomount::test_reset();
    let d = TestDir::new("sessionbound");
    d.dir("sub");
    d.file("sub/bench.txt", "");
    let body = crate::backend::mountinfo::read_body();
    let probes = WalkProbes::new();
    let (index, _) = walk(root(&d), false, false, &body, &probes);
    assert_eq!(index.len(), 2, "2 rows indexed over 1 directory walked");
    assert_eq!(crate::backend::iomount::test_calls(), 0, "0 bound calls over 1 directory walked");
}

// A name that changed kind between two patches is a new entry: a folder now a file keeps no rows below it, a file now a folder is walked.
#[test]
fn a_patch_follows_a_name_that_changed_kind() {
    let d = TestDir::new("sessionkind");
    d.file("was-dir", "");
    d.dir("was-file");
    d.file("was-file/new.txt", "");
    d.dir("same");
    d.file("same/in.txt", "");
    let mut index = Index::new();
    for (path, is_dir) in [("was-dir", true), ("was-dir/inner.txt", false), ("was-file", false), ("same", true), ("same/in.txt", false)] {
        index.push(path, is_dir);
    }
    assert!(index.patch_dir(root(&d), std::path::Path::new(""), false, &crate::backend::mountinfo::read_body()), "a local scope answers its patch");
    let mut rows: Vec<(String, bool)> = (0..index.len()).map(|row| (index.path(row), index.is_dir(row))).collect();
    rows.sort();
    assert_eq!(rows, [("same", true), ("same/in.txt", false), ("was-dir", false), ("was-file", true), ("was-file/new.txt", false)].map(|(path, kind)| (path.to_string(), kind)));
}

// A patch that drops two children ahead of a deeper row keeps every surviving row and drops only the gone ones.
#[test]
fn a_patch_dropping_two_children_keeps_the_rows_after_them() {
    let d = TestDir::new("sessionpatch");
    d.file("c.txt", "");
    d.dir("sub");
    d.file("sub/x.txt", "");
    let mut index = Index::new();
    for (path, is_dir) in [("a.txt", false), ("b.txt", false), ("c.txt", false), ("sub", true), ("sub/x.txt", false)] {
        index.push(path, is_dir);
    }
    assert!(index.patch_dir(root(&d), std::path::Path::new(""), false, &crate::backend::mountinfo::read_body()), "a local scope answers its patch");
    let mut paths: Vec<String> = (0..index.len()).map(|row| index.path(row)).collect();
    paths.sort();
    assert_eq!(paths, ["c.txt", "sub", "sub/x.txt"], "a.txt and b.txt gone, sub/x.txt kept");
}

#[test]
fn an_index_reports_checked_capacity_refusal_without_losing_prior_identities() {
    let mut index = Index::new();
    index.row_limit = 2;
    assert!(index.push("a", false));
    assert!(index.push("b", true));
    assert!(!index.push("c", false));
    assert_eq!(index.len(), 2);
    assert_eq!(index.partial, Some("capacity"));
    assert_eq!(index.raw(0), b"a");
    assert_eq!(index.raw(1), b"b");
    assert!(index.is_dir(1));
}

// Diagnostic only, never a gate: one full-index score over 100k synthetic realistic paths prints its ms.
#[test]
fn full_index_score_prints_its_ms() {
    let mut index = Index::new();
    for i in 0..100_000 {
        index.push(
            format!("some_directory_name_{:03}/nested_subdir/report_final_draft_v{:05}_reviewed.txt", i % 200, i),
            false,
        );
    }
    assert_eq!(index.len(), 100_000, "100000 synthetic rows indexed");
    let fuzzy = crate::backend::searchmatch::Matcher::new("report");
    let rows: Vec<usize> = (0..index.len()).collect();
    let started = std::time::Instant::now();
    let hits = super::score_all(&fuzzy, &rows, &index);
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let bytes = super::index_bytes(&index);
    eprintln!("TIMING full-index score: {} rows scored, {} hits, {:.1} ms", rows.len(), hits.len(), ms);
    eprintln!("TIMING index: {} bytes over {} rows, {} bytes per entry", bytes, index.len(), bytes / index.len());
}

// Every row path in the arena, in arena order.
fn paths_of(s: &crate::backend::searchsession::Session) -> Vec<String> {
    let index = s.index.lock().unwrap();
    (0..index.len()).map(|row| index.path(row).to_string()).collect()
}

#[test]
fn a_patch_that_drops_two_rows_keeps_the_survivor_without_repacking() {
    let d = TestDir::new("patchdrop");
    let a = d.file("a.txt", "");
    d.file("b.txt", "");
    let c = d.file("c.txt", "");
    let scope = d.path().to_str().expect("utf-8").to_string();
    let mut s = crate::backend::searchsession::Session::open_sync(&scope, &scope, false);
    assert_eq!(s.indexed(), 3, "3 rows indexed over 1 directory walked");
    d.assert_contains(&a);
    std::fs::remove_file(&a).expect("test sandbox remove");
    d.assert_contains(&c);
    std::fs::remove_file(&c).expect("test sandbox remove");
    s.index.lock().unwrap().compactions = 0;
    assert!(s.patch_open_dir(d.path()), "the scope itself is under the scope");
    assert_eq!(paths_of(&s), vec!["b.txt".to_string()], "the survivor stays after 2 of 3 rows drop");
    assert_eq!(s.index.lock().unwrap().compactions, 0, "local deletion never repacks unrelated rows");
}

#[test]
fn a_patch_reads_each_row_a_bounded_number_of_times() {
    let d = TestDir::new("patchscan");
    d.dir("other");
    d.dir("inbox");
    for i in 0..2000 {
        d.file(&format!("other/f{:04}.txt", i), "");
    }
    let scope = d.path().to_str().expect("utf-8").to_string();
    let mut s = crate::backend::searchsession::Session::open_sync(&scope, &scope, false);
    let before = s.indexed();
    assert_eq!(before, 2002, "2 directories and 2000 files indexed");
    d.dir("inbox/moved");
    for i in 0..200 {
        d.file(&format!("inbox/moved/m{:03}.txt", i), "");
    }
    s.index.lock().unwrap().scanned.store(0, std::sync::atomic::Ordering::Relaxed);
    assert!(s.patch_after_write(&d.join("inbox/moved")), "the moved tree is under the scope");
    let after = s.indexed();
    assert_eq!(after, before + 201, "the moved directory and its 200 files land whole");
    let scanned = s.index.lock().unwrap().scanned.load(std::sync::atomic::Ordering::Relaxed);
    assert!(scanned <= 3 * after, "{} row reads for a {} row index, at most {}", scanned, after, 3 * after);
}

#[test]
fn a_patch_on_a_stuck_mount_reads_nothing_and_leaves_the_index() {
    let d = TestDir::new("patchstuck");
    d.file("bench.txt", "");
    let scope = d.path().to_str().expect("utf-8").to_string();
    let mut s = crate::backend::searchsession::Session::open_sync(&scope, &scope, false);
    let mount = crate::backend::iomount::mount_key(d.path(), &crate::backend::mountinfo::read_body());
    let _stuck = crate::backend::iomount::test_hold_stuck(mount);
    crate::backend::iomount::test_reset();
    let added = d.file("bench-stuck.txt", "");
    assert!(!s.patch_open_dir(d.path()), "a stuck mount patches nothing from the watch");
    assert!(!s.patch_after_write(&added), "a stuck mount patches nothing after a write");
    assert_eq!(paths_of(&s), vec!["bench.txt".to_string()], "the index stands while the mount is stuck");
    assert!(crate::backend::iomount::test_calls() >= 2, "each patch goes through the bound");
}

#[test]
fn an_invalid_scoring_candidate_panics_instead_of_losing_rows() {
    let mut index = Index::new();
    for i in 0..PARALLEL_SCORE_AT + 50 {
        index.push(format!("report_{:05}.txt", i), false);
    }
    let fuzzy = crate::backend::searchmatch::Matcher::new("report");
    let rows: Vec<usize> = (0..index.len()).chain([index.len() + 5]).collect();
    let scored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| super::score_all(&fuzzy, &rows, &index)));
    assert!(scored.is_err(), "a row past the arena panics the caller, never a short answer");
}

#[test]
fn search_one_file_delta_visits_only_its_block_and_invalidates_pending_ids() {
    let d = TestDir::new("blockdelta");
    d.dir("hot");
    d.file("hot/needle-old", "");
    let mut index = Index::new();
    for row in 0..100_000 { index.push(format!("unrelated/f{row:06}"), false); }
    index.push("hot", true);
    index.push("hot/needle-old", false);
    let old = d.join("hot/needle-old");
    d.assert_contains(&old);
    std::fs::remove_file(old).unwrap();
    d.file("hot/needle-new", "");
    let epoch = index.epoch;
    assert!(index.patch_dir(d.path(), Path::new("hot"), false, ""));
    assert_eq!(index.len(), 100_002);
    assert_eq!(index.scanned.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(index.compactions, 0);
    assert_ne!(index.epoch, epoch);
    assert_eq!(index.path(index.len() - 1), "hot/needle-new");
}

#[test]
fn search_name_reclamation_preserves_dense_ids_and_advances_the_epoch() {
    let d = TestDir::new("reclaimepoch");
    d.file("needle", "");
    let mut index = Index::new();
    index.push("gone", false);
    index.push("needle", false);
    index.retired_bytes = 1024 * 1024;
    assert!(index.patch_dir(d.path(), Path::new(""), false, ""));
    assert_eq!(index.path(0), "needle");
    assert_eq!(index.compactions, 1);
    assert_eq!(index.epoch, 2);
    assert_eq!(index.last_patch(), (&[(0usize, 1usize)][..], 1));
}

#[test]
fn search_fb1_removal_keeps_its_survivor_count() {
    let d = TestDir::new("fb1survivors");
    let gone = d.file("a", "");
    d.file("b", "");
    let mut index = Index::new();
    assert!(index.push("a", false));
    assert!(index.push("b", false));
    d.assert_contains(&gone);
    std::fs::remove_file(&gone).expect("remove sandbox entry");
    assert!(index.patch_dir(d.path(), Path::new(""), false, ""));
    assert_eq!(index.len(), 1);
    assert_eq!(index.raw(0), b"b");
    assert_eq!(index.last_patch().1, 1, "a removal must retain the surviving row population");
}

// The same probe runs on the predecessor and candidate, with synthetic cold blocks and one real changed block.
#[test]
#[ignore]
fn search_delta_scale_probe() {
    for entries in [100_000, 1_000_000] {
        let d = TestDir::new("deltaprobe");
        d.dir("hot");
        let old = d.file("hot/needle-old", "");
        let mut index = Index::new();
        for row in 0..entries { index.push(format!("cold{:04}/file{:04}.txt", row / 1000, row % 1000), false); }
        index.push("hot", true);
        index.push("hot/needle-old", false);
        index.finish();
        let before = index.bytes(true);
        let pss = || std::fs::read_to_string("/proc/self/smaps_rollup").unwrap().lines()
            .find_map(|line| line.strip_prefix("Pss:").and_then(|line| line.split_whitespace().next()).and_then(|word| word.parse::<usize>().ok())).unwrap();
        let pss_before = pss();
        d.assert_contains(&old);
        std::fs::remove_file(old).unwrap();
        d.file("hot/needle-new", "");
        let started = std::time::Instant::now();
        assert!(index.patch_dir(d.path(), Path::new("hot"), false, ""));
        let micros = started.elapsed().as_secs_f64() * 1e6;
        let pss_after = pss();
        assert_eq!(index.len(), entries + 2);
        println!("SEARCH_DELTA entries={entries} apply_us={micros:.3} reads={} compactions={} allocated_before={before} allocated_after={} pss_before_kib={pss_before} pss_after_kib={pss_after}",
            index.scanned.load(std::sync::atomic::Ordering::Relaxed), index.compactions, index.bytes(true));
    }
}

#[test]
fn search_large_subtree_removal_releases_its_consumed_move_log() {
    let d = TestDir::new("moveresidency");
    let mut index = Index::new();
    index.push("gone", true);
    for row in 0..10_000 { index.push(format!("gone/file{row:05}"), false); }
    assert!(index.patch_dir(d.path(), Path::new(""), false, ""));
    assert_eq!(index.len(), 0);
    assert_eq!(index.last_patch().0.len(), 10_001);
    index.release_patch();
    assert!(index.moves.is_empty());
    assert!(index.moves.capacity() <= 32);
}

#[test]
fn review_fb4_missing_directory_patch_removes_its_own_identity() {
    let d = TestDir::new("reviewmissingpatch");
    let mut index = Index::new();
    index.push("gone", true);
    index.push("gone/child", false);
    index.push("survivor", false);
    assert!(index.patch_dir(d.path(), Path::new("gone"), false, ""));
    let mut paths: Vec<_> = (0..index.len()).map(|row| index.path(row)).collect();
    paths.sort();
    assert_eq!(paths, vec!["survivor"], "a missing directory patch must remove both its row and descendants");
}

#[test]
fn search_missing_directory_patch_remaps_only_affected_blocks() {
    let d = TestDir::new("missingblockdelta");
    let mut index = Index::new();
    for row in 0..100_000 { index.push(format!("cold/f{row:06}"), false); }
    for (path, kind) in [("parent", true), ("parent/gone/first", false), ("parent/gone/sub", true),
                         ("parent/gone/sub/deep", false), ("parent/a", false), ("parent/b", false), ("parent/gone", true)] {
        assert!(index.push(path, kind));
    }
    let mut identities: Vec<_> = (0..index.len()).map(|row| (index.raw(row), index.is_dir(row))).collect();
    let epoch = index.epoch;
    assert!(index.patch_dir(d.path(), Path::new("parent/gone"), false, ""));
    assert_eq!(index.len(), 100_003);
    assert_eq!(index.last_patch().0.len(), 4, "three descendants and the moved directory row leave");
    for &(at, last) in index.last_patch().0 {
        assert_eq!(last, identities.len() - 1);
        identities.swap_remove(at);
    }
    assert_eq!(identities, (0..index.len()).map(|row| (index.raw(row), index.is_dir(row))).collect::<Vec<_>>());
    assert!(identities.iter().all(|(path, _)| !path.starts_with(b"parent/gone")));
    assert_eq!(index.last_patch().1, index.len());
    assert_eq!(index.epoch, epoch.wrapping_add(1));
    assert_eq!(index.scanned.load(std::sync::atomic::Ordering::Relaxed), 4);
    assert_eq!(index.compactions, 0);
    assert_eq!(index.partial, None);
}

#[test]
fn search_missing_directory_patch_handles_a_file_and_reclamation() {
    let d = TestDir::new("missingkinddelta");
    d.file("gone", "");
    let mut index = Index::new();
    index.push("gone", true);
    index.push("gone/child", false);
    index.push("survivor", false);
    index.retired_bytes = 1024 * 1024;
    assert!(index.patch_dir(d.path(), Path::new("gone"), false, ""));
    assert_eq!(index.len(), 1);
    assert_eq!(index.raw(0), b"survivor");
    assert!(!index.is_dir(0));
    assert_eq!(index.last_patch(), (&[(1usize, 2usize), (0, 1)][..], 1));
    assert_eq!(index.compactions, 1);
    assert_eq!(index.epoch, 2);
    assert_eq!(index.partial, None);
}

#[test]
fn search_missing_scope_patch_has_no_containing_identity() {
    let d = TestDir::new("missingscopedelta");
    let mut index = Index::new();
    index.push("gone", true);
    index.push("gone/child", false);
    assert!(index.patch_dir(&d.join("absent"), Path::new(""), false, ""));
    assert_eq!(index.len(), 0);
    assert_eq!(index.last_patch(), (&[(0usize, 1usize), (0, 0)][..], 0));
    assert_eq!(index.epoch, 1);
    assert_eq!(index.partial, None);
}
