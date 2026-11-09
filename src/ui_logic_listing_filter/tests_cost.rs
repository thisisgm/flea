//! What a keystroke costs over 100,000 rows, counted in steps and table cells, and the folding the oracle and the backend share.
use super::*;

fn counted<T>(work: impl FnOnce() -> T) -> (T, u64) {
    STEPS.with(|steps| steps.set(0));
    let out = work();
    (out, STEPS.with(std::cell::Cell::get))
}

#[test]
fn prune_keep_and_cut_build_one_set_per_answer_over_100k_marked_rows() {
    let all: Vec<usize> = (0..100_000).collect();
    let tenth: Vec<usize> = (0..100_000).step_by(10).collect();
    let bound = |inputs: usize| 3 * inputs as u64 + 64;
    let (hidden, cost) = counted(|| pruned(&all, Some(&all)));
    assert!(hidden.is_empty());
    assert!(cost <= bound(all.len() * 2), "pruning every mark against every row took {cost} steps");
    let (hidden, cost) = counted(|| pruned(&all, Some(&tenth)));
    assert_eq!(hidden.len(), 90_000);
    assert!(cost <= bound(all.len() + tenth.len()), "pruning to one row in ten took {cost} steps");
    let (kept, cost) = counted(|| keep(&all, Some(&tenth)));
    assert_eq!(kept, tenth);
    assert!(cost <= bound(all.len() + tenth.len()), "keeping took {cost} steps");
    let pending: Vec<usize> = (0..100_000).rev().collect();
    let ((ask, drop), cost) = counted(|| cut(&tenth, &[5, 5], Some(&tenth), &pending));
    assert_eq!((ask.len(), drop.len()), (10_000, 90_001), "every hidden pending row drops once, the doubled drop kept as asked");
    assert!(cost <= bound(tenth.len() * 2 + pending.len() + 2), "cutting took {cost} steps");
}

#[test]
fn close_matching_skips_every_name_it_can_never_fit() {
    let names: Vec<String> = (0..100_000).map(|row| format!("item-{row:06}.txt")).collect();
    let mut missing = Matcher::new("zzzz").unwrap();
    assert!(names.iter().all(|name| missing.exact(name).is_none() && missing.close(name).is_none()));
    assert_eq!(missing.cells(), 0, "letters the name does not hold rule it out before any table");
    let long: String = "abcdefghij".chars().cycle().take(4096).collect();
    let mut pasted = Matcher::new(&long).unwrap();
    assert!(names[..10_000].iter().all(|name| pasted.close(name).is_none()));
    assert_eq!(pasted.cells(), 0, "a query longer than the name plus its budget never fills a table");
    let mut typo = Matcher::new("itme").unwrap();
    let close = names[..1_000].iter().filter(|name| typo.close(name).is_some()).count();
    assert_eq!(close, 1_000, "a real typo still finds every name it fits");
    assert!(typo.cells() <= 1_000 * 4 * 15, "and fills at most one table row per query letter: {}", typo.cells());
}

#[test]
fn folding_lowers_the_whole_string_as_the_oracle_and_the_backend_do() {
    let exact = |name: &str, query: &str| Matcher::new(query).unwrap().exact(name);
    assert_eq!(exact("καφες.txt", "ΚΑΦΕΣ"), Some(Run { start: 0, len: 5 }), "a final capital sigma lowers to the final form");
    assert_eq!(exact("ΟΔΟΣ.txt", "οδος"), None, "a sigma a letter follows stays the medial form");
    assert_eq!(exact("İstanbul.txt", "ist"), None, "a dotted capital I lowers to i and a combining dot");
    assert_eq!(exact("İSTANBUL", "istanbul"), None, "so it never matches a plain i");
    assert_eq!(exact("İstanbul.txt", "stan"), Some(Run { start: 1, len: 4 }), "and a run after it stays on the name's own chars");
    assert_eq!(exact("İstanbul.txt", "İst"), Some(Run { start: 0, len: 3 }), "the same capital matches itself");
    assert_eq!(narrow("ΚΑΦΕΣ", [(0usize, "καφες.txt")]).unwrap().exact, vec![0], "so Ctrl+A takes it as exact");
}

#[test]
fn between_takes_view_positions_so_ranked_close_rows_stay_whole() {
    assert_eq!(between(&[6, 2, 9], 0, 2), vec![6, 2, 9], "a range over three drawn rows takes all three in drawn order");
    assert_eq!(between(&[6, 2, 9], 2, 1), vec![2, 9], "either end may come first");
    assert_eq!(between(&[6, 2, 9], 1, 7), vec![2, 9], "an end past the drawn rows stops at the last");
}

// Cells a close pass fills for each name, bounded by the band its exact pieces pick, never the whole query by name table.
fn cells_per_name(names: &[String], query: &str) -> (usize, u64) {
    let mut matcher = Matcher::new(query).unwrap();
    let close = names.iter().filter(|name| matcher.close(name).is_some()).count();
    (close, matcher.cells())
}

#[test]
fn the_close_pass_fills_a_band_linear_in_the_name_at_4_19_and_43_letters() {
    let items: Vec<String> = (0..2_000).map(|row| format!("item-{row:06}.txt")).collect();
    let (close, cells) = cells_per_name(&items, "itme");
    assert_eq!(close, 2_000, "every item name is one swap from itme");
    assert!(cells <= 2_000 * 3 * 15, "4 letters, budget 1: at most 3 cells per name char, {cells} filled");
    let fox: Vec<String> = (0..2_000).map(|row| format!("the quick brown fox jumps over the lazy dog {row:05}.txt")).collect();
    for query in ["teh quick brown fox", "teh quick brown fox jumps over the lazy dog"] {
        let (close, cells) = cells_per_name(&fox, query);
        assert_eq!(close, 2_000, "{query} is one swap from every name");
        assert!(cells <= 2_000 * 5 * 53, "{} letters, budget 2: at most 5 cells per name char, {cells} filled", query.len());
    }
}

#[test]
fn a_name_full_of_the_querys_pieces_stops_at_the_scorers_start_cap() {
    let names: Vec<String> = (0..10).map(|_| "a".repeat(255)).collect();
    let query = format!("{}bb", "a".repeat(200));
    let (close, cells) = cells_per_name(&names, &query);
    assert_eq!(close, 10, "two substitutions still fit");
    assert!(cells <= 10 * 5 * 16 * 5 * 202, "each of 5 pieces keeps 16 starts of 5 diagonals at most, {cells} filled");
}

// Most whole-name passes one name takes in a zero-exact walk, by route; the per-route derivation is in NOTES.md (FIX8).
fn most_passes(matcher: &Matcher, name: &str, fit: matcher::Fit) -> u64 {
    let k = u64::from(matcher.budget());
    match fit {
        // A pattern screens No at once, but narrow still holds every name: is_ascii, lower, one pass.
        matcher::Fit::No if matcher.is_wildcard() => 4 + u64::from(!name.is_ascii()) + u64::from(name.contains('\u{3a3}')),
        // A query of 31 or more distinct ASCII letters screens an ASCII name the folded way: is_ascii twice, bytes once.
        matcher::Fit::No if matcher.wide() => 3,
        matcher::Fit::No => 2,
        matcher::Fit::Subsequence => 7,
        matcher::Fit::Typo if matcher.letters() => 3,
        matcher::Fit::Typo | matcher::Fit::Crowded => {
            if matcher.admits_typo(Name::of(name)) {
                6 + 2 * k
            } else {
                3
            }
        }
        matcher::Fit::Folded => 13 + 2 * k,
    }
}

// The walk `narrow` runs on a zero-exact key, one name at a time, checking each name's passes against its route's bound.
fn zero_exact_passes(key: &str, names: &[String]) -> (u64, u64) {
    let mut matcher = Matcher::new(key).unwrap();
    let (mut passes_total, mut bound_total) = (0, 0);
    for name in names {
        let before = PASSES.with(std::cell::Cell::get);
        let fit = matcher.screen(name);
        if matcher.may_hold(fit) {
            assert!(!matcher.holds(name), "{key} holds {name:?}, so it is no zero-exact key");
        }
        if fit != matcher::Fit::No {
            let _ = matcher.rank_fitting(name, fit);
        }
        let passes = PASSES.with(std::cell::Cell::get) - before;
        let most = most_passes(&matcher, name, fit);
        assert!(passes <= most, "{key}: {name:?} takes {passes} passes as a {fit:?} name, its route allows {most}");
        passes_total += passes;
        bound_total += most;
    }
    (passes_total, bound_total)
}

// Letters only, whose typo names the bound ranks alone; a space, and a digit with `_`, whose typo names the bound admits and the band scans; a 36-char query and a pattern reach the No routes they miss.
const ZERO_EXACT: [&str; 10] = ["reel", "reec", "reus", "msuic", "kuberent", "lact", "meeting nots", "IGM_2025", "abcdefghijklmnopqrstuvwxyz0123456789", "zq*xj"];

#[test]
fn a_zero_exact_key_takes_no_more_passes_over_a_name_than_its_route_allows() {
    let names = super::tests_fixture::realistic_names();
    for key in ZERO_EXACT {
        let (replayed, bound) = zero_exact_passes(key, &names);
        PASSES.with(|passes| passes.set(0));
        let out = narrow(key, names.iter().enumerate().map(|(row, name)| (row, name.as_str()))).unwrap();
        let passes = PASSES.with(std::cell::Cell::get);
        assert!(out.exact.is_empty(), "{key} has an exact match in the fixture");
        assert!(passes <= bound, "{key}: narrow took {passes} passes over {} names, its routes allow {bound}", names.len());
        println!("PASSES key={key:?} names={} passes={passes} replayed={replayed} bound={bound}", names.len());
    }
}

#[test]
fn each_scan_of_a_held_name_slice_is_a_pass() {
    PASSES.with(|passes| passes.set(0));
    let name = Name::of("reel.txt");
    let text = name.as_bytes();
    assert!(text.iter().all(u8::is_ascii));
    assert!(text.windows(4).any(|part| part == b"reel"));
    assert_eq!(PASSES.with(std::cell::Cell::get), 2, "two scans of one held slice are two passes");
}
