use super::*;
use crate::backend::searchmatch::{Matcher, tests::reference};

#[test]
fn search_review_generated_paths_match_reference() {
    let mut seed = 9u64;
    let mut word = |len: usize| {
        (0..len).map(|_| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let chars: Vec<char> = "abcAB_1.-éİ".chars().collect();
            chars[(seed >> 32) as usize % chars.len()]
        }).collect::<String>()
    };
    for round in 0..100 {
        let mut index = Index::new();
        for i in 0..24 {
            let path = format!("{}/{}/{}", word(1 + i % 4), word(1 + i % 5), word(1 + i % 8));
            assert!(index.push(path, false));
        }
        let rows: Vec<_> = (0..index.len()).collect();
        for i in 0..30 {
            let query = if i % 3 == 0 {
                format!("{}/{}", word(1 + i % 4), word(1 + i % 6))
            } else { word(1 + i % 10) };
            let actual = score_all(&Matcher::new(&query), &rows, &index);
            let expected: Vec<_> = rows.iter().filter_map(|&row| reference(&query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(actual, expected, "round={round} query={query:?} paths={:?}", rows.iter().map(|&row| index.path(row)).collect::<Vec<_>>());
        }
    }
}

#[test]
fn search_review_latency_loss_queries_return_same_raw_target() {
    let target = b"projects/editor/src/components/render_frame.rs";
    for count in [100_000, 1_000_000] {
        let (index, _) = super::fixture_tests::synthetic(count, false);
        assert_eq!(index.len(), count);
        assert_eq!(index.partial, None);
        let rows: Vec<_> = (0..index.len()).collect();
        for query in ["render_frame.rs", "redner_frame.rs", "editor/src/components/render"] {
            let actual: Vec<_> = score_all(&Matcher::new(query), &rows, &index).into_iter().map(|(_, row)| index.raw(row)).collect();
            let mut old = crate::backend::fuzzy::Fuzzy::new(query);
            let expected: Vec<_> = rows.iter().filter_map(|&row| {
                let raw = index.raw(row);
                old.score(&String::from_utf8_lossy(&raw)).map(|_| raw)
            }).collect();
            assert_eq!(expected, vec![target.to_vec()], "legacy {count} {query}");
            assert_eq!(actual, expected, "same full-coverage identity {count} {query}");
        }
    }
}
