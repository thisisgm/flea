//! Swap measurements retain lifecycle observations without driving the preview.
use super::SwapState;
use gpui::BackgroundExecutor;
use std::time::Instant;

pub(crate) struct Clock {
    pub executor: BackgroundExecutor,
    pub origin: Instant,
}

#[derive(Default)]
pub(crate) struct Swap {
    pub fell_back: bool,
    pub bursts: u64,
    pub shown_ms: u64,
    pub last_ms: u64,
    pub last_end: &'static str,
    since: Option<u64>,
    key: String,
}

impl Swap {
    pub fn hold(&mut self, now: u64, holding: bool, key: &str) {
        if !holding {
            self.since = Some(now);
        } else if !key.is_empty() && key != self.key {
            self.bursts += 1;
        }
        if !key.is_empty() {
            self.key = key.into();
        }
        self.fell_back = false;
    }
}

pub(crate) fn release(state: &mut SwapState, now: u64, end: &'static str, fallback: bool) {
    let observed = &mut state.observation;
    if let Some(start) = observed.since.take() {
        observed.last_ms = now.saturating_sub(start);
        observed.last_end = end;
    }
    if !fallback && (state.holding || observed.fell_back) {
        if let Some(start) = state.loading_at {
            observed.shown_ms = now.saturating_sub(start);
        }
    }
    observed.fell_back = fallback;
}

pub(crate) fn json(state: &SwapState, frames: Option<&super::frame_feedback::Frames>) -> String {
    let observed = &state.observation;
    let (held, mid, loading) = frames.map_or((0, 0, 0), |frames| {
        (frames.held, frames.blank, frames.loading)
    });
    // GPUI retains the prior frame directly, so it has no screenshot capture phase.
    format!(concat!("{{\"holding\":{},\"capturing\":false,\"fellBack\":{},\"holds\":{},\"fallbacks\":{},",
        "\"bursts\":{},\"heldFrames\":{},\"midFrames\":{},\"loadingFrames\":{},\"last\":{{\"ms\":{},\"end\":\"{}\"}},\"shownMs\":{}}}"),
        state.holding, observed.fell_back, state.holds, state.fallbacks, observed.bursts,
        held, mid, loading, observed.last_ms, observed.last_end, observed.shown_ms)
}
