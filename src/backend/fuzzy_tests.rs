use super::*;

fn score(hay: &str, query: &str) -> Option<i32> {
    Fuzzy::new(query).score(hay)
}

#[test]
fn the_operators_own_example_matches_across_the_separator() {
    // "dwnhelp should find downloads/helper.txt", which substring matching never could.
    assert!(score("downloads/helper.txt", "dwnhelp").is_some());
    assert!(score("downloads/helper.txt", "help").is_some());
    assert!(score("downloads/helper.txt", "zzz").is_none());
}

#[test]
fn a_query_out_of_order_is_not_a_match() {
    assert!(score("helper.txt", "pleh").is_none());
    assert!(score("abc", "abcd").is_none());
}

#[test]
fn case_folds_both_ways_including_beyond_ascii() {
    assert!(score("Bench-Notes.md", "bench").is_some());
    assert!(score("BENCH", "bench").is_some());
    assert!(score("CAFÉ.txt", "café").is_some());
    assert!(score("café.txt", "CAFÉ").is_some());
}

#[test]
fn an_empty_query_matches_every_candidate() {
    assert_eq!(score("anything", ""), Some(0));
    assert_eq!(score("", ""), Some(0));
}

#[test]
fn a_contiguous_run_beats_a_scattered_one() {
    let s_run = score("report.txt", "rep").unwrap();
    let s_scattered = score("raspberry-pie.txt", "rep").unwrap();
    assert!(
        s_run > s_scattered,
        "run {} scattered {}",
        s_run,
        s_scattered
    );
}

#[test]
fn a_boundary_start_beats_one_inside_a_word() {
    let s_boundary = score("my-notes.txt", "notes").unwrap();
    let s_inside = score("bignotes.txt", "notes").unwrap();
    assert!(
        s_boundary > s_inside,
        "boundary {} inside {}",
        s_boundary,
        s_inside
    );
}

#[test]
fn a_camel_hump_counts_as_a_boundary() {
    let s_hump = score("SearchStrip.qml", "strip").unwrap();
    let s_flat = score("searchstrip.qml", "strip").unwrap();
    assert!(s_hump > s_flat, "hump {} flat {}", s_hump, s_flat);
}

#[test]
fn a_match_in_the_name_beats_one_in_a_parent_directory() {
    let s_name = score("notes/bench.txt", "bench").unwrap();
    let s_parent = score("bench/notes.txt", "bench").unwrap();
    assert!(s_name > s_parent, "name {} parent {}", s_name, s_parent);
}

#[test]
fn a_later_start_can_score_better_than_the_first_one() {
    // The first "a" is at 0, but the alignment that reads "ab" contiguously starts at 3.
    let s = score("axxab", "ab").unwrap();
    let greedy_only = score("axxb", "ab").unwrap();
    assert!(s > greedy_only, "restarted {} greedy {}", s, greedy_only);
}

#[test]
fn a_pathological_name_is_scored_from_a_bounded_number_of_starts() {
    let hay = "a".repeat(4096);
    // Bounded work, and still the right answer: every start scores the same here.
    assert!(score(&hay, "aa").is_some());
}

#[test]
fn scoring_a_walk_reuses_one_candidate_buffer() {
    let mut fuzzy = Fuzzy::new("bench");
    for candidate in ["docs/bench-notes.txt", "docs/café.txt", "docs/bench.txt"] {
        assert_eq!(
            fuzzy.score(candidate),
            Fuzzy::new("bench").score(candidate),
            "{candidate} scores as a fresh fold does, a wide one between ASCII ones included"
        );
    }
    assert_eq!(
        fuzzy.ascii.len(),
        "docs/bench.txt".len(),
        "the last ASCII score replaced the buffer, never appended"
    );
}

#[test]
fn the_rank_order_is_total_so_one_tree_answers_in_one_order() {
    assert_eq!(rank_order(9, "a.txt", 4, "b.txt"), Ordering::Less);
    assert_eq!(rank_order(4, "a.txt", 9, "b.txt"), Ordering::Greater);
    assert_eq!(rank_order(4, "a.txt", 4, "bb.txt"), Ordering::Less);
    assert_eq!(rank_order(4, "b.txt", 4, "a.txt"), Ordering::Greater);
    assert_eq!(rank_order(4, "a.txt", 4, "a.txt"), Ordering::Equal);
}

// Sample input: var SCORES = [["report.txt", "rep", 34], [bounded + "ab", "ab", 6]] with rows split on "]," and the table closed by "]]".
fn js_scores(src: &str, bounded: &str) -> Vec<(String, String, Option<i32>)> {
    let table = src
        .split("var SCORES = [")
        .nth(1)
        .expect("tests/js/jump.js carries SCORES")
        .split("]]")
        .next()
        .expect("SCORES closes");
    let mut out = Vec::new();
    for entry in table.split("],") {
        let entry = entry.trim().trim_start_matches('[').trim();
        if entry.is_empty() {
            continue;
        }
        let (hay, rest) = match entry.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted.find('"').expect("a SCORES row closes its file");
                (quoted[..end].to_string(), &quoted[end + 1..])
            }
            // The one row jump.js builds programmatically rather than spelling out.
            None => {
                assert!(
                    entry.starts_with("bounded + \"ab\""),
                    "a SCORES row names its file first: {entry}"
                );
                (bounded.to_string(), &entry["bounded + \"ab\"".len()..])
            }
        };
        let rest = &rest[rest.find('"').expect("a SCORES row names its query") + 1..];
        let end = rest.find('"').expect("a SCORES row closes its query");
        let (query, rest) = (rest[..end].to_string(), &rest[end + 1..]);
        let digits = rest
            .rsplit(',')
            .next()
            .expect("a SCORES row carries its score")
            .trim()
            .trim_end_matches(']');
        out.push((
            hay,
            query,
            match digits {
                "null" => None,
                _ => Some(digits.parse().expect("a SCORES score parses as an integer")),
            },
        ));
    }
    out
}

// SCORES is read out of tests/js/jump.js, so a weight changed here fails this test until the JS table is changed with it.
#[test]
fn the_exact_scores_the_jump_port_mirrors() {
    let bounded = format!("{}ab", "ax".repeat(16));
    let rows = js_scores(include_str!("../../tests/js/jump.js"), &bounded);
    assert_eq!(
        rows.len(),
        15,
        "SCORES in tests/js/jump.js grew or shrank; change this row count with it"
    );
    for (hay, query, expected) in &rows {
        assert_eq!(score(hay, query), *expected, "{query} against {hay}");
    }
}
