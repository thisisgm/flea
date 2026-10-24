//! The close pass's typo match, bounded so a name it can never fit costs a count and one it can fills only a band of the table.
use super::{Matcher, Name, Run, MAX_STARTS};

/// One of the query's 2k+1 pieces and its failure table, built once per query.
pub(super) struct Piece {
    from: usize,
    to: usize,
    fail: Vec<usize>,
}

impl Piece {
    /// The piece's first query char, the letter its every occurrence starts with.
    pub(super) fn first(&self, needle: &[char]) -> Option<char> {
        (self.from < self.to).then(|| needle[self.from])
    }
}

/// The pieces a typo budget cuts the query into; none without a budget.
pub(super) fn pieces(needle: &[char], budget: u8) -> Vec<Piece> {
    let (m, count) = (needle.len(), 2 * usize::from(budget) + 1);
    if budget == 0 {
        return Vec::new();
    }
    (0..count)
        .map(|piece| {
            let (from, to) = (piece * m / count, (piece + 1) * m / count);
            let want = &needle[from..to];
            let (mut fail, mut matched) = (vec![0; want.len()], 0);
            for at in 1..want.len() {
                while matched > 0 && want[at] != want[matched] {
                    matched = fail[matched - 1];
                }
                if want[at] == want[matched] {
                    matched += 1;
                }
                fail[at] = matched;
            }
            Piece { from, to, fail }
        })
        .collect()
}

// An edit across a digit or punctuation costs past every budget, so those characters stay literal.
const NEVER: u16 = u16::MAX / 4;

impl Matcher {
    /// Whether the unit-cost bound admits a name within the budget; a name it rejects is past the budget in the band too.
    pub(in crate::ui_logic_sr2) fn admits_typo(&self, name: Name) -> bool {
        self.bound.admits(name.bytes().map(|b| char::from(b.to_ascii_lowercase())), u32::from(self.budget))
    }

    /// The query's letters only, so the bound alone counts its typo edits (test builds read it).
    #[cfg(test)]
    pub(in crate::ui_logic_sr2) fn letters(&self) -> bool {
        self.letters
    }

    /// The typo budget the query earns (test builds read it).
    #[cfg(test)]
    pub(in crate::ui_logic_sr2) fn budget(&self) -> u8 {
        self.budget
    }

    // Each edit removes at most one query char from the name it must fit, so a short name or a missing letter rules it out; `lowered` is the name's ASCII chars folded.
    pub(super) fn can_fit(&self, chars: usize, lowered: impl Iterator<Item = u8>) -> bool {
        let budget = usize::from(self.budget);
        if chars + budget < self.needle.len() {
            return false;
        }
        let mut have = [0u16; 128];
        for c in lowered {
            have[usize::from(c)] = have[usize::from(c)].saturating_add(1);
        }
        let mut short = 0;
        for &(c, count) in &self.need {
            short += usize::from(count.saturating_sub(have[usize::from(c)]));
            if short > budget {
                return false;
            }
        }
        true
    }

    // An edit breaks at most two of 2k+1 pieces, so a match holds one piece verbatim and keeps within k diagonals of it.
    fn band(&mut self) {
        let k = isize::from(self.budget);
        let Self { needle, hay, pieces, diags, scans, .. } = self;
        diags.clear();
        for piece in pieces.iter() {
            let want = &needle[piece.from..piece.to];
            if want.is_empty() || want.len() > hay.len() {
                continue;
            }
            // Each piece scans the loaded name once, so each scan is one counted pass.
            crate::ui_logic_sr2::pass();
            // Knuth-Morris-Pratt, so finding a piece compares at most two chars per name char whatever the query.
            let (mut matched, mut found) = (0, 0);
            for (at, c) in hay.iter().enumerate() {
                loop {
                    *scans += 1;
                    if want[matched] == c.lower {
                        matched += 1;
                        break;
                    }
                    if matched == 0 {
                        break;
                    }
                    matched = piece.fail[matched - 1];
                }
                if matched == want.len() {
                    matched = piece.fail[matched - 1];
                    // An occurrence leaving too little name before or after it for the rest of the query within k edits holds no match.
                    let start = (at + 1 - want.len()) as isize;
                    let (before, after) = (start - piece.from as isize, (hay.len() - at - 1) as isize - (needle.len() - piece.to) as isize);
                    if before < -k || after < -k {
                        continue;
                    }
                    diags.extend(before - k..=before + k);
                    // Each piece keeps the scorer's own start cap, so one piece's starts never hide another's.
                    found += 1;
                    if found == MAX_STARTS {
                        break;
                    }
                }
            }
        }
        diags.sort_unstable();
        diags.dedup();
    }

    // Approximate substring by optimal string alignment over the band: replace, insert, delete and swap neighbours, letters only.
    pub(super) fn typo(&mut self) -> Option<(u8, Run)> {
        self.typos += 1;
        self.band();
        let (m, n, w) = (self.needle.len(), self.hay.len() as isize, self.diags.len());
        let letter = |c: char| c.is_alphabetic();
        let budget = u16::from(self.budget);
        for (row, starts) in self.rows.iter_mut().zip(&mut self.starts) {
            row.clear();
            row.resize(w, NEVER);
            starts.clear();
            starts.resize(w, 0);
        }
        // Slot s of a row is the cell on diagonal diags[s]; row 0 lets a match start anywhere on the band for free.
        for (slot, &diagonal) in self.diags.iter().enumerate() {
            if (0..=n).contains(&diagonal) {
                self.rows[0][slot] = 0;
                self.starts[0][slot] = diagonal as usize;
            }
        }
        let mut was_over = false;
        for i in 1..=m {
            let (cur, prev, back) = (i % 3, (i - 1) % 3, (i + 1) % 3);
            let q = self.needle[i - 1];
            let delete = if letter(q) { 1 } else { NEVER };
            self.cells += w as u64;
            let mut least = NEVER;
            for slot in 0..w {
                let diagonal = self.diags[slot];
                let j = i as isize + diagonal;
                if !(0..=n).contains(&j) {
                    self.rows[cur][slot] = NEVER;
                    continue;
                }
                let j = j as usize;
                let wider = slot + 1 < w && self.diags[slot + 1] == diagonal + 1;
                let narrower = slot > 0 && self.diags[slot - 1] == diagonal - 1;
                let mut best = (NEVER, 0);
                if wider {
                    best = (self.rows[prev][slot + 1].saturating_add(delete), self.starts[prev][slot + 1]);
                }
                if j > 0 {
                    let h = self.hay[j - 1].lower;
                    let same = if q == h { 0 } else if letter(q) && letter(h) { 1 } else { NEVER };
                    let mut options = [(self.rows[prev][slot].saturating_add(same), self.starts[prev][slot]), (NEVER, 0), (NEVER, 0)];
                    if narrower {
                        let insert = if letter(h) { 1 } else { NEVER };
                        options[1] = (self.rows[cur][slot - 1].saturating_add(insert), self.starts[cur][slot - 1]);
                    }
                    if i > 1 && j > 1 && q != h && q == self.hay[j - 2].lower && self.needle[i - 2] == h && letter(q) && letter(h) {
                        options[2] = (self.rows[back][slot].saturating_add(1), self.starts[back][slot]);
                    }
                    // The diagonal step is weighed first, then the gap, then the insert, then the swap, as the full table weighed them.
                    let (diag, rest) = (options[0], [best, options[1], options[2]]);
                    best = diag;
                    for option in rest {
                        if option.0 < best.0 {
                            best = option;
                        }
                    }
                }
                self.rows[cur][slot] = best.0.min(NEVER);
                self.starts[cur][slot] = best.1;
                least = least.min(self.rows[cur][slot]);
            }
            // Two rows past the budget leave no cell a later row can build under it, a swap reaching back only one.
            let over = least > budget;
            if over && was_over {
                return None;
            }
            was_over = over;
        }
        let last = m % 3;
        let ends = (0..w).map(|slot| (slot, m as isize + self.diags[slot])).filter(|&(_, j)| (1..=n).contains(&j));
        let (slot, end) = ends.min_by_key(|&(slot, j)| (self.rows[last][slot], j))?;
        let edits = self.rows[last][slot];
        (edits <= budget).then(|| {
            let start = self.starts[last][slot];
            (edits as u8, Run { start, len: end as usize - start })
        })
    }
}
