use super::*;
use crate::backend::searchindex::RankOrder;
use std::ffi::OsStr;

fn session(dir: &TestDir, entries: &[(&str, bool)]) -> Session {
    let scope = dir.path().to_str().unwrap();
    let mut session = Session::open_sync(scope, scope, false);
    let mut index = Index::new();
    for &(path, directory) in entries { assert!(index.push(path, directory)); }
    index.finish();
    *session.index.lock().unwrap() = index;
    session.priors = None;
    session
}

fn all(session: &mut Session, gen: &mut usize, query: &str, window: usize) -> (usize, Vec<(String, bool)>) {
    *gen += 1;
    let answer = session.query(*gen, query, window).unwrap();
    let (total, mut rows) = (answer.total, answer.rows);
    assert_eq!(rows.len(), window.min(total), "head size is exactly the requested prefix");
    let mut adopted = rows.len();
    while let Some(next) = session.take_rest_chunk(adopted + CHUNK, adopted) {
        assert!(!next.is_empty());
        adopted += next.len();
        rows.extend(next);
    }
    assert_eq!(rows.len(), total);
    (total, rows)
}

#[test]
fn search_a42_total_order_breaks_the_cycle_without_global_files_first() {
    let dir = TestDir::new("searcha42cycle");
    let entries = [("a/x", true), ("a/z", true), ("a/x/f", false)];
    for permutation in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        let values: Vec<_> = permutation.iter().map(|&at| entries[at]).collect();
        let mut session = session(&dir, &values);
        let mut gen = 0;
        for window in [1, 2, CHUNK] {
            let (total, rows) = all(&mut session, &mut gen, "a/", window);
            assert_eq!(total, 3);
            assert_eq!(rows, [("a/z".to_string(), true), ("a/x/f".to_string(), false), ("a/x".to_string(), true)],
                "the unrelated directory stays ahead of the file while its own ancestor waits");
        }
    }
}

#[test]
fn search_a42_family_comparator_is_total_and_keeps_scores_and_unconstrained_order() {
    let mut index = Index::new();
    for (path, kind) in [("a/x", true), ("a/z", true), ("a/x/f", false), ("a/x/g", false),
        ("a/é", true), ("a/é/中", false), ("a/İ", true), ("a/İ/notes", false), ("else/strong", false)] {
        assert!(index.push(path, kind));
    }
    for (path, kind) in [(b"a/bad\xff".as_slice(), true), (b"a/bad\xff/file".as_slice(), false)] {
        assert!(index.push(OsStr::from_bytes(path), kind));
    }
    let hits: Vec<_> = (0..index.len()).map(|row| (if index.path(row) == "else/strong" { 200 } else { 100 }, row)).collect();
    let order = RankOrder::new(&index, &hits, true);
    let mut expected: Vec<_> = hits.iter().map(|&hit| (hit, hit.0)).collect();
    super::family_oracle::order(&index, &mut expected);
    let mut sorted = hits.clone();
    sorted.sort_by(|a, b| order.cmp(&index, a, b));
    assert_eq!(sorted, expected.iter().map(|pair| pair.0).collect::<Vec<_>>());
    assert_eq!(index.path(sorted[0].1), "else/strong", "stronger lexical/prior scores cannot be crossed");
    for a in &hits {
        assert_eq!(order.cmp(&index, a, a), std::cmp::Ordering::Equal);
        for b in &hits {
            assert_eq!(order.cmp(&index, a, b), order.cmp(&index, b, a).reverse());
            if !index.is_dir(a.1) && !index.is_dir(b.1) { assert_eq!(order.cmp(&index, a, b), rank_cmp(&index, a, b)); }
            for c in &hits {
                if !order.cmp(&index, a, b).is_gt() && !order.cmp(&index, b, c).is_gt() {
                    assert!(!order.cmp(&index, a, c).is_gt(), "transitivity");
                }
            }
        }
    }
    let plain = RankOrder::new(&index, &hits, false);
    for a in &hits { for b in &hits { assert_eq!(plain.cmp(&index, a, b), rank_cmp(&index, a, b)); } }
    let mut higher_ancestor = hits.clone();
    let ancestor = higher_ancestor.iter_mut().find(|hit| index.path(hit.1) == "a/x").unwrap();
    ancestor.0 = 101;
    let order = RankOrder::new(&index, &higher_ancestor, true);
    higher_ancestor.sort_by(|a, b| order.cmp(&index, a, b));
    assert_eq!(index.path(higher_ancestor[1].1), "a/x", "an unequal higher ancestor keeps its score priority");
}

#[test]
fn search_a42_boundary_and_prior_states_match_full_dependency_order() {
    let dir = TestDir::new("searcha42boundary");
    let mut entries = vec![("a/x".to_string(), true)];
    for at in 0..CHUNK + 7 {
        entries.push((format!("a/z{at:04}"), true));
        entries.push((format!("a/x/f{at:04}.txt"), false));
    }
    entries.extend([("a/é".to_string(), true), ("a/é/中.txt".to_string(), false), ("a/x/real.txt".to_string(), false), ("a/x/reaq.txt".to_string(), false)]);
    let borrowed: Vec<_> = entries.iter().map(|(path, kind)| (path.as_str(), *kind)).collect();
    let mut session = session(&dir, &borrowed);
    {
        let mut index = session.index.lock().unwrap();
        assert!(index.push(OsStr::from_bytes(b"a/bad\xff"), true));
        assert!(index.push(OsStr::from_bytes(b"a/bad\xff/file.txt"), false));
        index.finish();
    }
    let priors = { let index = session.index.lock().unwrap(); priors_for(&dir, &index, 19, 13) };
    let (mut gen, mut seen) = (0, Seen::default());
    for snapshot in [None, Some(priors)] {
        session.priors = snapshot;
        session.last_query.clear();
        for window in [1, 2, CHUNK, CHUNK + 1, entries.len() + 1] {
            for query in ["a", "a/", "a/x/", "a/x/f", "a/é/", "real", "reaq", "a/x/real"] {
                check(&mut session, &mut gen, query, window, &mut seen);
            }
        }
    }
}

#[test]
fn search_a42_slash_files_precede_their_equal_score_ancestors_in_head_and_rest() {
    let dir = TestDir::new("searcha42ancestors");
    let editor = "projects/editor/src/components/render_frame.rs";
    let archive = "archive/2024/client/reports/quarterly_review.pdf";
    let mut session = session(&dir, &[
        ("projects/editor/src", true), ("projects/editor/src/components", true), (editor, false),
        ("archive/2024/client", true), ("archive/2024/client/reports", true), (archive, false),
    ]);
    let mut gen = 0;
    for (query, expected) in [
        ("editor/", vec![(editor, false), ("projects/editor/src", true), ("projects/editor/src/components", true)]),
        ("editor/src/", vec![(editor, false), ("projects/editor/src/components", true)]),
        ("archive/2024/", vec![(archive, false), ("archive/2024/client", true), ("archive/2024/client/reports", true)]),
        ("archive/2024/client/", vec![(archive, false), ("archive/2024/client/reports", true)]),
    ] {
        for window in [1, 2, CHUNK] {
            let (total, rows) = all(&mut session, &mut gen, query, window);
            let want: Vec<_> = expected.iter().map(|&(path, kind)| (path.to_string(), kind)).collect();
            assert_eq!(total, want.len(), "{query:?} exact visible count");
            assert_eq!(rows, want, "same-score slash file must precede its own ancestors, including across the head/rest cut");
        }
    }
}
