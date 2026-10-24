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

    impl Default for Seam {
        fn default() -> Self {
            Self::new()
        }
    }

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
        pub fn mark(&self) -> crate::feedback::spinner::Arm {
            crate::feedback::spinner::Arm::Crawl
        }
    }
}

#[cfg(feature = "bench-seam")]
mod bench {
    use super::super::{pane::Pane, ListState};
    use crate::feedback::spinner::Arm;
    use gpui::{px, size, Pixels, Size};
    use std::{
        ffi::OsString,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

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
        mark: Arm,
    }

    /// FLEA_GUI_BENCH_VIEWPORT, "1024x899", pins the list to the benchmark pane's size at the window's corner.
    fn viewport(value: &str) -> Option<Size<Pixels>> {
        let (width, height) = value.split_once('x')?;
        let side = |text: &str| {
            text.parse::<f32>()
                .ok()
                .filter(|side| side.is_finite() && *side > 0.)
        };
        Some(size(px(side(width)?), px(side(height)?)))
    }

    /// FLEA_GUI_BENCH_MARK "none" leaves the loading mark undrawn for the floor arm; a number pins its mode time in seconds.
    fn mark(value: &str) -> Option<Arm> {
        match value {
            "none" => Some(Arm::Hidden),
            offset => offset
                .parse()
                .ok()
                .filter(|offset: &f32| offset.is_finite())
                .map(Arm::Pinned),
        }
    }

    /// An unset variable is None; a set one that does not parse is the message the run ends with.
    fn check<T>(
        name: &str,
        value: Option<OsString>,
        parse: fn(&str) -> Option<T>,
    ) -> Result<Option<T>, String> {
        let Some(value) = value else { return Ok(None) };
        match value.to_str().and_then(parse) {
            Some(parsed) => Ok(Some(parsed)),
            None => Err(format!(
                "flea-gui: {name}={} does not parse",
                value.to_string_lossy()
            )),
        }
    }

    // A benchmark run with a mistyped variable stops before it measures anything, rather than measure the default arm.
    fn read<T>(name: &str, parse: fn(&str) -> Option<T>) -> Option<T> {
        check(name, std::env::var_os(name), parse).unwrap_or_else(|message| {
            eprintln!("{message}");
            std::process::exit(2)
        })
    }

    impl Default for Seam {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Seam {
        pub fn new() -> Self {
            let viewport = read("FLEA_GUI_BENCH_VIEWPORT", viewport);
            let mark = read("FLEA_GUI_BENCH_MARK", mark).unwrap_or(Arm::Crawl);
            Self {
                ready: false,
                armed: false,
                run: None,
                viewport,
                mark,
            }
        }

        pub fn mark(&self) -> Arm {
            self.mark
        }

        pub fn viewport(&self) -> Option<Size<Pixels>> {
            self.viewport
        }

        pub fn window_opened(&self) {
            eprintln!("FEAS_T_WINDOW {}", now_ms());
        }

        pub fn rows(&mut self, pane: &Pane) {
            eprintln!(
                "PANE_ROWS start={} count={}",
                pane.held.start,
                pane.held.rows.len()
            );
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
            let first = (pane.scroll_y / pane.row_height()).floor() as usize;
            let last = (first + pane.visible_rows()).min(pane.held.total);
            run.misses += (first..last)
                .filter(|&index| pane.held.row(index).is_none())
                .count();
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

    #[cfg(test)]
    mod tests {
        use super::{check, mark, viewport, Arm};
        use gpui::{px, size};

        #[test]
        fn a_bench_variable_parses_or_names_itself_and_its_value() {
            assert_eq!(viewport("1024x899"), Some(size(px(1024.), px(899.))));
            for bad in ["1024", "1024x", "wide x 899", "NaNx899", "0x899", "-5x899"] {
                assert_eq!(viewport(bad), None, "{bad}");
            }
            assert_eq!(mark("none"), Some(Arm::Hidden));
            assert_eq!(mark("-27"), Some(Arm::Pinned(-27.)));
            for bad in ["", "crawl", "inf", "NaN", "-27px"] {
                assert_eq!(mark(bad), None, "{bad}");
            }
            assert_eq!(check("FLEA_GUI_BENCH_MARK", None, mark), Ok(None));
            assert_eq!(
                check("FLEA_GUI_BENCH_MARK", Some("crawl".into()), mark),
                Err("flea-gui: FLEA_GUI_BENCH_MARK=crawl does not parse".into())
            );
            assert_eq!(
                check("FLEA_GUI_BENCH_VIEWPORT", Some("1024".into()), viewport),
                Err("flea-gui: FLEA_GUI_BENCH_VIEWPORT=1024 does not parse".into())
            );
        }
    }
}
