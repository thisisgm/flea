use super::*;
use crate::backend::searchmatch::{Matcher, tests::reference};
use std::sync::atomic::Ordering;

#[test]
fn l_search_tail_resolved_bounds_equal_independent_raw_path_oracle() {
    let mut index = Index::new();
    for parent in ["", "abc", "a/b/c", "a_b/c", "café", "İ/ab", "x/abc/long_gap", "b/c/a"] {
        for name in ["xyz.txt", "a_b_c", "abc", "aBc.txt", "cab", "café", "a________b________c"] {
            index.push(if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") }, false);
        }
    }
    index.push(OsStr::from_bytes(b"abc/raw_\xff.zzz"), false);
    index.push(OsStr::from_bytes(b"d\xff/caf\xfe.txt"), false);
    let rows: Vec<_> = (0..index.len()).collect();
    let mut scratch = QueryScratch::default();
    for query in ["", "a", "ab", "abc", "bca", "abcd", "café", "caf�", "a/b", "ab/c", "abc/", "qqqq"] {
        let mut expected: Vec<_> = rows.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
        expected.sort_by(|a, b| rank_cmp(&index, a, b));
        for window in [0, 1, 3, 350] {
            let matcher = Matcher::deferred(query);
            let mut head = score_cached(&matcher, &rows, &index, &mut scratch);
            let mut tail = resolve_head(&matcher, &index, &mut scratch, &mut head, window);
            resolve(&matcher, &index, &mut scratch, &mut tail);
            head.extend(tail);
            head.sort_by(|a, b| rank_cmp(&index, a, b));
            assert_eq!(head, expected, "{query:?} window={window}");
        }
    }
}

#[test]
fn l_search_tail_slash_terminated_queries_reuse_the_completed_parent() {
    for count in [5000, 40_000] {
        let mut index = Index::new();
        for row in 0..count { index.push(format!("abc/xyz_{row}.zzz"), false); }
        index.push(OsStr::from_bytes(b"abc/raw_\xff.zzz"), false);
        let rows: Vec<_> = (0..index.len()).collect();
        let mut scratch = QueryScratch::default();
        scratch.pinned_cores = Some(2);
        let matcher = Matcher::deferred("abc/");
        let mut head = score_cached(&matcher, &rows, &index, &mut scratch);
        assert_eq!(head.len(), rows.len());
        index.path_dp.store(0, Ordering::Relaxed);
        let tail = resolve_head(&matcher, &index, &mut scratch, &mut head, 350);
        assert!(tail.is_empty());
        assert_eq!(head.len(), rows.len(), "all equal-score paths stay available for scrolling");
        assert_eq!(index.path_dp.load(Ordering::Relaxed), 0, "no basename can finish a slash-terminated query");
        head.sort_by(|a, b| rank_cmp(&index, a, b));
        assert_eq!(head, optimized(&index, "abc/", &rows));
    }
}

fn optimized(index: &Index, query: &str, rows: &[usize]) -> Vec<(i32, usize)> {
    let mut hits = score_all(&Matcher::new(query), rows, index);
    hits.sort_by(|a, b| rank_cmp(index, a, b));
    hits
}

#[test]
fn search_pruning_and_directory_states_equal_exhaustive_scores_and_ranks() {
    let mut index = Index::new();
    for parent in ["", "a", "b", "ab", "aba", "a/b", "a_", "d\u{fffd}", "İ"] {
        for name in ["a", "b", "ab", "ba", "aa", "abba", "a1", "b1", "a_b", "CAFÉ.txt"] {
            index.push(if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") }, false);
        }
    }
    index.push(OsStr::from_bytes(b"d\xff/caf\xfe.txt"), false);
    index.push("downloads/helper.txt", false);
    index.push("projects/editor/src/components/render_frame.rs", false);
    for i in 0..150 { index.push(format!("z{i}/zzz{i}"), false); }
    let rows: Vec<_> = (0..index.len()).collect();
    for query in ["", "a", "ab", "ba", "aa", "abba", "a/b", "a1", "1", "a_b", "café", "caf�", "dwnhelp", "donwloads/helper", "edotor/src/components/render", "editor/src/components/render", "redner_frame.rs", "qqqq"] {
        let mut expected: Vec<_> = rows.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
        expected.sort_by(|a, b| rank_cmp(&index, a, b));
        assert_eq!(optimized(&index, query, &rows), expected, "{query}");
    }
}

#[test]
fn search_ranks_a_typed_folder_run_above_scattered_name_matches() {
    let mut index = Index::new();
    index.push("projects/editor", true);
    for path in ["Music/folder_01/test_test_00074578_detail_detail_detail.json", "Music/folder_01/credit_notes.txt",
                 "projects/editor/src/components/render_frame.rs", "Music/folder_01/notes_edit.txt", "Music/folder_01/tide.txt"] {
        index.push(path, false);
    }
    let rows: Vec<_> = (0..index.len()).collect();
    for query in ["e", "ed", "edi", "edit", "edito"] {
        let ranked: Vec<String> = optimized(&index, query, &rows).iter().map(|&(_, row)| index.path(row).to_string()).collect();
        let at = |path: &str| ranked.iter().position(|row| row == path);
        let target = at("projects/editor/src/components/render_frame.rs").expect("the file under the typed folder matches");
        assert_eq!(at("projects/editor"), Some(0), "{query}: the folder itself leads, {ranked:?}");
        if let Some(scattered) = at("Music/folder_01/test_test_00074578_detail_detail_detail.json") {
            assert!(target < scattered, "{query}: {ranked:?}");
        }
    }
}

#[test]
fn search_scores_each_distinct_eligible_name_once_with_no_file_path_reconstruction() {
    let mut index = Index::new();
    for dir in 0..1000 {
        index.push(format!("d{dir}/render_frame.rs"), false);
        index.push(format!("d{dir}/other.txt"), false);
    }
    let rows: Vec<_> = (0..index.len()).collect();
    index.reconstructed.store(0, Ordering::Relaxed);
    let hits = score_all(&Matcher::new("redner_frame.rs"), &rows, &index);
    assert_eq!(hits.len(), 1000);
    assert_eq!(index.names_scored.load(Ordering::Relaxed), 1);
    assert_eq!(index.reconstructed.load(Ordering::Relaxed), 0);
    assert_eq!(index.dirs_scored.load(Ordering::Relaxed), 0);
    assert_eq!(index.paths_scored.load(Ordering::Relaxed), 0);
    assert!(score_all(&Matcher::new("qxyz"), &rows, &index).is_empty());
    assert_eq!(index.names_scored.load(Ordering::Relaxed), 0);
    assert_eq!(index.dirs_scored.load(Ordering::Relaxed), 0);
    assert_eq!(index.reconstructed.load(Ordering::Relaxed), 0);
}

#[test]
fn search_path_work_reuses_each_directory_state_and_honors_candidate_subset() {
    let mut index = Index::new();
    for dir in 0..100 {
        for name in ["helper.txt", "helping.txt", "other.txt"] {
            index.push(format!("downloads/d{dir}/{name}"), false);
        }
    }
    let rows: Vec<_> = (0..index.len()).collect();
    let hits = score_all(&Matcher::new("dwnhelp"), &rows, &index);
    assert_eq!(hits.len(), 200);
    assert_eq!(index.names_scored.load(Ordering::Relaxed), 0);
    assert_eq!(index.dirs_scored.load(Ordering::Relaxed), 101);
    assert_eq!(index.paths_scored.load(Ordering::Relaxed), 200);
    assert_eq!(index.reconstructed.load(Ordering::Relaxed), 0);
    let subset = [0, 3, 6];
    assert_eq!(score_all(&Matcher::new("help"), &subset, &index).len(), 3);
    assert_eq!(index.names_scored.load(Ordering::Relaxed), 1);
}

#[test]
fn search_rejected_path_suffixes_never_build_alignment_states() {
    let mut index = Index::new();
    for dir in 0..1000 { index.push(format!("folder_{dir}/render_model_detail.rs"), false); }
    let rows: Vec<_> = (0..index.len()).collect();
    for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render"] {
        let expected: Vec<_> = rows.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
        index.reconstructed.store(0, Ordering::Relaxed);
        assert!(expected.is_empty());
        assert_eq!(score_all(&Matcher::new(query), &rows, &index), expected);
        assert_eq!(index.dirs_scored.load(Ordering::Relaxed), 0);
        assert_eq!(index.paths_scored.load(Ordering::Relaxed), 0);
        assert_eq!(index.reconstructed.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn search_resolve_splits_pending_rows_across_threads_and_matches_each_row_alone() {
    // Top-level names, basename subsequences and path-only rows interleave, so every chunk resets, copies and switches parents.
    use crate::backend::searchmatch::{NAME, PENDING_NAME, PENDING_PATH};
    let mut index = Index::new();
    for row in 0..3001 {
        index.push(match row % 3 {
            0 => format!("top_e{row}x_d.txt"),
            1 => format!("f{:02}/notes_e{row}d.txt", row % 40),
            _ => format!("ed{:02}/x{row}.txt", row % 40),
        }, false);
    }
    let matcher = Matcher::deferred("ed");
    let rows: Vec<_> = (0..index.len()).collect();
    let mut scratch = QueryScratch::default();
    scratch.pinned_cores = Some(WALK_THREADS);
    let hits = score_cached(&matcher, &rows, &index, &mut scratch);
    let marked = |marker| hits.iter().filter(|hit| hit.0 == marker).count();
    assert_eq!((marked(PENDING_NAME), marked(PENDING_PATH)), (2001, 1000), "the fixture marks every row for the DP");
    let resolved: Vec<_> = rows.iter().map(|&row| (NAME, row)).collect();
    let alone: Vec<_> = hits.iter().map(|&hit| {
        let mut one = [hit];
        resolve(&matcher, &index, &mut scratch, &mut one);
        one[0]
    }).collect();
    let join = |head: &[(i32, usize)], tail: &[(i32, usize)]| -> Vec<_> { head.iter().chain(tail).copied().collect() };
    // resolve_head's low list reaches Rest::promote pending first; resolved first fails a cut that counts rows instead of pending ones.
    for (order, mut batch, expected) in [
        ("pending first", join(&hits, &resolved), join(&alone, &resolved)),
        ("resolved first", join(&resolved, &hits), join(&resolved, &alone)),
    ] {
        scratch.counters.resolve_largest_share = 0;
        resolve(&matcher, &index, &mut scratch, &mut batch);
        assert_eq!(batch, expected, "{order}: a split resolve scores each pending row as a resolve of that row alone and leaves resolved rows alone");
        let largest = scratch.counters.resolve_largest_share;
        assert_eq!(largest, hits.len().div_ceil(WALK_THREADS), "{order}: 3001 pending rows, which no thread count divides, split evenly across every thread");
    }
}
