//! Separable Catmull-Rom weights for bounded scanline decoding.
#[derive(Clone, Copy)]
pub(super) struct Tap {
    pub at: usize,
    pub weight: f32,
}

// Tap lists are cached per source pixel up to this width (a few MB); a wider source recomputes its taps per row.
const CACHED_SOURCE: u32 = 32768;

pub(super) struct Axis {
    source: u32,
    target: u32,
    sums: Vec<f32>,
    cached: Option<Vec<Vec<Tap>>>,
}

fn cubic(x: f32) -> f32 {
    let x = x.abs();
    if x < 1. {
        (1.5 * x - 2.5) * x * x + 1.
    } else if x < 2. {
        ((-0.5 * x + 2.5) * x - 4.) * x + 2.
    } else {
        0.
    }
}

impl Axis {
    pub fn new(source: u32, target: u32) -> Self {
        let ratio = source as f32 / target as f32;
        let support = ratio.max(1.) * 2.;
        let sums = (0..target)
            .map(|at| {
                if target >= source {
                    return 1.;
                }
                let centre = (at as f32 + 0.5) * ratio;
                let first = (centre - support).floor().max(0.) as u32;
                let end = ((centre + support).ceil() as u32).min(source);
                (first..end)
                    .map(|x| cubic((x as f32 + 0.5 - centre) / ratio))
                    .sum()
            })
            .collect();
        let mut axis = Self {
            source,
            target,
            sums,
            cached: None,
        };
        if source <= CACHED_SOURCE {
            axis.cached = Some((0..source).map(|at| axis.compute(at)).collect());
        }
        axis
    }

    fn compute(&self, source: u32) -> Vec<Tap> {
        let ratio = self.source as f32 / self.target as f32;
        if self.target >= self.source {
            let first = (source as f64 * self.target as f64 / self.source as f64 - 0.5)
                .ceil()
                .max(0.) as u32;
            let end = (((source + 1) as f64 * self.target as f64 / self.source as f64 - 0.5)
                .ceil()
                .max(0.) as u32)
                .min(self.target);
            return (first..end)
                .map(|at| Tap {
                    at: at as usize,
                    weight: 1.,
                })
                .collect();
        }
        let centre = (source as f32 + 0.5) / ratio - 0.5;
        let first = (centre - 2.).ceil().max(0.) as u32;
        let end = ((centre + 2.).floor() as u32 + 1).min(self.target);
        (first..end)
            .filter_map(|at| {
                let weight = cubic((source as f32 + 0.5 - (at as f32 + 0.5) * ratio) / ratio)
                    / self.sums[at as usize];
                (weight != 0.).then_some(Tap {
                    at: at as usize,
                    weight,
                })
            })
            .collect()
    }

    pub fn each(&self, source: u32, mut apply: impl FnMut(Tap)) {
        match &self.cached {
            Some(cached) => cached[source as usize].iter().copied().for_each(apply),
            None => self.compute(source).into_iter().for_each(&mut apply),
        }
    }
}
