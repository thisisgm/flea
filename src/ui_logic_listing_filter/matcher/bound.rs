//! A unit-cost lower bound on the typo match, Hyyro's bit-parallel search with transpositions, one word per name char.

/// The query's first 64 chars as match masks; a longer query is bounded by its first 64, never above its own distance.
pub(super) struct Bound {
    ascii: [u64; 128],
    wide: Vec<(char, u64)>,
    top: u64,
    len: u32,
}

impl Bound {
    pub(super) fn new(needle: &[char]) -> Self {
        let mut bound = Self { ascii: [0; 128], wide: Vec::new(), top: 0, len: 0 };
        for (at, &c) in needle.iter().take(64).enumerate() {
            let bit = 1u64 << at;
            if c.is_ascii() {
                bound.ascii[c as usize] |= bit;
            } else if let Some((_, mask)) = bound.wide.iter_mut().find(|(have, _)| *have == c) {
                *mask |= bit;
            } else {
                bound.wide.push((c, bit));
            }
            bound.top = bit;
            bound.len += 1;
        }
        bound
    }

    fn mask(&self, c: char) -> u64 {
        if c.is_ascii() {
            self.ascii[c as usize]
        } else {
            self.wide.iter().find(|(have, _)| *have == c).map_or(0, |(_, mask)| *mask)
        }
    }

    // Every edit the band weighs costs at least one here, so a name this rejects is past the budget in the band too.
    pub(super) fn admits(&self, hay: impl Iterator<Item = char>, budget: u32) -> bool {
        let mut column = Column::new(self.len);
        hay.into_iter().any(|c| column.push(self.mask(c), self.top) <= budget)
    }

    /// The typo match's edits for a query of letters only: no edit may touch a digit or punctuation, so a match lies inside one run of letters, where every edit costs one.
    pub(super) fn least_in_words(&self, name: &[u8], budget: u32) -> Option<u32> {
        let (mut column, mut least) = (Column::new(self.len), u32::MAX);
        for &b in name {
            if !b.is_ascii_alphabetic() {
                column = Column::new(self.len);
                continue;
            }
            least = least.min(column.push(self.ascii[usize::from(b.to_ascii_lowercase())], self.top));
            // A name off the subsequence route holds no exact match, so one edit is the least it can take.
            if least <= 1 {
                break;
            }
        }
        (least <= budget).then_some(least)
    }
}

/// One column of the search: the vertical rises and falls, the diagonal zeros and the match mask before it, as Myers and Hyyro name them VP, VN, D0, PM, and the last row's distance.
struct Column {
    up: u64,
    down: u64,
    zero: u64,
    before: u64,
    score: u32,
}

impl Column {
    fn new(len: u32) -> Self {
        Self { up: !0, down: 0, zero: 0, before: 0, score: len }
    }

    // One name char with match mask `eq`; returns the distance of the best match ending at it.
    #[inline(always)]
    fn push(&mut self, eq: u64, top: u64) -> u32 {
        let Self { up, down, zero, before, score } = self;
        let swap = ((!*zero & eq) << 1) & *before;
        *zero = (((eq & *up).wrapping_add(*up)) ^ *up) | eq | *down | swap;
        let (plus, minus) = (*down | !(*zero | *up), *up & *zero);
        if plus & top != 0 {
            *score += 1;
        } else if minus & top != 0 {
            *score -= 1;
        }
        // Row 0 stays zero across the name, so a match may start anywhere: no carry into the lowest bit.
        let (plus, minus) = (plus << 1, minus << 1);
        *up = minus | !(*zero | plus);
        *down = plus & *zero;
        *before = eq;
        *score
    }
}
