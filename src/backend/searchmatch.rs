// Typed search has its own matcher; folder jump keeps the legacy fuzzy scorer.
use super::searchbits;

pub(crate) const EXACT: i32 = 5_000_000;
pub(crate) const PREFIX: i32 = 4_000_000;
pub(crate) const WORD: i32 = 3_500_000;
pub(crate) const ALIGNED: i32 = 3_000_000;
pub(crate) const NAME: i32 = 2_000_000;
pub(crate) const PATH: i32 = 1_500_000;
pub(crate) const TYPO: i32 = 1_000_000;
const DEAD: i32 = i32::MIN / 4;

#[derive(Clone, Copy)]
pub(crate) struct Letter {
    pub lower: char,
    pub boundary: bool,
}

pub(crate) fn fold(bytes: &[u8], into: &mut Vec<Letter>) {
    into.clear();
    if bytes.is_ascii() {
        let mut before: Option<u8> = None;
        into.extend(bytes.iter().map(|&c| {
            let boundary = before.map_or(true, |p| separator(p as char) || (c.is_ascii_uppercase() && !p.is_ascii_uppercase()));
            before = Some(c);
            Letter { lower: c.to_ascii_lowercase() as char, boundary }
        }));
        return;
    }
    let mut before: Option<char> = None;
    for c in String::from_utf8_lossy(bytes).chars() {
        let boundary = before.map_or(true, |p| separator(p) || (c.is_uppercase() && !p.is_uppercase()));
        into.push(Letter { lower: c.to_lowercase().next().unwrap_or(c), boundary });
        before = Some(c);
    }
}

pub(crate) fn separator(c: char) -> bool { matches!(c, '/' | '-' | '_' | '.' | ' ') }

fn edit_budget(query: &str) -> usize {
    let count = query.chars().filter(|c| c.is_alphanumeric()).count();
    if query.chars().next().is_some_and(|c| !c.is_alphanumeric()) { 0 }
    else if count >= 8 { 2 } else if count >= 4 { 1 } else { 0 }
}

#[derive(Clone)]
pub(crate) struct State {
    best: Vec<i32>,
    end: Vec<i32>,
    pub hit: i32,
}

impl State {
    pub(crate) fn new(len: usize) -> Self {
        let mut best = vec![DEAD; len + 1];
        best[0] = 0;
        Self { best, end: vec![DEAD; len + 1], hit: if len == 0 { 0 } else { DEAD } }
    }

    pub(crate) fn resize(&mut self, len: usize) {
        self.best.resize(len + 1, DEAD);
        self.end.resize(len + 1, DEAD);
        self.reset();
    }

    pub(crate) fn copy_from(&mut self, other: &Self) {
        self.best.clone_from(&other.best);
        self.end.clone_from(&other.end);
        self.hit = other.hit;
    }

    pub(crate) fn reset(&mut self) {
        self.best.fill(DEAD);
        self.best[0] = 0;
        self.end.fill(DEAD);
        self.hit = if self.best.len() == 1 { 0 } else { DEAD };
    }

    // Descending query positions preserve the previous column without allocating another one.
    pub(crate) fn advance(&mut self, query: &[char], letters: &[Letter], basename: bool) {
        for letter in letters {
            for k in (1..=query.len()).rev() {
                let matched = if query[k - 1] == letter.lower {
                    self.best[k - 1].max(self.end[k - 1] + 8)
                        + i32::from(letter.boundary) * 6 + i32::from(basename) * 4
                } else { DEAD };
                self.best[k] = (self.best[k] - 1).max(matched);
                self.end[k] = matched;
            }
            self.hit = self.hit.max(self.end[query.len()]);
        }
    }

    pub(crate) fn score(&self, tier: i32) -> Option<i32> {
        (self.hit > DEAD / 2).then(|| tier + self.hit.clamp(-100_000, 100_000))
    }

}

pub(crate) struct Matcher {
    pub needle: Vec<char>,
    ascii: Option<Vec<u8>>,
    pub bits: u64,
    pub parent_bits: u64,
    pub suffix_bits: Vec<u64>,
    pub budget: usize,
    typo: super::searchtypo::Plan,
    pub prefix: usize,
    parent_typo: super::searchtypo::Plan,
}

impl Matcher {
    pub(crate) fn new(query: &str) -> Self {
        let needle: Vec<char> = query.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
        let budget = edit_budget(query);
        let typo = super::searchtypo::Plan::new(&needle);
        let prefix = needle.iter().rposition(|&c| c == '/').map_or(0, |at| at + 1);
        let parent_typo = super::searchtypo::Plan::new(&needle[..prefix]);
        let ascii = needle.iter().all(char::is_ascii).then(|| needle.iter().map(|&c| c as u8).collect());
        let parent: String = needle[..prefix].iter().collect();
        let mut suffix_bits = vec![0; needle.len() + 1];
        let mut encoded = [0; 4];
        for at in (0..needle.len()).rev() {
            suffix_bits[at] = suffix_bits[at + 1] | searchbits::mask(needle[at].encode_utf8(&mut encoded).as_bytes());
        }
        Self { needle, ascii, bits: searchbits::mask(query.as_bytes()), parent_bits: searchbits::mask(parent.as_bytes()), suffix_bits, budget, typo, prefix, parent_typo }
    }

    pub(crate) fn can_narrow(before: &str, now: &str) -> bool {
        if !now.starts_with(before) { return false; }
        let budget = edit_budget(before);
        // The first slash enables path typos, so it must not enlarge the match set.
        budget == edit_budget(now)
            && (budget == 0 || before.contains('/') || !now.contains('/'))
    }

    pub(crate) fn name(&self, letters: &[Letter], state: &mut State, edits: &mut super::searchtypo::Scratch) -> Option<i32> {
        if self.needle.is_empty() { return Some(0); }
        let prefix = |at: usize| letters.get(at..at + self.needle.len()).is_some_and(|part| {
            part.iter().zip(&self.needle).all(|(letter, query)| letter.lower == *query)
        });
        if prefix(0) { return Some(if letters.len() == self.needle.len() { EXACT } else { PREFIX }); }
        if (1..letters.len()).any(|at| letters[at].boundary && prefix(at)) { return Some(WORD); }
        let mut at = 0;
        for letter in letters {
            if self.needle.get(at) == Some(&letter.lower) { at += 1; }
        }
        if at == self.needle.len() {
            state.reset();
            state.advance(&self.needle, letters, true);
            return state.score(NAME);
        }
        self.distance(letters, edits).map(|distance| TYPO - distance as i32 * 1000)
    }

    pub(crate) fn name_bytes(&self, bytes: &[u8], letters: &mut Vec<Letter>, state: &mut State, edits: &mut super::searchtypo::Scratch) -> Option<i32> {
        if let Some(query) = self.ascii.as_ref().filter(|_| bytes.is_ascii()) {
            return ascii::name(self, query, bytes, letters, state, edits);
        }
        fold(bytes, letters);
        self.name(letters, state, edits)
    }

    pub(crate) fn reach(&self, bytes: &[u8], mut at: usize) -> usize {
        if bytes.is_ascii() {
            for &byte in bytes {
                if self.needle.get(at) == Some(&(byte.to_ascii_lowercase() as char)) { at += 1; }
            }
        } else {
            for c in String::from_utf8_lossy(bytes).chars() {
                if self.needle.get(at) == Some(&c.to_lowercase().next().unwrap_or(c)) { at += 1; }
            }
        }
        at
    }

    pub(crate) fn distance(&self, letters: &[Letter], scratch: &mut super::searchtypo::Scratch) -> Option<usize> {
        self.typo.distance(&self.needle, letters, self.budget, scratch)
    }

    pub(crate) fn parent_possible(&self, letters: &[Letter], scratch: &mut super::searchtypo::Scratch) -> bool {
        self.parent_typo.distance(&self.needle[..self.prefix], letters, self.budget, scratch).is_some()
    }

    #[cfg(test)]
    pub(crate) fn full(&self, path: &[u8]) -> Option<i32> {
        let mut letters = Vec::new();
        fold(path, &mut letters);
        let base = path.iter().rposition(|&b| b == b'/').map_or(0, |at| at + 1);
        let mut leaf = Vec::new();
        fold(&path[base..], &mut leaf);
        let mut state = State::new(self.needle.len());
        let name = self.name_bytes(&path[base..], &mut leaf, &mut state, &mut super::searchtypo::Scratch::default());
        if name.is_some_and(|score| score >= NAME) || base == 0 { return name; }
        state = State::new(self.needle.len());
        let split = letters.len() - leaf.len();
        state.advance(&self.needle, &letters[..split], false);
        state.advance(&self.needle, &leaf, true);
        let mut score = name.max(state.score(if self.needle.contains(&'/') { ALIGNED } else { PATH }));
        if self.needle.contains(&'/') {
            score = score.max(self.distance(&letters, &mut super::searchtypo::Scratch::default())
                .map(|distance| TYPO - distance as i32 * 1000));
        }
        score
    }
}

#[path = "searchascii.rs"]
pub(crate) mod ascii;

#[cfg(test)]
#[path = "searchmatch_tests.rs"]
pub(crate) mod tests;
