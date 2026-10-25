//! Medians, percentile bootstrap 95% intervals and paired differences, from a seeded generator.

/// xorshift64*: deterministic resampling so a summary is reproducible from its seed.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

pub fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Percentile bootstrap of the median: (median, low, high) at 95%.
pub fn bootstrap(values: &[f64], resamples: usize, rng: &mut Rng) -> (f64, f64, f64) {
    let center = median(values);
    if values.len() < 2 {
        return (center, f64::NAN, f64::NAN);
    }
    let mut medians = Vec::with_capacity(resamples);
    let mut sample = vec![0.0; values.len()];
    for _ in 0..resamples {
        for slot in sample.iter_mut() {
            *slot = values[rng.below(values.len())];
        }
        medians.push(median(&sample));
    }
    medians.sort_by(f64::total_cmp);
    let at = |q: f64| medians[((q * (resamples - 1) as f64).round() as usize).min(resamples - 1)];
    (center, at(0.025), at(0.975))
}

/// Paired difference per round (entrant minus reference), bootstrapped as one series.
pub fn paired(
    entrant: &[(usize, f64)],
    reference: &[(usize, f64)],
    resamples: usize,
    rng: &mut Rng,
) -> Option<(f64, f64, f64, usize)> {
    let diffs: Vec<f64> = entrant
        .iter()
        .filter_map(|(round, value)| {
            reference
                .iter()
                .find(|r| r.0 == *round)
                .map(|r| value - r.1)
        })
        .collect();
    if diffs.is_empty() {
        return None;
    }
    let (m, lo, hi) = bootstrap(&diffs, resamples, rng);
    Some((m, lo, hi, diffs.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_even_and_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    }

    #[test]
    fn bootstrap_brackets_the_median_and_is_seeded() {
        let values: Vec<f64> = (1..=30).map(f64::from).collect();
        let (m, lo, hi) = bootstrap(&values, 2000, &mut Rng::new(7));
        assert!(lo <= m && m <= hi && lo < hi);
        assert_eq!(bootstrap(&values, 2000, &mut Rng::new(7)), (m, lo, hi));
    }

    #[test]
    fn paired_uses_only_shared_rounds() {
        let a = [(0, 10.0), (1, 12.0), (2, 99.0)];
        let b = [(0, 9.0), (1, 10.0)];
        let (m, _, _, n) = paired(&a, &b, 500, &mut Rng::new(3)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(m, 1.5);
    }
}
