// Natural path ordering streams borrowed components and defers the raw-byte tie break.
use super::*;
use std::iter::Peekable;

#[cfg(test)]
thread_local! {
    static CUT: std::cell::Cell<std::time::Duration> = const { std::cell::Cell::new(std::time::Duration::ZERO) };
}

#[cfg(test)]
pub(crate) fn cut_elapsed() -> std::time::Duration { CUT.with(std::cell::Cell::get) }

// The raw path of one row, yielded byte by byte from the root without building it.
#[derive(Clone)]
struct PathBytes<'a> {
    index: &'a Index,
    parent: u32,
    leaf: u32,
    remaining: usize,
    bytes: &'a [u8],
    at: usize,
}

impl<'a> PathBytes<'a> {
    fn part(index: &'a Index, mut parent: u32, levels: usize) -> &'a [u8] {
        for _ in 0..levels { parent = index.blocks.record(parent).1; }
        index.names.get(index.blocks.record(parent).0)
    }

    fn new(index: &'a Index, row: usize) -> Self {
        let row = &index.rows[row];
        let (mut parent, mut remaining) = (row.parent, 0);
        while parent != EMPTY {
            remaining += 1;
            parent = index.blocks.record(parent).1;
        }
        let bytes = if remaining == 0 { index.names.get(row.name) } else { Self::part(index, row.parent, remaining - 1) };
        Self { index, parent: row.parent, leaf: row.name, remaining, bytes, at: 0 }
    }
}

impl Iterator for PathBytes<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if let Some(&byte) = self.bytes.get(self.at) {
            self.at += 1;
            return Some(byte);
        }
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        self.bytes = if self.remaining == 0 { self.index.names.get(self.leaf) }
            else { Self::part(self.index, self.parent, self.remaining - 1) };
        self.at = 0;
        Some(b'/')
    }
}

// One digit run: an iterator at its first significant digit and the significant length, one for an all-zero run.
fn digits<'a>(bytes: &mut Peekable<PathBytes<'a>>) -> (Peekable<PathBytes<'a>>, usize) {
    let mut significant = bytes.clone();
    let mut len = 0;
    while bytes.peek().is_some_and(u8::is_ascii_digit) {
        if *bytes.peek().unwrap() != b'0' || len > 0 {
            if len == 0 { significant = bytes.clone(); }
            len += 1;
        } else { significant = bytes.clone(); }
        bytes.next();
    }
    (significant, len.max(1))
}

// sort::finder_cmp, read from two borrowed paths.
fn natural(a: PathBytes<'_>, b: PathBytes<'_>) -> CmpOrdering {
    let (mut a, mut b) = (a.peekable(), b.peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (Some(left), Some(right)) => {
                if left.is_ascii_digit() && right.is_ascii_digit() {
                    let (left, left_len) = digits(&mut a);
                    let (right, right_len) = digits(&mut b);
                    let order = left_len.cmp(&right_len).then_with(|| left.take(left_len).cmp(right.take(right_len)));
                    if order != CmpOrdering::Equal { return order; }
                } else {
                    let order = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
                    if order != CmpOrdering::Equal { return order; }
                    a.next();
                    b.next();
                }
            }
            (None, None) => return CmpOrdering::Equal,
            (None, Some(_)) => return CmpOrdering::Less,
            (Some(_), None) => return CmpOrdering::Greater,
        }
    }
}

// Same parent: the identical raw prefix ends in a slash, so the basenames decide exactly as the full paths would.
fn path_order(index: &Index, a: usize, b: usize) -> CmpOrdering {
    if a == b { return CmpOrdering::Equal; }
    let (left, right) = (&index.rows[a], &index.rows[b]);
    if left.parent == right.parent {
        return super::super::sort::name_order(index.names.get(left.name), index.names.get(right.name));
    }
    let (a, b) = (PathBytes::new(index, a), PathBytes::new(index, b));
    natural(a.clone(), b.clone()).then_with(|| a.cmp(b))
}

// The one rank comparator for answers and rest chunks, so a scrolled window only extends the head's order.
pub(crate) fn rank_cmp(index: &Index, a: &(i32, usize), b: &(i32, usize)) -> CmpOrdering {
    b.0.cmp(&a.0).then_with(|| index.path_len(a.1).cmp(&index.path_len(b.1)))
        .then_with(|| path_order(index, a.1, b.1))
}

// The best `window` hits in rank order, the rest unranked; `cmp` orders score descending first, as `rank_cmp` does, and `key` never orders two rows against `cmp`.
pub(crate) fn select_head<K, F, C>(hits: &mut Vec<(i32, usize)>, window: usize, key: F, cmp: C) -> Vec<(i32, usize)>
where
    K: Ord,
    F: Fn(&(i32, usize)) -> K,
    C: Fn(&(i32, usize), &(i32, usize)) -> CmpOrdering,
{
    #[cfg(test)]
    let cut_started = std::time::Instant::now();
    if window > 0 && window < hits.len() {
        // Rank order is score first, so every row above the window's last score is in the head and every row below it is out.
        let last = hits.select_nth_unstable_by(window - 1, |a, b| b.0.cmp(&a.0)).1.0;
        let above = to_front(&mut hits[..window], |hit| hit.0 > last);
        let tied = window + to_front(&mut hits[window..], |hit| hit.0 == last);
        if tied > window {
            hits[above..tied].select_nth_unstable_by(window - 1 - above, |a, b| key(a).cmp(&key(b)).then_with(|| cmp(a, b)));
        }
    }
    #[cfg(test)]
    CUT.with(|time| time.set(cut_started.elapsed()));
    let rest = hits.split_off(window.min(hits.len()));
    hits.sort_by(&cmp);
    debug_assert!(hits.windows(2).all(|pair| pair[0].0 >= pair[1].0), "select_head needs a score-first cmp");
    rest
}

// Moves the rows `keep` accepts to the front, in place, and returns how many there are.
fn to_front(rows: &mut [(i32, usize)], keep: impl Fn(&(i32, usize)) -> bool) -> usize {
    let mut kept = 0;
    for at in 0..rows.len() {
        if keep(&rows[at]) {
            rows.swap(kept, at);
            kept += 1;
        }
    }
    kept
}

#[cfg(test)]
#[path = "searchorder_tests.rs"]
mod tests;
