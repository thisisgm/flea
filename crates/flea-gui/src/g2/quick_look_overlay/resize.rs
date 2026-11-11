//! Catmull-Rom reduction with image's own weights and summation order, streamed so no source-sized float copy exists.
use std::collections::VecDeque;

/// One output pixel's first source index and its normalised weights, as imageops::sample computes them.
struct Taps {
    first: usize,
    weights: Vec<f32>,
}

fn catmull_rom(x: f32) -> f32 {
    let a = x.abs();
    let k = if a < 1.0 {
        9.0 * a.powi(3) - 15.0 * a.powi(2) + 6.0
    } else if a < 2.0 {
        -3.0 * a.powi(3) + 15.0 * a.powi(2) - 24.0 * a + 12.0
    } else {
        0.0
    };
    k / 6.0
}

fn taps(source: u32, target: u32) -> Vec<Taps> {
    let ratio = source as f32 / target as f32;
    let sratio = if ratio < 1.0 { 1.0 } else { ratio };
    let support = 2.0 * sratio;
    (0..target)
        .map(|out| {
            let centre = (out as f32 + 0.5) * ratio;
            let left = ((centre - support).floor() as i64).clamp(0, i64::from(source) - 1);
            let right = ((centre + support).ceil() as i64).clamp(left + 1, i64::from(source));
            let centre = centre - 0.5;
            let mut weights: Vec<f32> = (left..right)
                .map(|i| catmull_rom((i as f32 - centre) / sratio))
                .collect();
            let sum: f32 = weights.iter().sum();
            for w in &mut weights {
                *w /= sum;
            }
            Taps {
                first: left as usize,
                weights,
            }
        })
        .collect()
}

/// Reduces `N`-channel rows to four-channel `target` rows, lane `c` written to byte `order[c]` of each pixel.
/// One and three channel sources read as opaque grey or RGB; four channels pass through unchanged.
pub(crate) fn reduce<const N: usize>(
    source: &[u8],
    size: (u32, u32),
    target: (u32, u32),
    order: [usize; 4],
) -> Vec<u8> {
    // Bands only pay for their threads on a large source; each output row is summed exactly as one band sums it.
    let large = u64::from(size.0) * u64::from(size.1) >= 4_000_000;
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    reduce_in::<N>(
        source,
        size,
        target,
        order,
        if large { threads.min(BANDS) } else { 1 },
    )
}

const BANDS: usize = 4;

pub(crate) fn reduce_in<const N: usize>(
    source: &[u8],
    (sw, sh): (u32, u32),
    (tw, th): (u32, u32),
    order: [usize; 4],
    bands: usize,
) -> Vec<u8> {
    assert!(N == 1 || N == 3 || N == 4);
    assert!(source.len() >= sw as usize * N * sh as usize && tw <= sw && th <= sh);
    if tw == 0 || th == 0 {
        return Vec::new();
    }
    let rows = taps(sh, th);
    let columns = taps(sw, tw);
    let mut out = vec![0u8; tw as usize * th as usize * 4];
    let line = tw as usize * 4;
    let per = (th as usize).div_ceil(bands.max(1));
    std::thread::scope(|scope| {
        let mut chunks = out.chunks_mut(per * line).enumerate();
        let first = chunks.next();
        for (index, chunk) in chunks {
            let (rows, columns) = (&rows, &columns);
            scope.spawn(move || {
                band::<N>(
                    source,
                    sw as usize,
                    rows,
                    columns,
                    index * per,
                    chunk,
                    order,
                )
            });
        }
        if let Some((_, chunk)) = first {
            band::<N>(source, sw as usize, &rows, &columns, 0, chunk, order);
        }
    });
    out
}

/// Output rows from `top`, streamed over only the source rows their taps read.
fn band<const N: usize>(
    source: &[u8],
    width: usize,
    rows: &[Taps],
    columns: &[Taps],
    top: usize,
    out: &mut [u8],
    order: [usize; 4],
) {
    let stride = width * N;
    let line = columns.len() * 4;
    let end = top + out.len() / line;
    let last = rows[end - 1].first + rows[end - 1].weights.len();
    let mut lanes = vec![0f32; width * 4];
    let mut open: VecDeque<(usize, Vec<f32>)> = VecDeque::new();
    let mut spare: Vec<Vec<f32>> = Vec::new();
    let mut next = top;
    for r in rows[top].first..last {
        while next < end && rows[next].first <= r {
            let mut sums = spare.pop().unwrap_or_else(|| vec![0f32; width * 4]);
            sums.fill(0.);
            open.push_back((next, sums));
            next += 1;
        }
        let row = &source[r * stride..(r + 1) * stride];
        for (lane, pixel) in lanes
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(row.as_chunks::<N>().0)
        {
            let (grey, last) = (N == 1, N - 1);
            lane[0] = f32::from(pixel[0]);
            lane[1] = f32::from(pixel[if grey { 0 } else { 1 }]);
            lane[2] = f32::from(pixel[if grey { 0 } else { 2 }]);
            lane[3] = if N == 4 { f32::from(pixel[last]) } else { 255. };
        }
        for (y, sums) in open.iter_mut() {
            let taps = &rows[*y];
            if let Some(&w) = taps.weights.get(r - taps.first) {
                for (sum, lane) in sums.iter_mut().zip(&lanes) {
                    *sum += lane * w;
                }
            }
        }
        while open
            .front()
            .is_some_and(|(y, _)| rows[*y].first + rows[*y].weights.len() <= r + 1)
        {
            let (y, sums) = open.pop_front().unwrap();
            let line = &mut out[(y - top) * line..(y - top + 1) * line];
            for (pixel, taps) in line.as_chunks_mut::<4>().0.iter_mut().zip(columns) {
                let mut t = [0f32; 4];
                for (i, w) in taps.weights.iter().enumerate() {
                    let at = (taps.first + i) * 4;
                    for c in 0..4 {
                        t[c] += sums[at + c] * w;
                    }
                }
                for c in 0..4 {
                    pixel[order[c]] = t[c].clamp(0., 255.).round() as u8;
                }
            }
            spare.push(sums);
        }
    }
}

#[cfg(test)]
#[path = "resize_tests.rs"]
mod tests;
