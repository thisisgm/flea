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
    Folded {
        lower,
        upper: lower != c,
    }
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
            Hay::Ascii(bytes) => Folded {
                lower: (bytes[i] & 0x7F) as char,
                upper: bytes[i] & 0x80 != 0,
            },
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
        Fuzzy {
            needle: query.to_lowercase().chars().collect(),
            ascii: Vec::new(),
            wide: Vec::new(),
        }
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
        for (k, &needle) in self.needle.iter().enumerate() {
            if k > 0 {
                at += 1;
                while at < hay.len() && hay.get(at).lower != needle {
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
#[path = "fuzzy_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "fuzzy_packed_tests.rs"]
mod packed_tests;
