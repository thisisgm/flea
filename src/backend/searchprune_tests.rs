use super::*;
use crate::backend::searchmatch::{Matcher, tests::reference};
use std::sync::atomic::Ordering;

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
