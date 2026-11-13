//! ui/js/Fuzzy.js matchFolded: the greedy subsequence alignment and its score, the close pass's first stage.
use super::{is_separator, Matcher, BONUS_BASENAME, BONUS_BOUNDARY, BONUS_CONSECUTIVE, MAX_STARTS, PENALTY_GAP};

impl Matcher {
    fn starts_a_word(&self, at: usize) -> bool {
        at == 0 || {
            let before = self.hay[at - 1];
            is_separator(before.lower) || (self.hay[at].upper && !before.upper)
        }
    }

    // ui/js/Fuzzy.js matchFolded with its positions: greedy from each start, the best score winning, the first on a tie.
    pub(in crate::ui_logic_listing_filter) fn align(&self) -> Option<(i32, Vec<usize>)> {
        // Only the winning start records its positions, so a scan allocates once per close row.
        let (score, start, base) = self.best()?;
        crate::ui_logic_listing_filter::recorded();
        let mut positions = Vec::with_capacity(self.needle.len());
        crate::ui_logic_listing_filter::pass();
        self.align_from(start, base, Some(&mut positions));
        Some((score, positions))
    }

    // The winning score, its start and the basename's first position, with no positions recorded.
    pub(super) fn best(&self) -> Option<(i32, usize, usize)> {
        crate::ui_logic_listing_filter::pass();
        let base = self.hay.iter().rposition(|c| c.lower == '/').map_or(0, |at| at + 1);
        let mut best: Option<(i32, usize)> = None;
        let mut starts = 0;
        for i in 0..self.hay.len() {
            if self.hay[i].lower != self.needle[0] {
                continue;
            }
            // A start that cannot finish means no later start can either, the greedy scan's own guarantee.
            let Some(score) = self.align_from(i, base, None) else { break };
            if best.is_none_or(|(top, _)| score > top) {
                best = Some((score, i));
            }
            starts += 1;
            if starts == MAX_STARTS {
                break;
            }
        }
        best.map(|(score, start)| (score, start, base))
    }

    fn align_from(&self, start: usize, base: usize, mut positions: Option<&mut Vec<usize>>) -> Option<i32> {
        let (mut total, mut at, mut previous) = (0, start, None::<usize>);
        for (k, want) in self.needle.iter().enumerate() {
            if k > 0 {
                at += 1;
                while at < self.hay.len() && self.hay[at].lower != *want {
                    at += 1;
                }
                if at == self.hay.len() {
                    return None;
                }
            }
            total += match previous {
                Some(p) if at == p + 1 => BONUS_CONSECUTIVE,
                Some(p) => -PENALTY_GAP * (at - p - 1) as i32,
                None => 0,
            };
            if self.starts_a_word(at) {
                total += BONUS_BOUNDARY;
            }
            if at >= base {
                total += BONUS_BASENAME;
            }
            if let Some(positions) = positions.as_mut() {
                positions.push(at);
            }
            previous = Some(at);
        }
        Some(total)
    }
}
