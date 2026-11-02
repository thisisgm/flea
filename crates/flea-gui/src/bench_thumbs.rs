//! B7's opt-in output receipt: decoded pixels painted in an exactly presented frame.
use crate::preview_trace::{now, CLOCK_MONOTONIC};
use gpui::{App, Bounds, FrameFeedbackEvent, FrameOutcome, Pixels, Subscription, Window};
use std::{cell::RefCell, collections::VecDeque, sync::OnceLock, time::Duration};

const KEPT: usize = 512;
/// Quick Look's marked raster edge, independent of monitor dimensions.
pub(crate) const PREVIEW_EDGE: u32 = 800;
const MAX_DELAY_MS: u64 = 5000;

pub(crate) fn enabled() -> bool {
    token().is_some()
}

fn token() -> Option<&'static str> {
    static TOKEN: OnceLock<Option<String>> = OnceLock::new();
    TOKEN
        .get_or_init(|| {
            std::env::var("FLEA_B7_BENCH")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .as_deref()
}

fn delay_ms() -> Option<u64> {
    static DELAY: OnceLock<Option<u64>> = OnceLock::new();
    *DELAY.get_or_init(|| {
        std::env::var("FLEA_B7_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
    })
}

#[derive(Clone)]
struct Job {
    kind: &'static str,
    path: String,
    start: Duration,
    worker_ms: Option<f64>,
    done: bool,
}

#[derive(Clone)]
struct Paint {
    job: usize,
    width: u32,
    height: u32,
    bounds: [f32; 4],
}

#[derive(Default)]
struct State {
    jobs: Vec<Job>,
    draws: VecDeque<(u64, Vec<Paint>)>,
    pending: VecDeque<(u64, Vec<Paint>)>,
    observer: Option<Subscription>,
    delayed: bool,
    delay_until: Option<Duration>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub(crate) fn begin(kind: &'static str, path: &str) {
    if !enabled() {
        return;
    }
    let Some(start) = now() else { return };
    STATE.with_borrow_mut(|state| {
        if state.jobs.len() < KEPT && !state.jobs.iter().any(|j| j.kind == kind && j.path == path) {
            state.jobs.push(Job {
                kind,
                path: path.into(),
                start,
                worker_ms: None,
                done: false,
            });
        }
    });
}

pub(crate) fn worker(path: &str, ms: f64) {
    if !enabled() || !ms.is_finite() || ms <= 0. {
        return;
    }
    STATE.with_borrow_mut(|state| {
        if let Some(job) = state
            .jobs
            .iter_mut()
            .find(|j| j.kind == "thumbnail" && j.path == path && !j.done)
        {
            job.worker_ms = Some(ms);
        }
    });
}

pub(crate) fn observe(window: &mut Window, cx: &mut App) {
    if !enabled() || STATE.with_borrow(|state| state.observer.is_some()) {
        return;
    }
    let observer = window.observe_frame_feedback(cx, |event, _, _| {
        STATE.with_borrow_mut(|state| state.receive(event));
    });
    STATE.with_borrow_mut(|state| state.observer = observer);
}

pub(crate) fn painted(
    kind: &str,
    path: &str,
    pixels: (u32, u32),
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    if !enabled() || !window.is_visible() || pixels.0 == 0 || pixels.1 == 0 {
        return;
    }
    let Some(draw) = window.current_draw_id() else {
        return;
    };
    let rect = [
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    ];
    let size = window.viewport_size();
    let mask = window.content_mask().bounds;
    if rect[0] < 0.
        || rect[1] < 0.
        || rect[2] <= 0.
        || rect[3] <= 0.
        || rect[0] + rect[2] > f32::from(size.width)
        || rect[1] + rect[3] > f32::from(size.height)
        || bounds.origin.x < mask.origin.x
        || bounds.origin.y < mask.origin.y
        || bounds.origin.x + bounds.size.width > mask.origin.x + mask.size.width
        || bounds.origin.y + bounds.size.height > mask.origin.y + mask.size.height
    {
        return;
    }
    STATE.with_borrow_mut(|state| {
        let Some(job) = state
            .jobs
            .iter()
            .position(|j| j.kind == kind && j.path == path && !j.done)
        else {
            return;
        };
        if state.withheld(now(), delay_ms()) {
            window.request_animation_frame();
            return;
        }
        if state.draws.back().is_none_or(|(id, _)| *id != draw) {
            if state.draws.len() == KEPT {
                state.draws.pop_front();
            }
            state.draws.push_back((draw, Vec::new()));
        }
        if let Some((_, paints)) = state.draws.back_mut() {
            if !paints.iter().any(|p| p.job == job) {
                paints.push(Paint {
                    job,
                    width: pixels.0,
                    height: pixels.1,
                    bounds: rect,
                });
            }
        }
    });
}

impl State {
    /// Negative control: from the first matched paint, no job stages a receipt until the delay passes.
    fn withheld(&mut self, at: Option<Duration>, delay_ms: Option<u64>) -> bool {
        if !self.delayed {
            self.delayed = true;
            if let Some(ms) = delay_ms.filter(|ms| *ms <= MAX_DELAY_MS) {
                self.delay_until = at.and_then(|at| at.checked_add(Duration::from_millis(ms)));
            }
        }
        if self
            .delay_until
            .is_some_and(|until| at.is_some_and(|at| at < until))
        {
            return true;
        }
        self.delay_until = None;
        false
    }

    fn receive(&mut self, event: FrameFeedbackEvent) {
        match event {
            FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                if let Some((_, paints)) = self.draws.iter().find(|(id, _)| *id == draw_id) {
                    self.pending.push_back((frame_id, paints.clone()));
                    while self.pending.len() > KEPT {
                        self.pending.pop_front();
                    }
                }
            }
            FrameFeedbackEvent::Terminal(terminal) => {
                let paints = self
                    .pending
                    .iter()
                    .position(|(id, _)| *id == terminal.frame_id)
                    .and_then(|at| self.pending.remove(at));
                let Some((_, paints)) = paints else { return };
                let FrameOutcome::Presented(gpui::PresentationTime {
                    timestamp: Some(end),
                    clock_id: Some(clock),
                }) = terminal.outcome
                else {
                    return;
                };
                if i64::from(clock) != i64::from(CLOCK_MONOTONIC) {
                    return;
                }
                for paint in paints {
                    let job = &mut self.jobs[paint.job];
                    if job.done || end <= job.start {
                        continue;
                    }
                    job.done = true;
                    let worker = job.worker_ms.map_or("null".into(), |ms| ms.to_string());
                    eprintln!("FLEA-B7 {{\"token\":\"{}\",\"kind\":\"{}\",\"path\":\"{}\",\"start_ns\":{},\"end_ns\":{},\"worker_ms\":{},\"width\":{},\"height\":{},\"bounds\":{:?},\"frame\":{},\"clock\":{CLOCK_MONOTONIC}}}", flea::json::escape(token().unwrap_or("")), job.kind, flea::json::escape(&job.path), job.start.as_nanos(), end.as_nanos(), worker, paint.width, paint.height, paint.bounds, terminal.frame_id);
                }
            }
        }
    }
}

/// B7 compares an actual `PREVIEW_EDGE` raster, independently of monitor dimensions.
pub(crate) fn preview_size(source: (f64, f64), default: (u32, u32)) -> (u32, u32) {
    let edge = f64::from(PREVIEW_EDGE);
    if !enabled() || source.0 < edge && source.1 < edge {
        return default;
    }
    let ratio = edge / source.0.max(source.1);
    (
        (source.0 * ratio).round().max(1.) as u32,
        (source.1 * ratio).round().max(1.) as u32,
    )
}

#[cfg(test)]
#[path = "bench_thumbs_tests.rs"]
mod tests;
