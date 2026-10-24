//! ui/js/Jump.js and ui/js/Fuzzy.js ported: the folders a name typed into the bar lists, best first, and the run each washes.
use super::{
    suggestions::{Ranker, Suggestion},
    Jumped,
};
use std::{cmp::Ordering, collections::HashMap};

// src/backend/fuzzy.rs's weights, which Fuzzy.js copies, so the bar and search agree on what matched.
const BONUS_CONSECUTIVE: i32 = 8;
const BONUS_BOUNDARY: i32 = 6;
const BONUS_BASENAME: i32 = 4;
const PENALTY_GAP: i32 = 1;
const MAX_STARTS: usize = 16;
// A source past its fifth match is a query one letter short; the next keystroke narrows it.
pub const SOURCE_ROWS: usize = 5;
// Past its first character a match earns, on average, what one character of a folder's own name earns.
const MIN_SCORE_PER_CHARACTER: i32 = BONUS_BASENAME;

#[derive(Clone, Copy)]
struct Folded {
    lower: char,
    upper: bool,
}

// corner: a character whose lowercase form is longer keeps its first character, as fuzzy.rs does.
fn fold(text: &str) -> Vec<Folded> {
    text.chars()
        .map(|c| {
            let lower = c.to_lowercase().next().unwrap_or(c);
            Folded {
                lower,
                upper: lower != c,
            }
        })
        .collect()
}

fn base_start(hay: &[Folded]) -> usize {
    hay.iter()
        .rposition(|c| c.lower == '/')
        .map_or(0, |at| at + 1)
}

fn is_separator(c: char) -> bool {
    matches!(c, '/' | '-' | '_' | '.' | ' ')
}

fn starts_a_word(hay: &[Folded], at: usize) -> bool {
    at == 0 || is_separator(hay[at - 1].lower) || (hay[at].upper && !hay[at - 1].upper)
}

fn character_score(hay: &[Folded], at: usize, base: usize, previous: Option<usize>) -> i32 {
    let mut score = match previous {
        Some(previous) if at == previous + 1 => BONUS_CONSECUTIVE,
        Some(previous) => -PENALTY_GAP * (at - previous - 1) as i32,
        None => 0,
    };
    if starts_a_word(hay, at) {
        score += BONUS_BOUNDARY;
    }
    if at >= base {
        score += BONUS_BASENAME;
    }
    score
}

// Greedy from one start: every query character takes the next candidate character that matches it.
fn align_from(
    hay: &[Folded],
    needle: &[char],
    start: usize,
    base: usize,
) -> Option<(i32, Vec<usize>)> {
    let (mut total, mut at, mut previous, mut positions) =
        (0, start, None, Vec::with_capacity(needle.len()));
    for (k, &wanted) in needle.iter().enumerate() {
        if k > 0 {
            at += 1 + hay.get(at + 1..)?.iter().position(|c| c.lower == wanted)?;
        }
        total += character_score(hay, at, base, previous);
        positions.push(at);
        previous = Some(at);
    }
    Some((total, positions))
}

// None when the needle is not a subsequence; otherwise the best of the first MAX_STARTS alignments.
fn match_folded(hay: &[Folded], base: usize, needle: &[char]) -> Option<(i32, Vec<usize>)> {
    let Some(&first) = needle.first() else {
        return Some((0, Vec::new()));
    };
    let mut best: Option<(i32, Vec<usize>)> = None;
    for start in hay
        .iter()
        .enumerate()
        .filter(|(_, c)| c.lower == first)
        .map(|(at, _)| at)
        .take(MAX_STARTS)
    {
        // A start that cannot finish means no later start can either, the greedy scan's own guarantee.
        let Some(found) = align_from(hay, needle, start, base) else {
            return best;
        };
        if best.as_ref().is_none_or(|best| found.0 > best.0) {
            best = Some(found);
        }
    }
    best
}

/// Fuzzy.match: the best alignment's score and the character positions it matched.
pub fn matched(candidate: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let hay = fold(candidate);
    let needle: Vec<char> = query.to_lowercase().chars().collect();
    match_folded(&hay, base_start(&hay), &needle)
}

/// Fuzzy.run: the longest stretch of consecutive positions, the first on a tie, as (start, length).
pub fn run(positions: &[usize]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < positions.len() {
        let mut j = i;
        while j + 1 < positions.len() && positions[j + 1] == positions[j] + 1 {
            j += 1;
        }
        if best.is_none_or(|(_, length)| j - i + 1 > length) {
            best = Some((positions[i], j - i + 1));
        }
        i = j + 1;
    }
    best
}

/// Jump.isQuery: a slash, a leading tilde, "." and ".." make the line a path; anything else is a name.
pub fn is_query(line: &str) -> bool {
    let line = line.trim();
    !(line.is_empty() || line.contains('/') || line == "." || line == ".." || line.starts_with('~'))
}

/// Jump.display: the path as the chrome writes it, a tilde for home.
pub fn display(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_owned(),
    }
}

// One candidate folded once per answer, so a keystroke never displays or folds anything.
struct Entry {
    path: String,
    text: String,
    // Each character's byte offset and the text's length, for the byte ranges a Suggestion carries.
    bytes: Vec<usize>,
    leaf: usize,
    whole: Vec<Folded>,
    base_whole: usize,
    base_leaf: usize,
    source: usize,
    at: usize,
    frecency: f64,
}

struct Found<'a> {
    entry: &'a Entry,
    own: bool,
    runs: usize,
    wash: (usize, usize),
}

/// The ranker the path bar ships: Jump.js's order over one answer's three sources.
#[derive(Default)]
pub struct Jump {
    entries: Vec<Entry>,
}

impl Jump {
    // Jump.rankedPrepared: None when the needle misses or only scatters below the floor.
    fn ranked<'a>(entry: &'a Entry, needle: &[char], floor: i32) -> Option<Found<'a>> {
        if entry.whole.len() < needle.len() {
            return None;
        }
        let (score, whole) = match_folded(&entry.whole, entry.base_whole, needle)?;
        if score < floor {
            return None;
        }
        let named = match_folded(&entry.whole[entry.leaf..], entry.base_leaf, needle);
        let positions = named.as_ref().map_or(&whole, |(_, named)| named);
        let runs = positions
            .windows(2)
            .filter(|pair| pair[1] != pair[0] + 1)
            .count()
            + usize::from(!positions.is_empty());
        let (start, length) = run(positions).unwrap_or((0, 0));
        let start = if named.is_some() {
            start + entry.leaf
        } else {
            start
        };
        Some(Found {
            entry,
            own: named.is_some(),
            runs,
            wash: (start, length),
        })
    }
}

fn before(a: &Found<'_>, b: &Found<'_>) -> Ordering {
    b.own
        .cmp(&a.own)
        .then(a.runs.cmp(&b.runs))
        .then(b.entry.frecency.total_cmp(&a.entry.frecency))
        .then(a.entry.source.cmp(&b.entry.source))
        .then(a.entry.at.cmp(&b.entry.at))
}

impl Ranker for Jump {
    fn prepare(&mut self, answer: &Jumped, home: &str) {
        let frecency: HashMap<&str, f64> = answer
            .frecency
            .iter()
            .map(|(path, score)| (path.as_str(), *score))
            .collect();
        self.entries.clear();
        for (source, paths) in [&answer.favourites, &answer.zoxide, &answer.recent]
            .into_iter()
            .enumerate()
        {
            for (at, path) in paths.iter().enumerate() {
                let text = display(path, home);
                let mut bytes: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
                bytes.push(text.len());
                let leaf = text
                    .rfind('/')
                    .map_or(0, |cut| text[..=cut].chars().count());
                let whole = fold(&text);
                let (base_whole, base_leaf) = (base_start(&whole), base_start(&whole[leaf..]));
                let frecency = frecency.get(path.as_str()).copied().unwrap_or(0.);
                self.entries.push(Entry {
                    path: path.clone(),
                    text,
                    bytes,
                    leaf,
                    whole,
                    base_whole,
                    base_leaf,
                    source,
                    at,
                    frecency,
                });
            }
        }
    }

    fn rows(&self, line: &str, out: &mut Vec<Suggestion>) {
        if !is_query(line) {
            return;
        }
        let query = line.trim();
        let needle: Vec<char> = query.to_lowercase().chars().collect();
        let floor = (query.chars().count() as i32 - 1) * MIN_SCORE_PER_CHARACTER;
        let mut found: Vec<Found<'_>> = self
            .entries
            .iter()
            .filter_map(|entry| Self::ranked(entry, &needle, floor))
            .collect();
        found.sort_by(before);
        let mut taken = [0; 3];
        for row in found {
            if taken[row.entry.source] == SOURCE_ROWS {
                continue;
            }
            taken[row.entry.source] += 1;
            let Entry {
                path,
                text,
                bytes,
                leaf,
                ..
            } = row.entry;
            let (start, length) = row.wash;
            out.push(Suggestion {
                path: path.clone(),
                text: text.clone(),
                leaf_start: bytes[*leaf],
                wash: bytes[start]..bytes[start + length],
            });
        }
    }

    fn is_query(&self, line: &str) -> bool {
        is_query(line)
    }
}
