//! What a key costs once the close pass is bounded: one walk, one pattern test per name, a unit-cost gate and a linear piece search.
use super::tests_fixture::{realistic_names, Rng};
use super::*;
use std::cell::Cell;

fn counted<T>(work: impl FnOnce() -> T) -> (T, u64) {
    STEPS.with(|steps| steps.set(0));
    let out = work();
    (out, STEPS.with(Cell::get))
}

fn rows(names: &[String]) -> impl Iterator<Item = (usize, &str)> + Clone + '_ {
    names.iter().enumerate().map(|(row, name)| (row, name.as_str()))
}

fn folded(name: &str) -> Vec<char> {
    name.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

#[test]
fn a_pattern_is_tested_once_per_name_from_its_start() {
    for len in [20usize, 80, 255] {
        let names: Vec<String> = (0..1_000).map(|row| format!("{row:05}{}", "x".repeat(len - 5))).collect();
        for query in ["*.pdf", "*q*z", "IMG_*_14"] {
            let (found, cost) = counted(|| narrow(query, rows(&names)).unwrap());
            assert!(found.exact.is_empty() && found.close.is_empty(), "{query} matches no name");
            let bound = 1_000 * (2 * len + 2 * query.len() + 4) as u64;
            assert!(cost <= bound, "{query} over {len}-char names took {cost} steps, linear allows {bound}");
        }
    }
    let names = ["Report 2025.pdf", "notes.txt", "draft.PDF.bak", "pdf"].map(String::from);
    assert_eq!(narrow("*.pdf", rows(&names)).unwrap().exact, vec![0, 2], "a pattern still matches anywhere in the name");
    assert_eq!(narrow("rep*5", rows(&names)).unwrap().exact, vec![0], "and spans the stretch a star takes");
}

// Unit-cost optimal string alignment as an approximate substring: replace, insert, delete and swap neighbours all cost one.
fn unit_distance(needle: &[char], hay: &[char]) -> usize {
    let (m, n) = (needle.len(), hay.len());
    let mut d = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for i in 1..=m {
        for j in 1..=n {
            let mut best = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + usize::from(needle[i - 1] != hay[j - 1]));
            if i > 1 && j > 1 && needle[i - 1] == hay[j - 2] && needle[i - 2] == hay[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    (1..=n).map(|j| d[m][j]).min().unwrap_or(m)
}

#[test]
fn a_zero_exact_key_walks_the_names_once_and_bands_only_what_the_unit_cost_bound_admits() {
    let names = &realistic_names()[..20_000];
    for (query, budget) in [("kubere", 1), ("kuberent", 2), ("quartelr", 2), ("meeting nots", 2), ("zqxj", 1)] {
        let walks = Cell::new(0usize);
        let counted = names.iter().enumerate().map(|(row, name)| {
            walks.set(walks.get() + 1);
            (row, name.as_str())
        });
        let found = narrow(query, counted).unwrap();
        assert!(found.exact.is_empty(), "{query} has no exact match in the fixture");
        assert_eq!(walks.get(), names.len(), "{query} read each name once, exact and close together");
        let mut matcher = Matcher::new(query).unwrap();
        let close = names.iter().filter(|name| matcher.close(name).is_some()).count();
        assert_eq!(close, found.close.len());
        let needle: Vec<char> = query.chars().collect();
        let admitted = names.iter().filter(|name| unit_distance(&needle, &folded(name)) <= budget).count() as u64;
        assert!(matcher.typos() <= admitted, "{query}: {} names reached the band, {admitted} are within {budget} unit edits", matcher.typos());
    }
}

#[test]
fn the_piece_search_compares_linear_in_the_name_at_4_19_43_and_200_letters() {
    let name = "a".repeat(255);
    for (len, budget) in [(4usize, 1usize), (19, 2), (43, 2), (200, 2)] {
        let pieces = 2 * budget + 1;
        // A b closing the first piece: its prefix matches at every position of the name, the piece itself never.
        let mut query = "a".repeat(len);
        query.replace_range(len / pieces - 1..len / pieces, "b");
        let mut matcher = Matcher::new(&query).unwrap();
        let close = (0..10).filter(|_| matcher.close(&name).is_some()).count();
        assert_eq!(close, 10, "{len} letters: one substitution fits");
        // At most two compares per char read: the whole name for the piece that never occurs, up to 16 usable starts past its end for each other.
        let bound = 10 * 2 * (name.len() + (pieces - 1) * (len + 16)) as u64;
        assert!(matcher.scans() <= bound, "{len} letters compared {} chars, linear allows {bound}", matcher.scans());
    }
}

// Full-table optimal string alignment with the band's weights, the reference a close match is held to.
fn full_typo(needle: &[char], hay: &[char], budget: u16) -> Option<u16> {
    const NEVER: u16 = u16::MAX / 4;
    let letter = |c: char| c.is_alphabetic();
    let (m, n) = (needle.len(), hay.len());
    let mut d = vec![vec![NEVER; n + 1]; m + 1];
    d[0].fill(0);
    for i in 1..=m {
        let q = needle[i - 1];
        for j in 0..=n {
            let mut best = d[i - 1][j].saturating_add(if letter(q) { 1 } else { NEVER });
            if j > 0 {
                let h = hay[j - 1];
                let same = if q == h { 0 } else if letter(q) && letter(h) { 1 } else { NEVER };
                best = best.min(d[i - 1][j - 1].saturating_add(same));
                best = best.min(d[i][j - 1].saturating_add(if letter(h) { 1 } else { NEVER }));
                if i > 1 && j > 1 && q != h && q == hay[j - 2] && needle[i - 2] == h && letter(q) && letter(h) {
                    best = best.min(d[i - 2][j - 2].saturating_add(1));
                }
            }
            d[i][j] = best.min(NEVER);
        }
    }
    let edits = (1..=n).map(|j| d[m][j]).min()?;
    (edits <= budget).then_some(edits)
}

#[test]
fn every_piece_keeps_its_own_starts_so_the_intact_one_is_banded() {
    let name = format!("{} item.txt", "i".repeat(20));
    assert_eq!(Matcher::new("itme").unwrap().close(&name).map(|close| close.edits), Some(1), "twenty i's before item still leave its swap");
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let (mut tried, mut differ) = (0usize, Vec::new());
    for _ in 0..30_000 {
        let hay: String = (0..5 + rng.below(40)).map(|_| ['a', 'b', 'c', 'd', 'e', ' ', '-'][rng.below(7)]).collect();
        let query: String = (0..4 + rng.below(8)).map(|_| ['a', 'b', 'c', 'd', 'e'][rng.below(5)]).collect();
        let mut probe = Matcher::new(&query).unwrap();
        probe.load(Name::of(&hay));
        if probe.align().is_some() {
            continue;
        }
        tried += 1;
        let budget = if query.len() >= 8 { 2 } else { 1 };
        let want = full_typo(&query.chars().collect::<Vec<_>>(), &hay.chars().collect::<Vec<_>>(), budget);
        let got = Matcher::new(&query).unwrap().close(&hay).map(|close| u16::from(close.edits));
        if want != got {
            differ.push((hay, query, want, got));
        }
    }
    assert!(tried > 10_000, "the subsequence pass leaves most cases to the typo match");
    assert!(differ.is_empty(), "{} of {tried} differ from the full table, first {:?}", differ.len(), differ.first());
}

#[test]
fn ranking_close_rows_records_no_positions() {
    let names: Vec<String> = (0..2_000).map(|row| format!("Screenshot {row:04}.png")).collect();
    RECORDED.with(|recorded| recorded.set(0));
    let found = narrow("scrnsht", rows(&names)).unwrap();
    assert_eq!((found.exact.len(), found.close.len()), (0, 2_000), "every name holds scrnsht as a subsequence");
    assert_eq!(RECORDED.with(Cell::get), 0, "narrow ranks by edits and score, so no row records its positions");
    let mut matcher = Matcher::new("scrnsht").unwrap();
    assert!(names[..10].iter().all(|name| matcher.close(name).is_some_and(|close| !close.runs.is_empty())), "a drawn row still gets its runs");
    assert_eq!(RECORDED.with(Cell::get), 10);
}
