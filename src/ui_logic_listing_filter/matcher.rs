//! Filter040's matcher: a substring, a * and ? pattern, and close matches asked for only when nothing matched exactly.
use super::narrow::runs_of;
use super::{pass, Name};
use glob::glob_prefix;
pub use screen::Fit;

mod align;
mod bound;
mod fold;
mod glob;
mod screen;
mod typo;

/// One stretch of a name the row washes, counted in chars of the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub len: usize,
}

/// A close match: letter edits it took, the fuzzy score of a subsequence alignment, and the stretches it matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Close {
    pub edits: u8,
    pub score: i32,
    pub runs: Vec<Run>,
}

// The weights of the backend's fuzzy scorer, so a close match ranks as search ranks it.
const BONUS_CONSECUTIVE: i32 = 8;
const BONUS_BOUNDARY: i32 = 6;
const BONUS_BASENAME: i32 = 4;
const PENALTY_GAP: i32 = 1;
const MAX_STARTS: usize = 16;

#[derive(Clone, Copy)]
struct Folded {
    lower: char,
    upper: bool,
}

// ui/js/Fuzzy.js fold: a char whose lowercase form is longer keeps its first char, so the scorer's positions stay one to one.
fn fold(c: char) -> Folded {
    let lower = c.to_lowercase().next().unwrap_or(c);
    Folded { lower, upper: lower != c }
}

fn is_separator(c: char) -> bool {
    matches!(c, '/' | '-' | '_' | '.' | ' ')
}

/// The query folded once, with buffers a whole-listing scan reuses, so a scan allocates per match and not per name.
pub struct Matcher {
    needle: Vec<char>,
    // The needle's bytes for the backend's ASCII test, when the query is plain ASCII text.
    ascii: Option<Vec<u8>>,
    // How often each ASCII char stands in the needle, the letters a close match must find or pay an edit for.
    need: Vec<(u8, u16)>,
    slots: screen::Slots,
    wildcard: bool,
    // A pattern behind an implied leading star, so membership is one test from the name's start.
    starred: Vec<char>,
    budget: u8,
    // A query of up to 64 ASCII letters, whose typo edits the bound alone counts exactly.
    letters: bool,
    pieces: Vec<typo::Piece>,
    bound: bound::Bound,
    hay: Vec<Folded>,
    lowered: Vec<char>,
    origin: Vec<usize>,
    rows: [Vec<u16>; 3],
    starts: [Vec<usize>; 3],
    // The diagonals the typo table fills, picked by where the query's pieces stand in the name.
    diags: Vec<isize>,
    cells: u64,
    // Chars the piece search compared and names that reached the typo band, the close pass's other costs.
    scans: u64,
    typos: u64,
}

impl Matcher {
    /// None for an empty query, which is no filter at all.
    pub fn new(query: &str) -> Option<Self> {
        if query.is_empty() {
            return None;
        }
        // The whole query lowered at once, as JS toLowerCase and the backend's str::to_lowercase lower it.
        let needle: Vec<char> = query.to_lowercase().chars().collect();
        let wildcard = needle.iter().any(|c| matches!(c, '*' | '?'));
        let letters = needle.iter().filter(|c| c.is_alphanumeric()).count();
        // Search's typo budget: none below four letters or digits, one up to seven, two from eight, none when punctuation leads.
        let budget = match letters {
            _ if wildcard || !needle[0].is_alphanumeric() => 0,
            0..=3 => 0,
            4..=7 => 1,
            _ => 2,
        };
        let ascii = (!wildcard && query.is_ascii()).then(|| needle.iter().map(|&c| c as u8).collect());
        let mut need: Vec<(u8, u16)> = Vec::new();
        for c in needle.iter().filter(|c| c.is_ascii()).map(|&c| c as u8) {
            match need.iter_mut().find(|(have, _)| *have == c) {
                Some((_, count)) => *count = count.saturating_add(1),
                None => need.push((c, 1)),
            }
        }
        let starred = if wildcard { std::iter::once('*').chain(needle.iter().copied()).collect() } else { Vec::new() };
        let (pieces, bound) = (typo::pieces(&needle, budget), bound::Bound::new(&needle));
        let slots = screen::Slots::new(ascii.as_deref(), &need, pieces.iter().filter_map(|piece| piece.first(&needle)));
        let letters = needle.len() <= 64 && needle.iter().all(char::is_ascii_alphabetic);
        let (hay, lowered, origin, diags) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (rows, starts) = (Default::default(), Default::default());
        Some(Self { needle, ascii, need, slots, wildcard, starred, budget, letters, pieces, bound, hay, lowered, origin, rows, starts, diags, cells: 0, scans: 0, typos: 0 })
    }

    /// Distance table cells filled so far, the close pass's cost.
    pub fn cells(&self) -> u64 {
        self.cells
    }

    /// Chars compared while finding the query's pieces in names, the band's search cost.
    pub fn scans(&self) -> u64 {
        self.scans
    }

    /// Names that reached the typo band.
    pub fn typos(&self) -> u64 {
        self.typos
    }

    pub fn is_wildcard(&self) -> bool {
        self.wildcard
    }

    // The name folded into `hay`; true when it is plain ASCII.
    pub(super) fn load(&mut self, name: Name) -> bool {
        self.hay.clear();
        let ascii = name.is_ascii();
        if ascii {
            self.hay.extend(name.bytes().map(|b| Folded { lower: char::from(b.to_ascii_lowercase()), upper: b.is_ascii_uppercase() }));
        } else {
            self.hay.extend(name.chars().map(fold));
        }
        ascii
    }

    /// Whether the name holds an exact match, all a narrow needs; a pattern is tested once, never from every start.
    pub fn holds(&mut self, name: &str) -> bool {
        let name = Name::of(name);
        if let (Some(needle), true) = (&self.ascii, name.is_ascii()) {
            return name.as_bytes().windows(needle.len()).any(|part| part.eq_ignore_ascii_case(needle));
        }
        self.lower(name);
        pass();
        if self.wildcard {
            glob_prefix(&self.lowered, &self.starred).is_some()
        } else {
            self.lowered.windows(self.needle.len()).any(|part| part == self.needle.as_slice())
        }
    }

    /// The run an exact match washes: the first case-folded occurrence, or the first stretch a pattern matches.
    pub fn exact(&mut self, name: &str) -> Option<Run> {
        let name = Name::of(name);
        if let (Some(needle), true) = (&self.ascii, name.is_ascii()) {
            // src/backend/filter.rs matches an ASCII pair byte for byte, so this does too.
            let found = name.as_bytes().windows(needle.len()).position(|part| part.eq_ignore_ascii_case(needle));
            return found.map(|start| Run { start, len: needle.len() });
        }
        self.lower(name);
        pass();
        let (hay, needle) = (&self.lowered, &self.needle);
        let found = if self.wildcard {
            (0..=hay.len()).find_map(|start| glob_prefix(&hay[start..], needle).map(|len| (start, len)))
        } else {
            hay.windows(needle.len()).position(|part| part == needle.as_slice()).map(|start| (start, needle.len()))
        };
        found.map(|(start, len)| self.span(start, len))
    }

    /// A subsequence alignment first, as search's fuzzy scorer finds one, else a typo within the budget; never for a pattern.
    pub fn close(&mut self, name: &str) -> Option<Close> {
        let fit = self.screen(name);
        self.close_fitting(name, fit)
    }

    /// Whether `screen` leaves the name a route to a close match.
    pub fn fits(&self, name: &str) -> bool {
        self.screen(name) != Fit::No
    }

    /// The close match of a name by the route `screen` gave it, with the runs it washes.
    pub fn close_fitting(&mut self, name: &str, fit: Fit) -> Option<Close> {
        self.weigh(Name::of(name), fit, true)
    }

    /// The edits and score of a name by the route `screen` gave it, all a narrow needs, with no runs built.
    pub fn rank_fitting(&mut self, name: &str, fit: Fit) -> Option<(u8, i32)> {
        self.weigh(Name::of(name), fit, false).map(|close| (close.edits, close.score))
    }

    // A name with other chars is counted once it is folded; `runs` false leaves the runs empty and allocates nothing.
    fn weigh(&mut self, name: Name, fit: Fit, runs: bool) -> Option<Close> {
        match fit {
            Fit::No => return None,
            // A name that does not align goes straight to the bound, unloaded.
            Fit::Typo | Fit::Crowded => {
                // Ranking needs only the edits, which the bound counts exactly for a query of letters, so the band runs only for drawn rows.
                if fit == Fit::Typo && self.letters && !runs {
                    let edits = self.bound.least_in_words(&name.as_bytes(), u32::from(self.budget))?;
                    return Some(Close { edits: edits as u8, score: 0, runs: Vec::new() });
                }
                if !self.admits_typo(name) {
                    return None;
                }
                self.load(name);
                return self.typo_close(runs);
            }
            Fit::Subsequence | Fit::Folded => {}
        }
        let ascii = self.load(name);
        if !ascii && fit == Fit::Folded {
            pass();
            if !self.can_fit(self.hay.len(), self.hay.iter().map(|c| c.lower).filter(char::is_ascii).map(|c| c as u8)) {
                return None;
            }
        }
        if !runs {
            if let Some((score, _, _)) = self.best() {
                return Some(Close { edits: 0, score, runs: Vec::new() });
            }
        } else if let Some((score, positions)) = self.align() {
            return Some(Close { edits: 0, score, runs: runs_of(&positions) });
        }
        if self.budget == 0 {
            return None;
        }
        pass();
        if !self.bound.admits(self.hay.iter().map(|c| c.lower), u32::from(self.budget)) {
            return None;
        }
        self.typo_close(runs)
    }


    fn typo_close(&mut self, runs: bool) -> Option<Close> {
        self.typo().map(|(edits, run)| Close { edits, score: 0, runs: if runs { vec![run] } else { Vec::new() } })
    }
}
