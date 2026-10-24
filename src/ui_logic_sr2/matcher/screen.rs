//! The walk's read of a name: the letters a close match needs and whether the query is a subsequence, found in one walk of its bytes after the ASCII test.
use super::{Matcher, Name, MAX_STARTS};

// The distinct letters one pass counts on the stack; a query with more is read the folded way.
const SLOTS: usize = 31;

/// How a name the walk read stands before it is weighed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Too short, too many letters missing, or no typo budget for a name that does not align.
    No,
    /// An ASCII name holding the ASCII query as a subsequence, so the greedy alignment finds one.
    Subsequence,
    /// An ASCII name that does not align but holds the letters a typo needs.
    Typo,
    /// A `Typo` name holding some piece's first letter more often than the band keeps starts, so only the band ranks it.
    Crowded,
    /// A name read only for its letter count, weighed once it is folded.
    Folded,
}

/// The query's ASCII letters as the walk reads them, built once per query.
pub(super) struct Slots {
    // Each lowered ASCII byte's place in `need`, counted from one; zero for a byte the query lacks.
    of: [u8; 128],
    // The query's bytes and a byte no ASCII name holds, so the subsequence step never runs off the end.
    trail: Vec<u8>,
    // The slot of each typo piece's first letter, counted as the name is read.
    firsts: Vec<usize>,
}

impl Slots {
    pub(super) fn new(ascii: Option<&[u8]>, need: &[(u8, u16)], firsts: impl Iterator<Item = char>) -> Self {
        let mut of = [0u8; 128];
        for (slot, &(c, _)) in need.iter().enumerate() {
            if let (Some(at), Ok(slot)) = (of.get_mut(usize::from(c)), u8::try_from(slot + 1)) {
                *at = slot;
            }
        }
        let trail = ascii.map(|needle| needle.iter().copied().chain([u8::MAX]).collect()).unwrap_or_default();
        let firsts = firsts.filter(char::is_ascii).map(|c| usize::from(of[c as usize])).collect();
        Self { of, trail, firsts }
    }
}

impl Matcher {
    /// False only when the screen proves no exact match: an exact match is a subsequence, and a pattern is never screened.
    pub fn may_hold(&self, fit: Fit) -> bool {
        self.wildcard || matches!(fit, Fit::Subsequence | Fit::Folded)
    }

    /// Whether the query holds 31 or more distinct ASCII letters, so an ASCII name is screened the folded way (test builds read it).
    #[cfg(test)]
    pub(in crate::ui_logic_sr2) fn wide(&self) -> bool {
        self.need.len() >= SLOTS
    }

    /// The route a name takes to be weighed; an ASCII name pairs its letter count with the subsequence test in one pass.
    pub fn screen(&self, name: &str) -> Fit {
        if self.wildcard {
            return Fit::No;
        }
        let name = Name::of(name);
        if self.ascii.is_none() || !name.is_ascii() || self.need.len() >= SLOTS {
            let fits = !name.is_ascii() || self.can_fit(name.len(), name.bytes().map(|b| b.to_ascii_lowercase()));
            return if fits { Fit::Folded } else { Fit::No };
        }
        let budget = usize::from(self.budget);
        if name.len() + budget < self.needle.len() {
            return Fit::No;
        }
        let (trail, of) = (&self.slots.trail, &self.slots.of);
        let (mut have, mut next) = ([0u16; SLOTS + 1], 0);
        for b in name.bytes().map(|b| b.to_ascii_lowercase()) {
            let slot = usize::from(of[usize::from(b & 0x7f)]).min(SLOTS);
            have[slot] = have[slot].saturating_add(1);
            next += usize::from(trail[next] == b);
        }
        if next + 1 == trail.len() {
            return Fit::Subsequence;
        }
        if budget == 0 {
            return Fit::No;
        }
        let mut short = 0;
        for (slot, &(_, count)) in self.need.iter().enumerate() {
            short += usize::from(count.saturating_sub(have[slot + 1]));
            if short > budget {
                return Fit::No;
            }
        }
        // The band keeps a piece's first MAX_STARTS starts, so a name that may hold more is ranked by the band, never by the bound alone.
        if self.slots.firsts.iter().any(|&slot| usize::from(have[slot.min(SLOTS)]) > MAX_STARTS) {
            return Fit::Crowded;
        }
        Fit::Typo
    }
}
