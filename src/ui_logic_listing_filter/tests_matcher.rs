//! tests/js/match.js and Fuzzy.js ported, then Filter040's wildcards and close matches.
use super::*;

fn run(name: &str, query: &str) -> Option<Run> {
    Matcher::new(query)?.exact(name)
}

fn names(list: &[&'static str]) -> Vec<(usize, &'static str)> {
    list.iter().copied().enumerate().collect()
}

#[test]
fn match_js_splits_a_walked_path_into_its_name_and_folder() {
    assert_eq!(location("BENCHMARK.md"), "", "a match in the root has no location");
    assert_eq!(base("BENCHMARK.md"), "BENCHMARK.md", "a match in the root is its own base name");
    assert_eq!(location("tools/benches/bench-run.sh"), "tools/benches", "a nested match keeps its parent path");
    assert_eq!(base("tools/benches/bench-run.sh"), "bench-run.sh", "a nested match's base name is the leaf");
    assert_eq!(base("tools/benches"), "benches", "a directory match's base name is its last segment");
}

#[test]
fn match_js_paints_the_run_where_the_folded_query_sits() {
    assert_eq!(run("bench.txt", ""), None, "an empty query paints no run");
    assert_eq!(run("other.txt", "bench"), None, "a miss paints no run");
    assert_eq!(run("bench.txt", "bench").map(|r| r.start), Some(0), "a run starts where the query starts");
    assert_eq!(run("bench.txt", "bench").map(|r| r.len), Some(5), "a run is exactly the query's length");
    assert_eq!(run("BENCHMARK.md", "bench").map(|r| r.start), Some(0), "the run folds case the way the backend does");
    assert_eq!(run("field-bench-notes.md", "bench").map(|r| r.start), Some(6), "the run finds a query inside the name");
}

// SCORES is read out of tests/js/jump.js, the table the backend scorer is also held to, so the two cannot drift apart.
#[test]
fn the_close_scorer_keeps_the_backend_scorers_exact_scores() {
    let src = include_str!("../../tests/js/jump.js");
    let table = src.split("var SCORES = [").nth(1).unwrap().split("]]").next().unwrap();
    let bounded = format!("{}ab", "ax".repeat(16));
    let mut seen = 0;
    for entry in table.split("],") {
        let entry = entry.trim().trim_start_matches('[').trim();
        let quoted: Vec<&str> = entry.split('"').collect();
        let (hay, query) = if entry.starts_with("bounded") { (bounded.as_str(), quoted[3]) } else { (quoted[1], quoted[3]) };
        let score: i32 = entry.rsplit(',').next().unwrap().trim().trim_end_matches(']').parse().unwrap();
        assert_eq!(fuzzy_score(hay, query), Some(score), "{query} against {hay}");
        seen += 1;
    }
    assert_eq!(seen, 15, "SCORES in tests/js/jump.js grew or shrank");
    assert_eq!(fuzzy_score("report.txt", "xyz"), None, "a query out of the name is no subsequence");
}

#[test]
fn fuzzy_js_washes_the_longest_run_the_first_on_a_tie() {
    assert_eq!(longest_run(&[2, 5, 6, 7, 9]), Some(Run { start: 5, len: 3 }), "a scattered match washes its longest run");
    assert_eq!(longest_run(&[1, 2, 5, 6]), Some(Run { start: 1, len: 2 }), "a tie washes the first run");
    assert_eq!(longest_run(&[]), None, "no positions wash nothing");
    let mut matcher = Matcher::new("o").unwrap();
    let close = matcher.close("~/Documents/claude/omarchy").unwrap();
    assert_eq!(close.runs[0].start, 19, "the leaf's word start wins the wash over a letter inside a parent");
}

#[test]
fn a_pattern_with_wildcards_shows_item_70_to_79_and_a_plain_query_stays_a_substring() {
    let many: Vec<String> = (1..=80).map(|i| format!("item-{i:02}.txt")).collect();
    let rows: Vec<(usize, &str)> = many.iter().map(String::as_str).enumerate().collect();
    let shown = narrow("item-7?.txt", rows.iter().copied()).unwrap();
    let drawn: Vec<&str> = shown.exact.iter().map(|&row| many[row].as_str()).collect();
    assert_eq!(drawn, (70..=79).map(|i| format!("item-{i}.txt")).collect::<Vec<_>>(), "? stands for one character");
    assert!(shown.close.is_empty(), "a pattern never lists close matches");
    assert_eq!(narrow("ITEM-7*", rows.iter().copied()).unwrap().exact.len(), 10, "* takes any run and case folds");
    assert_eq!(narrow("*", rows.iter().copied()).unwrap().exact.len(), 80, "a lone star keeps every row");
    assert_eq!(narrow("item-7", rows.iter().copied()).unwrap().exact.len(), 10, "without wildcards the query is a substring");
    assert_eq!(run("item-70.txt", "7?.t"), Some(Run { start: 5, len: 4 }), "the pattern washes the stretch it matched");
    assert_eq!(run("item-7.txt", "item-7?.txt"), None, "? needs a character to stand for");
    assert_eq!(run("a", "a?"), None, "and the end of a name is not one");
    assert_eq!(run("abc", "a*c*"), Some(Run { start: 0, len: 3 }), "a trailing star takes nothing it does not need");
}

#[test]
fn a_typo_lists_its_close_match_only_while_nothing_matches_exactly() {
    let work = names(&["A", "Code", "Docs", "Locked", "Many", "Media", "notes.txt", "photo.png", "todo.md"]);
    let typo = narrow("ntoes", work.iter().copied()).unwrap();
    assert_eq!((typo.exact.len(), typo.close.clone()), (0, vec![6]), "0 of 9 shown, 1 close match: notes.txt");
    let mut matcher = Matcher::new("ntoes").unwrap();
    assert_eq!(matcher.close("notes.txt"), Some(Close { edits: 1, score: 0, runs: vec![Run { start: 0, len: 5 }] }), "the swap washes notes");
    let exact = narrow("notes", work.iter().copied()).unwrap();
    assert_eq!((exact.exact, exact.close), (vec![6], vec![]), "any exact match hides every close one");
    let nothing = narrow("zzz", work.iter().copied()).unwrap();
    assert_eq!((nothing.exact.len(), nothing.close.len()), (0, 0), "a genuine miss has neither");
    assert_eq!(narrow("", work.iter().copied()), None, "no query is no filter");
}

#[test]
fn close_matches_rank_by_edits_then_score_then_listing_order() {
    let rows = names(&["xnxoxtxexsx", "notes-old", "the-notes"]);
    let ranked = narrow("ntes", rows.iter().copied()).unwrap();
    assert!(ranked.exact.is_empty());
    assert_eq!(ranked.close, vec![1, 2, 0], "a tie at 37 keeps listing order and the scattered 11 comes last");
    let typos = narrow("nottes", names(&["zzz-notes", "notes", "nottes-not"]).iter().copied()).unwrap();
    assert_eq!(typos.exact, vec![2], "an exact match still wins over two typos");
    let typos = narrow("nottes", names(&["zzz-notes", "notes", "tes"]).iter().copied()).unwrap();
    assert_eq!(typos.close, vec![0, 1], "equal edits keep listing order");
}

#[test]
fn the_typo_budget_follows_search_and_keeps_digits_and_punctuation_literal() {
    let close = |name: &str, query: &str| Matcher::new(query).unwrap().close(name);
    assert_eq!(close("cat.txt", "cta"), None, "three letters allow no edit");
    assert!(close("notes.txt", "ntoes").is_some(), "five letters allow one");
    assert_eq!(close("notes.txt", "ntoez"), None, "but not two");
    assert!(close("screenshot-2026.png", "scerenshto").is_some(), "ten letters allow two");
    assert_eq!(close("item-71.txt", "item-81"), None, "a digit is never edited");
    assert_eq!(close("item-71.txt", "item_71"), None, "nor is punctuation");
    assert_eq!(close("notes.txt", ".ntoes"), None, "a query led by punctuation allows no edit");
    assert_eq!(close("notes", "noes"), Some(Close { edits: 0, score: 37, runs: vec![Run { start: 0, len: 2 }, Run { start: 3, len: 2 }] }));
}

#[test]
fn folding_keeps_each_character_on_its_own_position() {
    assert_eq!(run("ÉCOLE-notes", "école"), Some(Run { start: 0, len: 5 }), "a capital beyond ASCII folds");
    assert_eq!(run("İstanbul", "i"), Some(Run { start: 0, len: 1 }), "a run inside a longer lowercase stays on the name's own char");
    assert_eq!(run("a b.txt", "a b"), Some(Run { start: 0, len: 3 }), "a space is a character like any other");
    assert_eq!(run("a b.txt", "a b.txt extra"), None, "a query longer than the name never matches");
}
