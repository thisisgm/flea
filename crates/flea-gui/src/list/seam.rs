//! The bench seam: with the bench-seam feature the pane prints a benchmark driver's markers and runs its scroll test.
#[cfg(feature = "bench-seam")]
pub use bench::Seam;
#[cfg(not(feature = "bench-seam"))]
pub use off::Seam;

#[cfg(not(feature = "bench-seam"))]
mod off {
    use super::super::{pane::Pane, ListState};

    /// Without the feature every hook is empty, so a packaged build carries none of it.
    pub struct Seam;

    impl Seam {
        pub fn new() -> Self {
            Self
        }
        pub fn viewport(&self) -> Option<gpui::Size<gpui::Pixels>> {
            None
        }
        pub fn window_opened(&self) {}
        pub fn rows(&mut self, _: &Pane) {}
        pub fn selected(&self, _: usize) {}
        pub fn key(&mut self, _: &str, _: &mut Pane) -> bool {
            false
        }
        pub fn frame(&mut self, _: &mut Pane, _: &ListState, _: &gpui::Window) {}
    }
}

#[cfg(feature = "bench-seam")]
mod bench {
    use super::super::{
        pane::{Pane, ROW_HEIGHT},
        ListState,
    };
    use gpui::{px, size, Pixels, Size};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    // The benchmark's sustained scroll: 3000 px a second for five seconds.
    const SPEED: f32 = 3000.;
    const SECONDS: f32 = 5.;

    fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis())
    }

    struct Run {
        started: Instant,
        last: Instant,
        y: f32,
        intervals: Vec<f32>,
        misses: usize,
    }

    pub struct Seam {
        ready: bool,
        armed: bool,
        run: Option<Run>,
        viewport: Option<Size<Pixels>>,
    }

    impl Seam {
        /// FLEA_GUI_BENCH_VIEWPORT, "1024x899", pins the list to the benchmark pane's size at the window's corner.
        pub fn new() -> Self {
            let viewport = std::env::var("FLEA_GUI_BENCH_VIEWPORT").ok().and_then(|value| {
                let (width, height) = value.split_once('x')?;
                Some(size(px(width.parse().ok()?), px(height.parse().ok()?)))
            });
            Self {
                ready: false,
                armed: false,
                run: None,
                viewport,
            }
        }

        pub fn viewport(&self) -> Option<Size<Pixels>> {
            self.viewport
        }

        pub fn window_opened(&self) {
            eprintln!("FEAS_T_WINDOW {}", now_ms());
        }

        pub fn rows(&mut self, pane: &Pane) {
            eprintln!("PANE_ROWS start={} count={}", pane.held.start, pane.held.rows.len());
            if !self.ready {
                self.ready = true;
                eprintln!("FEAS_T_LOADED {} rows={}", now_ms(), pane.held.total);
                eprintln!("PANE_READY {}", now_ms());
            }
        }

        pub fn selected(&self, index: usize) {
            eprintln!("FEAS_SEL {} index={index}", now_ms());
        }

        /// "s" starts the scroll test from the top once row 0 is held.
        pub fn key(&mut self, key: &str, pane: &mut Pane) -> bool {
            if key != "s" || pane.held.total == 0 {
                return false;
            }
            eprintln!("PANE_SCROLL_START");
            pane.scroll_to(0.);
            self.armed = true;
            self.run = None;
            true
        }

        pub fn frame(&mut self, pane: &mut Pane, state: &ListState, window: &gpui::Window) {
            if self.armed && pane.held.row(0).is_some() {
                self.armed = false;
                let now = Instant::now();
                self.run = Some(Run {
                    started: now,
                    last: now,
                    y: 0.,
                    intervals: Vec::new(),
                    misses: 0,
                });
            }
            let Some(run) = self.run.as_mut() else {
                if self.armed {
                    window.request_animation_frame();
                }
                return;
            };
            let now = Instant::now();
            let dt = now.duration_since(run.last).as_secs_f32();
            // The first gap is key-to-render latency, not a frame interval.
            if run.last != run.started {
                run.intervals.push(dt * 1000.);
            }
            run.last = now;
            run.y += SPEED * dt;
            pane.scroll_to(run.y);
            let first = (pane.scroll_y / ROW_HEIGHT).floor() as usize;
            let last = (first + pane.visible_rows()).min(pane.held.total);
            run.misses += (first..last).filter(|&index| pane.held.row(index).is_none()).count();
            if now.duration_since(run.started).as_secs_f32() < SECONDS {
                window.request_animation_frame();
                return;
            }
            let frames: Vec<String> = run.intervals.iter().map(|ms| format!("{ms:.2}")).collect();
            let rows = &state.rows;
            eprintln!(
                "PANE_ROW_CACHE len={} peak={} hits={} misses={}",
                rows.len(),
                rows.peak,
                rows.hits,
                rows.misses
            );
            eprintln!("FEAS_FRAMES {}", frames.join(","));
            eprintln!("PANE_SCROLL_MISSES {}", run.misses);
            self.run = None;
        }
    }
}
