// The scoring half of docs/protocol.md "search": a home-directory subsequence match needs ranking by run, boundary and basename to stay usable.
use crate::backend::sort::name_order;
use std::cmp::Ordering;

// A character matching right after the previous one, which is what makes a contiguous run win.
const BONUS_CONSECUTIVE: i32 = 8;
// The first character of the candidate, of a path segment, of a word, or of a camelCase hump.
const BONUS_BOUNDARY: i32 = 6;
// The match is in the file's own name rather than in a parent directory the query merely passed through.
const BONUS_BASENAME: i32 = 4;
// Charged per candidate character skipped between two matches, so a scattered match sinks.
const PENALTY_GAP: i32 = 1;
// Past this many first-character occurrences only the first ones score: an unseen alignment is not worth an unbounded scan.
const MAX_STARTS: usize = 16;

// The characters a word or a path segment starts after.
fn is_separator(c: char) -> bool {
    c == '/' || c == '-' || c == '_' || c == '.' || c == ' '
}

// corner: a multi-char lowercase (Turkish dotted capital I) keeps only its first char, as ui/js/Match.js does.
#[derive(Clone, Copy)]
pub(crate) struct Folded {
    lower: char,
    upper: bool,
}

// One folded haystack in either stored form: packed ASCII bytes or packed wide words.
#[derive(Clone, Copy)]
pub(crate) enum Hay<'a> {
    Ascii(&'a [u8]),
    Wide(&'a [u32]),
}

// One candidate folded once, packed tight: ASCII rows cost one byte per char, wide rows one word.
#[cfg(test)]
pub(crate) enum Packed {
    Ascii(Vec<u8>),
    Wide(Vec<u32>),
}

// One ASCII byte folded and packed; lowered ASCII never sets bit 7, so the upper flag rides it.
fn pack_byte(byte: u8) -> u8 {
    let folded = fold(byte as char);
    (folded.lower as u8) | (u8::from(folded.upper) << 7)
}

// A wide word holds the lowered char above the upper flag.
fn pack_char(c: char) -> u32 {
    let folded = fold(c);
    ((folded.lower as u32) << 1) | u32::from(folded.upper)
}

// Folds one candidate into a fresh packed form, for rows that outlive the call.
#[cfg(test)]
pub(crate) fn fold_packed(candidate: &str) -> Packed {
    if candidate.is_ascii() {
        Packed::Ascii(candidate.bytes().map(pack_byte).collect())
    } else {
        Packed::Wide(candidate.chars().map(pack_char).collect())
    }
}

// Byte-order subsequence on the folded haystack, the prefilter before any scoring.
pub(crate) fn is_subsequence(needle: &[char], hay: Hay) -> bool {
    let mut pos = 0;
    for n in needle {
        while pos < hay.len() && hay.get(pos).lower != *n {
            pos += 1;
        }
        if pos == hay.len() {
            return false;
        }
        pos += 1;
    }
    true
}

fn fold(c: char) -> Folded {
    let lower = c.to_lowercase().next().unwrap_or(c);
    Folded { lower, upper: lower != c }
}

// Where the candidate's own name begins: the character after its last path separator.
fn base_start(hay: Hay) -> usize {
    let mut start = 0;
    for i in 0..hay.len() {
        if hay.get(i).lower == '/' {
            start = i + 1;
        }
    }
    start
}

impl<'a> Hay<'a> {
    fn len(&self) -> usize {
        match self {
            Hay::Ascii(bytes) => bytes.len(),
            Hay::Wide(words) => words.len(),
        }
    }

    // The one accessor both stored forms score through; the alignment never learns which it holds.
    fn get(&self, i: usize) -> Folded {
        match self {
            Hay::Ascii(bytes) => Folded { lower: (bytes[i] & 0x7F) as char, upper: bytes[i] & 0x80 != 0 },
            Hay::Wide(words) => Folded {
                lower: char::from_u32(words[i] >> 1).unwrap_or('\u{FFFD}'),
                upper: words[i] & 1 != 0,
            },
        }
    }
}

// The folded query plus one reusable candidate buffer, so a subtree walk allocates twice rather than once per entry.
pub struct Fuzzy {
    needle: Vec<char>,
    ascii: Vec<u8>,
    wide: Vec<u32>,
}

impl Fuzzy {
    pub fn new(query: &str) -> Fuzzy {
        Fuzzy { needle: query.to_lowercase().chars().collect(), ascii: Vec::new(), wide: Vec::new() }
    }

    // None means the query is no subsequence of the candidate; Some carries the best alignment's score, higher winning.
    pub fn score(&mut self, candidate: &str) -> Option<i32> {
        // An empty query matches everything, which is what an empty search line shows.
        if self.needle.is_empty() {
            return Some(0);
        }
        if candidate.is_ascii() {
            self.ascii.clear();
            self.ascii.extend(candidate.bytes().map(pack_byte));
            self.score_pre_folded(Hay::Ascii(&self.ascii))
        } else {
            self.wide.clear();
            self.wide.extend(candidate.chars().map(pack_char));
            self.score_pre_folded(Hay::Wide(&self.wide))
        }
    }

    // Scores a caller-folded haystack with this needle; answers exactly what score would.
    pub fn score_pre_folded(&self, hay: Hay) -> Option<i32> {
        if self.needle.is_empty() {
            return Some(0);
        }
        if !is_subsequence(&self.needle, hay) {
            return None;
        }
        let base = base_start(hay);
        let first = self.needle[0];
        let mut best: Option<i32> = None;
        let mut starts = 0;
        for i in 0..hay.len() {
            if hay.get(i).lower != first {
                continue;
            }
            match self.score_from(hay, i, base) {
                Some(s) => best = Some(best.map_or(s, |b| if s > b { s } else { b })),
                // A greedy scan takes the earliest position per character, so a start that cannot finish means no later start can.
                None => return best,
            }
            starts += 1;
            if starts == MAX_STARTS {
                break;
            }
        }
        best
    }

    // Greedy alignment from one start: each needle character takes its next match, the hand-traced alignment.
    fn score_from(&self, hay: Hay, start: usize, base: usize) -> Option<i32> {
        let mut total = 0;
        let mut at = start;
        let mut previous: Option<usize> = None;
        for k in 0..self.needle.len() {
            if k > 0 {
                at += 1;
                while at < hay.len() && hay.get(at).lower != self.needle[k] {
                    at += 1;
                }
                if at == hay.len() {
                    return None;
                }
            }
            total += Self::character_score(hay, at, base, previous);
            previous = Some(at);
        }
        Some(total)
    }

    // One matched character's worth: run, boundary and basename bonuses minus the gap charge.
    fn character_score(hay: Hay, at: usize, base: usize, previous: Option<usize>) -> i32 {
        let mut score = 0;
        match previous {
            Some(p) if at == p + 1 => score += BONUS_CONSECUTIVE,
            Some(p) => score -= PENALTY_GAP * ((at - p - 1) as i32),
            None => {}
        }
        if starts_a_word(hay, at) {
            score += BONUS_BOUNDARY;
        }
        if at >= base {
            score += BONUS_BASENAME;
        }
        score
    }
}

fn starts_a_word(hay: Hay, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let before = hay.get(at - 1);
    is_separator(before.lower) || (hay.get(at).upper && !before.upper)
}

// Result order: better score, then shorter path, then name order, so one walk answers in exactly one order.
pub fn rank_order(a_score: i32, a_name: &str, b_score: i32, b_name: &str) -> Ordering {
    match b_score.cmp(&a_score) {
        Ordering::Equal => {}
        other => return other,
    }
    match a_name.len().cmp(&b_name.len()) {
        Ordering::Equal => {}
        other => return other,
    }
    name_order(a_name.as_bytes(), b_name.as_bytes())
}

#[cfg(test)]
mod tests {
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
        assert!(s_run > s_scattered, "run {} scattered {}", s_run, s_scattered);
    }

    #[test]
    fn a_boundary_start_beats_one_inside_a_word() {
        let s_boundary = score("my-notes.txt", "notes").unwrap();
        let s_inside = score("bignotes.txt", "notes").unwrap();
        assert!(s_boundary > s_inside, "boundary {} inside {}", s_boundary, s_inside);
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
            assert_eq!(fuzzy.score(candidate), Fuzzy::new("bench").score(candidate), "{candidate} scores as a fresh fold does, a wide one between ASCII ones included");
        }
        assert_eq!(fuzzy.ascii.len(), "docs/bench.txt".len(), "the last ASCII score replaced the buffer, never appended");
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
        let table = src.split("var SCORES = [").nth(1).expect("tests/js/jump.js carries SCORES").split("]]").next().expect("SCORES closes");
        let mut out = Vec::new();
        for entry in table.split("],") {
            let entry = entry.trim().trim_start_matches('[').trim();
            if entry.is_empty() { continue; }
            let (hay, rest) = match entry.strip_prefix('"') {
                Some(quoted) => {
                    let end = quoted.find('"').expect("a SCORES row closes its file");
                    (quoted[..end].to_string(), &quoted[end + 1..])
                }
                // The one row jump.js builds programmatically rather than spelling out.
                None => {
                    assert!(entry.starts_with("bounded + \"ab\""), "a SCORES row names its file first: {entry}");
                    (bounded.to_string(), &entry["bounded + \"ab\"".len()..])
                }
            };
            let rest = &rest[rest.find('"').expect("a SCORES row names its query") + 1..];
            let end = rest.find('"').expect("a SCORES row closes its query");
            let (query, rest) = (rest[..end].to_string(), &rest[end + 1..]);
            let digits = rest.rsplit(',').next().expect("a SCORES row carries its score").trim().trim_end_matches(']');
            out.push((hay, query, match digits {
                "null" => None,
                _ => Some(digits.parse().expect("a SCORES score parses as an integer")),
            }));
        }
        out
    }

    // SCORES is read out of tests/js/jump.js, so a weight changed here fails this test until the JS table is changed with it.
    #[test]
    fn the_exact_scores_the_jump_port_mirrors() {
        let bounded = format!("{}ab", "ax".repeat(16));
        let rows = js_scores(include_str!("../../tests/js/jump.js"), &bounded);
        assert_eq!(rows.len(), 15, "SCORES in tests/js/jump.js grew or shrank; change this row count with it");
        for (hay, query, expected) in &rows {
            assert_eq!(score(hay, query), *expected, "{query} against {hay}");
        }
    }
}

#[cfg(test)]
#[path = "fuzzy_packed_tests.rs"]
mod packed_tests;
