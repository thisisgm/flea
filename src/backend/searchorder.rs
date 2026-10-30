// Natural path ordering streams borrowed components and defers the raw-byte tie break.
use super::*;
use std::iter::Peekable;

#[path = "searchfamily.rs"]
mod family;
pub(crate) use family::RankOrder;

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

// The best `window` hits in rank order, the rest unranked; `cmp` orders score descending, then `len` ascending, as `rank_cmp` does.
pub(crate) fn select_head<F, C>(hits: &mut Vec<(i32, usize)>, window: usize, len: F, cmp: C) -> Vec<(i32, usize)>
where
    F: Fn(&(i32, usize)) -> usize + Sync,
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
            cut_tied(&mut hits[above..tied], window - above, last, &len, &cmp);
        }
    }
    #[cfg(test)]
    CUT.with(|time| time.set(cut_started.elapsed()));
    let rest = split_rest(hits, window);
    hits.sort_by(&cmp);
    debug_assert!(hits.windows(2).all(|pair| pair[0].0 >= pair[1].0), "select_head needs a score-first cmp");
    rest
}

// Lengths under this are counted one bucket each; longer rows share the last bucket and meet `cmp` there.
const CUT_LENGTHS: usize = 1024;

// The `need` best of rows tied on `score`: one length per row, held in its score slot, and only rows at the cut length meet `cmp`.
fn cut_tied<F, C>(rows: &mut [(i32, usize)], need: usize, score: i32, len: &F, cmp: &C)
where
    F: Fn(&(i32, usize)) -> usize + Sync,
    C: Fn(&(i32, usize), &(i32, usize)) -> CmpOrdering,
{
    let counts = count_lengths(rows, len);
    let (mut cut, mut shorter) = (0, 0);
    while shorter + counts[cut] < need {
        shorter += counts[cut];
        cut += 1;
    }
    let cut = cut as i32;
    let front = to_front(rows, |hit| hit.0 < cut);
    debug_assert_eq!(front, shorter);
    let at_cut = front + to_front(&mut rows[front..], |hit| hit.0 == cut);
    for hit in rows.iter_mut() { hit.0 = score; }
    if at_cut > need {
        rows[front..at_cut].select_nth_unstable_by(need - front - 1, cmp);
    }
}

// Tied rows past this many read their lengths on every core, up to CUT_THREADS of them, as the scorer does.
const PARALLEL_CUT_AT: usize = 32_768;
const CUT_THREADS: usize = 16;

#[cfg(test)]
thread_local! {
    pub(crate) static CUT_THREADS_PINNED: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

fn cut_threads(rows: usize) -> usize {
    if rows < PARALLEL_CUT_AT { return 1; }
    #[cfg(test)]
    if let Some(threads) = CUT_THREADS_PINNED.with(std::cell::Cell::get) { return threads; }
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, CUT_THREADS)
}

// Each row's length goes into its score slot and its count; a length read misses cache, so a large tie splits across threads.
fn count_lengths<F: Fn(&(i32, usize)) -> usize + Sync>(rows: &mut [(i32, usize)], len: &F) -> [usize; CUT_LENGTHS + 1] {
    let count = |rows: &mut [(i32, usize)]| {
        let mut counts = [0usize; CUT_LENGTHS + 1];
        for hit in rows.iter_mut() {
            let length = len(hit).min(CUT_LENGTHS);
            hit.0 = length as i32;
            counts[length] += 1;
        }
        counts
    };
    let threads = cut_threads(rows.len());
    if threads == 1 { return count(rows); }
    let chunk = rows.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let jobs: Vec<_> = rows.chunks_mut(chunk).map(|rows| scope.spawn(move || count(rows))).collect();
        jobs.into_iter().fold([0usize; CUT_LENGTHS + 1], |mut all, job| {
            for (sum, part) in all.iter_mut().zip(job.join().unwrap()) { *sum += part; }
            all
        })
    })
}

// The head moves to the end, so the rest keeps its storage and only the window is copied out.
fn split_rest(hits: &mut Vec<(i32, usize)>, window: usize) -> Vec<(i32, usize)> {
    let window = window.min(hits.len());
    let kept = hits.len() - window;
    if kept <= window { return hits.split_off(window); }
    for at in 0..window { hits.swap(at, kept + at); }
    let head = hits.split_off(kept);
    std::mem::replace(hits, head)
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
