//! Compositor presentation timing on CLOCK_MONOTONIC, and GPU readiness labelled as the fork leaves it.
use flea::jsondoc::Json;
use std::{collections::VecDeque, rc::Rc, time::Duration};

const SAMPLES: usize = 1024;
const MONOTONIC: u32 = 1;

pub(crate) type Clock = Rc<dyn Fn() -> Duration>;

extern "C" {
    // The same declaration as pv4's frame tests, so the two never clash.
    fn clock_gettime(clock: i32, now: *mut [i64; 2]) -> i32;
}

/// CLOCK_MONOTONIC now, the clock Wayland presentation feedback announces on Hyprland.
fn monotonic() -> Duration {
    let mut now = [0i64; 2];
    // SAFETY: clock_gettime fills the two-word timespec it is given.
    unsafe { clock_gettime(MONOTONIC as i32, &mut now) };
    Duration::new(now[0].max(0) as u64, now[1].clamp(0, 999_999_999) as u32)
}

#[derive(Default)]
struct Series(VecDeque<f64>);

impl Series {
    fn push(&mut self, ms: f64) {
        if self.0.len() == SAMPLES {
            self.0.pop_front();
        }
        self.0.push_back(ms);
    }

    /// Nearest-rank percentiles over the retained window, or null before the first sample.
    fn json(&self) -> Json {
        use crate::visual::{number, object};
        if self.0.is_empty() {
            return Json::Null;
        }
        let mut sorted: Vec<f64> = self.0.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let rank =
            |p: f64| sorted[((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1];
        object([
            ("count", number(sorted.len())),
            ("p50", number(rank(0.50))),
            ("p95", number(rank(0.95))),
            ("p99", number(rank(0.99))),
            ("max", number(sorted[sorted.len() - 1])),
        ])
    }
}

pub(crate) struct Timing {
    clock: Clock,
    commit_to_present: Series,
    present_interval: Series,
    last_presented: Option<(u64, Duration)>,
    untimed: u64,
    foreign_clock: u64,
    inverted: u64,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            clock: Rc::new(monotonic),
            commit_to_present: Series::default(),
            present_interval: Series::default(),
            last_presented: None,
            untimed: 0,
            foreign_clock: 0,
            inverted: 0,
        }
    }
}

impl Timing {
    #[cfg(test)]
    pub(crate) fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    /// CLOCK_MONOTONIC when the successful-submit receipt reached the app, right after the backend's present.
    pub(crate) fn now(&self) -> Duration {
        (self.clock)()
    }

    /// One presented terminal. Only a CLOCK_MONOTONIC compositor timestamp becomes a sample.
    pub(crate) fn presented(&mut self, id: u64, submitted: Duration, time: gpui::PresentationTime) {
        let (Some(at), Some(clock)) = (time.timestamp, time.clock_id) else {
            self.untimed += 1;
            return;
        };
        if clock != MONOTONIC {
            self.foreign_clock += 1;
            return;
        }
        let Some(latency) = at.checked_sub(submitted) else {
            self.inverted += 1;
            return;
        };
        self.commit_to_present.push(latency.as_secs_f64() * 1000.);
        // Intervals join consecutive commits only, so a discarded or untimed frame between them breaks the pair.
        if let Some((last, then)) = self.last_presented {
            if id == last + 1 {
                if let Some(gap) = at.checked_sub(then) {
                    self.present_interval.push(gap.as_secs_f64() * 1000.);
                }
            }
        }
        self.last_presented = Some((id, at));
    }

    pub(crate) fn json(&self, supported: bool) -> Json {
        use crate::visual::{number, object};
        let series = |series: &Series| if supported { series.json() } else { Json::Null };
        object([
            ("clock", Json::Str("CLOCK_MONOTONIC".into())),
            ("start", Json::Str("submit-observed".into())),
            ("commitToPresentMs", series(&self.commit_to_present)),
            ("presentIntervalMs", series(&self.present_interval)),
            // Idle gaps between consecutive commits count too, so pacing reads need continuous motion.
            ("intervalScope", Json::Str("consecutive-commits".into())),
            ("retained", number(SAMPLES)),
            ("untimed", number(self.untimed)),
            ("foreignClock", number(self.foreign_clock)),
            ("inverted", number(self.inverted)),
        ])
    }
}

/// The pinned fork reports submission and presentation, never GPU completion, so readiness stays unmeasured.
pub(crate) fn gpu_readiness() -> Json {
    crate::visual::object([
        ("status", Json::Str("unavailable".into())),
        ("readyMs", Json::Null),
        ("reason", Json::Str("gpu-completion-not-exposed".into())),
    ])
}
