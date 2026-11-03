//! Installed-candidate diagnostics. Disabled launches install no observers, state or frame driver.
use crate::state::Shell;
use flea::json::escape;
use gpui::{App, Entity, FrameFeedbackEvent, FrameOutcome, Window};
use std::{cell::RefCell, collections::VecDeque, ffi::CStr, rc::Rc, time::Duration};

const SCROLL: Duration = Duration::from_secs(5);
const SCROLL_PIXELS_PER_SECOND: f32 = 3000.;
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);
const MONOTONIC: u32 = 1;
const LIMIT: Duration = Duration::from_secs(20);
const RETAINED: usize = 512;
const SAMPLES: usize = 5000;

extern "C" {
    fn getenv(name: *const std::ffi::c_char) -> *const std::ffi::c_char;
}

fn enabled() -> bool {
    // SAFETY: the name is NUL-terminated; read the process-owned value without retaining it or allocating.
    let value = unsafe { getenv(c"FLEA_GPU_REPORT".as_ptr()) };
    !value.is_null() && unsafe { CStr::from_ptr(value) }.to_bytes() == b"1"
}

struct Commit {
    id: u64,
    scroll: f32,
    scrolling: bool,
}

#[derive(Default)]
struct Frames {
    first_ms: Option<f64>,
    scrolling: bool,
    incomplete: bool,
    end_scroll: Option<f32>,
    end_presented: bool,
    pending: VecDeque<Commit>,
    samples: Vec<(Duration, f32)>,
}

impl Frames {
    fn submitted(&mut self, id: u64, scroll: Option<f32>) {
        let Some(scroll) = scroll else { return };
        if self.pending.len() == RETAINED {
            self.pending.pop_front();
            self.incomplete = true;
        }
        self.pending.push_back(Commit {
            id,
            scroll,
            scrolling: self.scrolling,
        });
    }

    fn terminal(&mut self, terminal: gpui::FrameTerminal, launched_ns: Option<u128>) {
        let Some(index) = self
            .pending
            .iter()
            .position(|frame| frame.id == terminal.frame_id)
        else {
            return;
        };
        let Some(frame) = self.pending.remove(index) else {
            return;
        };
        let FrameOutcome::Presented(time) = terminal.outcome else {
            return;
        };
        let (Some(at), Some(MONOTONIC)) = (time.timestamp, time.clock_id) else {
            self.incomplete = true;
            return;
        };
        if launched_ns.is_some_and(|start| at.as_nanos() < start) {
            self.incomplete = true;
            return;
        }
        if let Some(elapsed) = launched_ns.and_then(|start| at.as_nanos().checked_sub(start)) {
            let ms = elapsed as f64 / 1_000_000.;
            self.first_ms = Some(self.first_ms.map_or(ms, |first| first.min(ms)));
        }
        let final_presentation = self.end_scroll == Some(frame.scroll) && !self.end_presented;
        if frame.scrolling || final_presentation {
            if self.samples.len() < SAMPLES {
                self.samples.push((at, frame.scroll));
            } else {
                self.incomplete = true;
            }
        }
        if final_presentation {
            self.end_presented = true;
            self.scrolling = false;
        }
    }

    fn json(&mut self, complete: bool, ready: bool) -> String {
        let complete = complete
            && self.end_presented
            && !self.incomplete
            && !self.pending.iter().any(|frame| frame.scrolling);
        self.samples.sort_unstable_by_key(|(at, _)| *at);
        self.samples.dedup_by_key(|(at, _)| *at);
        let distance = self
            .samples
            .first()
            .zip(self.samples.last())
            .map_or(0., |(first, last)| last.1 - first.1);
        let mut intervals = Vec::new();
        if let Some(mut previous) = self.samples.first().copied().filter(|_| distance > 0.) {
            for sample in self.samples.iter().copied().skip(1) {
                if sample.1 == previous.1 {
                    continue;
                }
                intervals.push(format!(
                    "{:.3}",
                    (sample.0 - previous.0).as_secs_f64() * 1000.
                ));
                previous = sample;
            }
            // Preserve the measured trailing stall without letting stationary repaints split it.
            if let Some(last) = self.samples.last().filter(|last| last.0 > previous.0) {
                intervals.push(format!(
                    "{:.3}",
                    (last.0 - previous.0).as_secs_f64() * 1000.
                ));
            }
        }
        let first = self.first_ms.map_or("null".into(), |ms| format!("{ms:.3}"));
        format!("{{\"first_ms\":{first},\"complete\":{complete},\"ready\":{ready},\"scroll_px\":{distance:.3},\"intervals_ms\":[{}]}}", intervals.join(","))
    }
}

pub(super) fn start(shell: &Entity<Shell>, window: &mut Window, cx: &mut App) {
    start_if(enabled(), shell, window, cx);
}

fn start_if(active: bool, shell: &Entity<Shell>, window: &mut Window, cx: &mut App) {
    if !active {
        return;
    }
    #[cfg(test)]
    tests::work(0);
    adapter(window);
    let launched_ns = std::env::var("FLEA_GPU_REPORT_START_NS")
        .ok()
        .and_then(|value| value.parse().ok());
    let frames = Rc::new(RefCell::new(Frames::default()));
    let observed = frames.clone();
    let weak = shell.downgrade();
    if let Some(subscription) = window.observe_frame_feedback(cx, move |event, _, cx| {
        #[cfg(test)]
        tests::work(1);
        match event {
            FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                let scene = weak
                    .read_with(cx, |shell, _| {
                        shell.presentation.gpu_report_scene(draw_id, frame_id)
                    })
                    .ok()
                    .flatten();
                observed.borrow_mut().submitted(frame_id, scene);
            }
            FrameFeedbackEvent::Terminal(terminal) => {
                observed.borrow_mut().terminal(terminal, launched_ns)
            }
        }
    }) {
        subscription.detach();
    }
    tick(
        shell.downgrade(),
        frames,
        None,
        None,
        cx.background_executor().now(),
        window,
    );
}

fn adapter(window: &Window) -> bool {
    let Some(specs) = window.gpu_specs() else {
        return false;
    };
    eprintln!(
            "FLEA_GPU_ADAPTER {{\"name\":\"{}\",\"driver\":\"{}\",\"version\":\"{}\",\"cpu\":{},\"build\":\"{} {}\"}}",
            escape(&specs.device_name), escape(&specs.driver_name), escape(&specs.driver_info),
            specs.is_software_emulated, env!("CARGO_PKG_VERSION"),
            if cfg!(debug_assertions) { "debug" } else { "release" }
    );
    true
}

fn tick(
    shell: gpui::WeakEntity<Shell>,
    frames: Rc<RefCell<Frames>>,
    mut scrolling: Option<std::time::Instant>,
    mut stopped: Option<std::time::Instant>,
    started: std::time::Instant,
    window: &Window,
) {
    window.on_next_frame(move |window, cx| {
        #[cfg(test)]
        tests::work(2);
        let now = cx.background_executor().now();
        let mut ready = false;
        if shell
            .update(cx, |shell, cx| {
                if let Some(host) = &mut shell.panes[0] {
                    ready = host.pane.held.total == 10_000 && !host.pane.busy();
                    if ready && scrolling.is_none() {
                        scrolling = Some(now);
                        frames.borrow_mut().scrolling = true;
                    }
                    if let Some(since) = scrolling.filter(|_| stopped.is_none()) {
                        host.pane.scroll_to(
                            (now - since).as_secs_f32().min(SCROLL.as_secs_f32())
                                * SCROLL_PIXELS_PER_SECOND,
                        );
                        if now - since >= SCROLL {
                            stopped = Some(now);
                            frames.borrow_mut().end_scroll = Some(host.pane.scroll_y);
                        }
                        cx.notify();
                    }
                }
            })
            .is_err()
        {
            return;
        }
        let drained = !frames.borrow().pending.iter().any(|frame| frame.scrolling);
        let done = stopped.is_some_and(|since| {
            (drained && frames.borrow().end_presented) || now - since >= DRAIN_TIMEOUT
        });
        if done || now - started >= LIMIT {
            let actual_adapter = adapter(window);
            eprintln!(
                "FLEA_GPU_FRAMES {}",
                frames
                    .borrow_mut()
                    .json(stopped.is_some() && actual_adapter, ready)
            );
            cx.quit();
        } else {
            tick(shell, frames, scrolling, stopped, started, window);
        }
    });
}

#[cfg(test)]
mod tests;
