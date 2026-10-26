use super::*;
use crate::backend::searchindex::{self, Index, score_cached};
use crate::backend::searchmatch::{Matcher, tests::reference};
use crate::backend::searchprior::alloc_tests::count_allocations;
use std::collections::HashSet;

fn bounds(index: &Index, rows: &[usize]) -> (usize, usize) {
    let mut names = HashSet::new();
    let mut parents = HashSet::new();
    for &row in rows {
        names.insert(index.rows[row].name);
        let mut parent = index.rows[row].parent;
        while parent != searchindex::EMPTY && parents.insert(parent) {
            parent = index.blocks.record(parent).1;
        }
    }
    (names.len(), parents.len())
}

fn target(index: &Index) -> usize {
    index.rows.iter().position(|row| index.names.get(row.name) == b"render_frame.rs").unwrap()
}

#[test]
fn search_q2_scratch_work_follows_candidates_not_index_size() {
    for count in [100_000, 1_000_000] {
        let (index, _) = searchindex::fixture_tests::synthetic(count, false);
        let rows: Vec<_> = (0..index.len()).collect();
        let mut scratch = QueryScratch::default();
        score_cached(&Matcher::new("r"), &rows, &index, &mut scratch);
        let target = target(&index);
        for candidates in [vec![target], vec![index.len() - 1], vec![0, target, index.len() - 1], vec![]] {
            let (names, parents) = bounds(&index, &candidates);
            for query in ["render_frame.rs", "qqqq", "editor/src/components/render", "redner_frame.rs"] {
                score_cached(&Matcher::new(query), &candidates, &index, &mut scratch);
                let counters = &scratch.counters;
                assert!(counters.names_scored <= names, "{count} {query}: {counters:?}, names={names}");
                assert!(counters.directory_reach <= parents, "{count} {query}: {counters:?}, parents={parents}");
                assert!(counters.scratch_touched <= names + parents, "{count} {query}: {counters:?}, slots={}", names + parents);
            }
        }
        println!("Q2_COUNTERS entries={count}: candidate name and ancestor bounds passed");
    }
}

#[test]
fn search_q2_warm_sparse_scoring_allocates_only_its_output() {
    for count in [10_000, 100_000, 1_000_000] {
        let (index, _) = searchindex::fixture_tests::synthetic(count, false);
        let mut scratch = QueryScratch::default();
        let rows: Vec<_> = (0..index.len()).collect();
        score_cached(&Matcher::new("r"), &rows, &index, &mut scratch);
        let candidates = [target(&index)];
        for (query, expected) in [("qqqq", 0), ("render_frame.rs", 1), ("editor/src/components/render", 1)] {
            let matcher = Matcher::new(query);
            score_cached(&matcher, &candidates, &index, &mut scratch);
            let (hits, allocations) = count_allocations(|| score_cached(&matcher, &candidates, &index, &mut scratch));
            println!("Q2_ALLOC entries={count} query={query:?} allocations={allocations} hits={}", hits.len());
            assert_eq!(hits.len(), expected);
            assert!(allocations <= expected, "{count} {query}: {allocations} allocations, only the output may allocate");
        }
    }
}

#[test]
fn search_q2_reused_scratch_preserves_full_first_query_and_changed_queries() {
    let mut scratch = QueryScratch::default();
    for round in 0..3 {
        let mut index = Index::new();
        for parent in ["a", "downloads", "projects/editor/src/components", "café/CAFÉ", "archive/2024"] {
            for name in ["render_frame.rs", "helper.txt", "abcd1.svg", "cafè.txt", "quarterly_review.pdf"] {
                assert!(index.push(format!("{parent}/{name}"), false));
            }
        }
        if round > 0 {
            index.epoch = round;
            assert!(index.push("new-parent/renamed.txt", false));
        }
        let rows: Vec<_> = (0..index.len()).rev().collect();
        for query in ["r", "redner_frame.rs", "editor/src/components/render", "dwnhelp", "café", "archive/2024", ".svg", "", "zzzz"] {
            let expected: Vec<_> = rows.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(score_cached(&Matcher::new(query), &rows, &index, &mut scratch), expected, "round={round} {query}");
            let subset: Vec<_> = rows.iter().copied().step_by(3).collect();
            let expected: Vec<_> = subset.iter().filter_map(|&row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(score_cached(&Matcher::new(query), &subset, &index, &mut scratch), expected, "subset round={round} {query}");
        }
    }
}

#[test]
fn search_q2_replaced_arenas_and_generation_wrap_drop_query_results() {
    let mut scratch = QueryScratch::default();
    for parent in ["aaaa", "zzzz"] {
        let mut index = Index::new();
        assert!(index.push(format!("{parent}/leaf.txt"), false));
        assert!(index.push(format!("{parent}/other.txt"), false));
        for query in ["aaaa/leaf", "zzzz/leaf", "leaf", "qqqq"] {
            let expected: Vec<_> = (0..index.len()).filter_map(|row| reference(query, &index.raw(row)).map(|score| (score, row))).collect();
            assert_eq!(score_cached(&Matcher::new(query), &[0, 1], &index, &mut scratch), expected);
            scratch.generation = u32::MAX;
            assert_eq!(score_cached(&Matcher::new(query), &[0, 1], &index, &mut scratch), expected);
        }
    }
}

// The typed queries whose stale path text and states once held 35.7 MB of a 1M session after scoring.
const TYPED: [&str; 6] = ["render_frame.rs", "redner_frame.rs", "editor/src/components/render", ".svg",
    "archive/2024/client/reports/quarterly", "editor/src/components/redner"];
const TYPED_ROWS: usize = 100_000;

#[test]
fn search_g1_path_text_and_states_keep_only_the_last_generation() {
    let (index, _) = searchindex::fixture_tests::synthetic(TYPED_ROWS, false);
    let rows: Vec<_> = (0..index.len()).collect();
    let mut scratch = QueryScratch::default();
    let (mut peak_text, mut peak_states) = (0, 0);
    for query in TYPED {
        for end in 1..=query.len() {
            score_cached(&Matcher::new(&query[..end]), &rows, &index, &mut scratch);
            peak_text = peak_text.max(scratch.texts.len());
            peak_states = peak_states.max(scratch.states_used);
            assert!(scratch.texts.capacity() <= SHRINK_ABOVE * scratch.texts.len(), "{:?}: text {} of {}",
                &query[..end], scratch.texts.len(), scratch.texts.capacity());
            assert!(scratch.states.len() <= SHRINK_ABOVE * scratch.states_used, "{:?}: states {} of {}",
                &query[..end], scratch.states_used, scratch.states.len());
        }
    }
    assert!(peak_text > 0 && peak_states > 0, "the typed queries must build path text and states");
    assert_eq!((scratch.names.len(), scratch.dirs.len()), (index.names.len(), index.blocks.len()));
    let state_bytes: usize = scratch.states.iter().map(|state| std::mem::size_of::<State>() + state.heap_bytes()).sum();
    println!("G1_GATE rows={TYPED_ROWS} names={} dirs={} jobs={} text={} states={state_bytes} peak_text={} peak_states={peak_states}",
        scratch.names.capacity() * std::mem::size_of::<NameSlot>(), scratch.dirs.capacity() * std::mem::size_of::<Directory>(),
        scratch.jobs.capacity() * std::mem::size_of::<(u32, i32)>(), scratch.texts.capacity() * std::mem::size_of::<Letter>(),
        peak_text * std::mem::size_of::<Letter>());
    score_cached(&Matcher::new("qqqq"), &rows, &index, &mut scratch);
    assert_eq!((scratch.texts.len(), scratch.states_used), (0, 0), "qqqq builds no path text and no states");
    assert_eq!((scratch.texts.capacity(), scratch.states.capacity()), (0, 0), "a query without path work frees the last one's");
}
