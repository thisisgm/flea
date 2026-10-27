// Banded optimal-string-alignment distance at word starts, with literal digits and punctuation.
use super::searchmatch::{Letter, ascii::{Ascii, Text}};

// Each row is two layers: index 0 without a dropped query letter, index 1 with one, which may only end at a word end.
#[derive(Default)]
pub(crate) struct Scratch {
    older: [Vec<usize>; 2],
    previous: [Vec<usize>; 2],
    current: [Vec<usize>; 2],
}

fn editable(c: char) -> bool { c.is_alphabetic() }

// A match only a query letter dropped inside a word gives is reported SHADOW edits further: narrowing keeps it, answers never list it.
pub(crate) const SHADOW: usize = 8;

// The listed distance, else the shadow of an unrestricted one, so the match set still only shrinks as the query grows.
fn reported(best: usize, plain: usize, budget: usize) -> Option<usize> {
    if best <= budget { Some(best) } else { (plain <= budget).then_some(SHADOW + plain) }
}

fn word_end(hay: &[Letter], at: usize) -> bool {
    at == hay.len() || !editable(hay[at].lower) || hay[at].boundary
}

impl Scratch {
    pub(crate) fn distance(&mut self, query: &[char], hay: &[Letter], budget: usize) -> Option<usize> {
        if budget == 0 { return None; }
        let (mut best, mut plain) = (budget + 1, budget + 1);
        for start in 0..hay.len() {
            if !hay[start].boundary || hay.len() - start + budget < query.len() { continue; }
            let len = (hay.len() - start).min(query.len() + budget);
            for layer in 0..2 {
                self.older[layer].resize(len + 1, budget + 1);
                self.previous[layer].resize(len + 1, budget + 1);
                self.current[layer].resize(len + 1, budget + 1);
                self.previous[layer].fill(budget + 1);
            }
            self.previous[0][0] = 0;
            for j in 1..=len.min(budget) {
                self.previous[0][j] = if editable(hay[start + j - 1].lower) { self.previous[0][j - 1] + 1 } else { budget + 1 };
            }
            for i in 1..=query.len() {
                for layer in 0..2 { self.current[layer].fill(budget + 1); }
                if editable(query[i - 1]) { self.current[1][0] = self.previous[0][0].min(self.previous[1][0]) + 1; }
                let mut low = self.current[1][0];
                for j in i.saturating_sub(budget).max(1)..=len.min(i + budget) {
                    let (a, b) = (query[i - 1], hay[start + j - 1].lower);
                    for layer in 0..2 {
                        let previous = &self.previous[layer];
                        let mut value = if a == b { previous[j - 1] }
                            else if editable(a) && editable(b) { previous[j - 1] + 1 } else { budget + 1 };
                        if layer == 1 && editable(a) { value = value.min(self.previous[0][j].min(self.previous[1][j]) + 1); }
                        if editable(b) { value = value.min(self.current[layer][j - 1] + 1); }
                        if i > 1 && j > 1 && a == hay[start + j - 2].lower && query[i - 2] == b && editable(a) && editable(b) {
                            value = value.min(self.older[layer][j - 2] + 1);
                        }
                        self.current[layer][j] = value.min(budget + 1);
                        low = low.min(value);
                    }
                }
                std::mem::swap(&mut self.older, &mut self.previous);
                std::mem::swap(&mut self.previous, &mut self.current);
                if low > budget { break; }
                if i == query.len() {
                    for j in query.len().saturating_sub(budget)..=len {
                        best = best.min(self.previous[0][j]);
                        plain = plain.min(self.previous[0][j]).min(self.previous[1][j]);
                        if word_end(hay, start + j) { best = best.min(self.previous[1][j]); }
                    }
                }
            }
        }
        reported(best, plain, budget)
    }
}

// Short ASCII queries use the same edit automaton packed into three bounded bit sets.
pub(crate) struct Plan {
    equal: [u64; 128],
    letters: u64,
    goal: u64,
    segments: Vec<(usize, u32)>,
    literals: Vec<char>,
}

impl Plan {
    pub(crate) fn new(query: &[char]) -> Self {
        let mut plan = Self { equal: [0; 128], letters: 0, goal: 0, segments: Vec::new(), literals: Vec::new() };
        if query.len() >= 64 || !query.iter().all(char::is_ascii) { return plan; }
        plan.goal = 1 << query.len();
        let (mut len, mut bits) = (0, 0);
        for (at, &c) in query.iter().enumerate() {
            plan.equal[c as usize] |= 1 << at;
            if editable(c) {
                plan.letters |= 1 << at;
                len += 1;
                bits |= 1 << (c.to_ascii_lowercase() as u8 - b'a');
            } else {
                plan.segments.push((len, bits));
                plan.literals.push(c);
                (len, bits) = (0, 0);
            }
        }
        plan.segments.push((len, bits));
        plan
    }

    // An edit cannot cross a literal, so length and missing-letter costs add across its segments.
    fn possible(&self, hay: impl Text, budget: usize) -> bool {
        if !self.trailing_possible(hay, budget) { return false; }
        for start in 0..hay.len() {
            if !hay.boundary(start) { continue; }
            let (mut at, mut spent, mut valid) = (start, 0, true);
            for (part, &(wanted, required)) in self.segments.iter().enumerate() {
                let last = part == self.literals.len();
                let (mut len, mut bits, mut over) = (0usize, 0u32, false);
                while at < hay.len() && hay.alphabetic(at) {
                    if len == wanted + budget {
                        over = !last;
                        break;
                    }
                    let c = hay.lower(at);
                    if c.is_ascii_alphabetic() { bits |= 1 << (c.to_ascii_lowercase() as u8 - b'a'); }
                    len += 1;
                    at += 1;
                }
                let length = if last { wanted.saturating_sub(len) } else { wanted.abs_diff(len) };
                spent += length.max((required & !bits).count_ones() as usize);
                if over || spent > budget { valid = false; break; }
                if last { return true; }
                if at == hay.len() || hay.lower(at) != self.literals[part] {
                    valid = false;
                    break;
                }
                at += 1;
            }
            if valid { return true; }
        }
        false
    }

    // The segment before the last literal must fit immediately before one occurrence of that literal.
    fn trailing_possible(&self, hay: impl Text, budget: usize) -> bool {
        let Some(&literal) = self.literals.last() else { return true };
        let part = self.literals.len() - 1;
        let (wanted, required) = self.segments[part];
        for end in (0..hay.len()).rev() {
            if hay.lower(end) != literal { continue; }
            let (mut at, mut len, mut bits) = (end, 0usize, 0u32);
            while at > 0 && len < wanted + budget && hay.alphabetic(at - 1) {
                at -= 1;
                len += 1;
                let c = hay.lower(at);
                if c.is_ascii_alphabetic() { bits |= 1 << (c.to_ascii_lowercase() as u8 - b'a'); }
            }
            if part > 0 && at > 0 && hay.alphabetic(at - 1) { continue; }
            let length = if part == 0 { wanted.saturating_sub(len) } else { wanted.abs_diff(len) };
            if length.max((required & !bits).count_ones() as usize) <= budget { return true; }
        }
        false
    }

    pub(crate) fn distance(&self, query: &[char], hay: &[Letter], budget: usize, scratch: &mut Scratch) -> Option<usize> {
        if budget == 0 { return None; }
        if self.goal == 0 { return scratch.distance(query, hay, budget); }
        self.bit_distance(hay, budget)
    }

    pub(crate) fn ascii_distance(&self, hay: Ascii<'_>, budget: usize) -> Option<Option<usize>> {
        if budget == 0 { return Some(None); }
        if self.goal == 0 { return None; }
        Some(self.bit_distance(hay, budget))
    }

    fn bit_distance(&self, hay: impl Text, budget: usize) -> Option<usize> {
        if !self.possible(hay, budget) { return None; }
        // Layer 0 holds states without a dropped query letter, layer 1 states with one, which may only end at a word end.
        let mut start = [[0u64; 3]; 2];
        start[0] = [1; 3];
        for e in 1..=budget { start[1][e] = start[1][e - 1] | (((start[0][e - 1] | start[1][e - 1]) & self.letters) << 1); }
        let mut previous = [[0u64; 3]; 2];
        let mut older = [[0u64; 3]; 2];
        let mut last = '\0';
        let mut last_editable = false;
        let (mut best, mut plain) = (budget + 1, budget + 1);
        for at in 0..hay.len() {
            let lower = hay.lower(at);
            let editable = hay.alphabetic(at);
            if hay.boundary(at) {
                for layer in 0..2 { for e in 0..=budget { previous[layer][e] |= start[layer][e]; } }
            }
            let equal = self.equal.get(lower as usize).copied().unwrap_or(0);
            let before = if last_editable { self.equal.get(last as usize).copied().unwrap_or(0) } else { 0 };
            let ends = at + 1 == hay.len() || !hay.alphabetic(at + 1) || hay.boundary(at + 1);
            let mut current = [[0u64; 3]; 2];
            for e in 0..=budget {
                for layer in 0..2 {
                    current[layer][e] = (previous[layer][e] & equal) << 1;
                    if e > 0 {
                        current[layer][e] |= current[layer][e - 1];
                        if editable {
                            current[layer][e] |= previous[layer][e - 1] | ((previous[layer][e - 1] & self.letters) << 1);
                            current[layer][e] |= (older[layer][e - 1] & equal & (before >> 1)) << 2;
                        }
                    }
                }
                if e > 0 { current[1][e] |= ((current[0][e - 1] | current[1][e - 1]) & self.letters) << 1; }
                if current[0][e] & self.goal != 0 || (ends && current[1][e] & self.goal != 0) { best = best.min(e); }
                if (current[0][e] | current[1][e]) & self.goal != 0 { plain = plain.min(e); }
            }
            older = previous;
            previous = current;
            last = lower;
            last_editable = editable;
        }
        reported(best, plain, budget)
    }
}
