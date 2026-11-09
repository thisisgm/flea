//! Passive lifecycle and paint observations, separate from the decisions that draw the card.
use gpui::{Bounds, Pixels};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(crate) struct Swap {
    pub holding: bool,
    pub fell_back: bool,
    pub holds: u64,
    pub fallbacks: u64,
    pub bursts: u64,
    pub held_frames: u64,
    pub mid_frames: u64,
    pub loading_frames: u64,
    pub shown_ms: u64,
    pub last_ms: u64,
    pub last_end: &'static str,
    since: u64,
    loading_since: Option<u64>,
}

impl Swap {
    pub fn begin(&mut self, now: u64) {
        if self.holding {
            self.bursts += 1;
        } else {
            self.holds += 1;
            self.since = now;
        }
        self.holding = true;
        self.fell_back = false;
    }

    pub fn release(&mut self, now: u64, end: &'static str, fallback: bool) {
        if self.holding {
            self.last_ms = now.saturating_sub(self.since);
            self.last_end = end;
        }
        self.holding = false;
        self.fell_back = fallback;
        self.fallbacks += u64::from(fallback);
        if fallback {
            self.loading_since.get_or_insert(now);
        } else if let Some(start) = self.loading_since.take() {
            self.shown_ms = now.saturating_sub(start);
        }
    }

    pub fn frame(&mut self, visible: bool, ready: bool, holding: bool, loading: bool) {
        self.held_frames += u64::from(visible && holding);
        self.mid_frames += u64::from(visible && !holding && !ready && !loading);
        self.loading_frames += u64::from(visible && loading);
    }

    pub fn json(&self) -> String {
        format!(concat!("{{\"holding\":{},\"capturing\":false,\"fellBack\":{},\"holds\":{},\"fallbacks\":{},",
            "\"bursts\":{},\"heldFrames\":{},\"midFrames\":{},\"loadingFrames\":{},\"last\":{{\"ms\":{},\"end\":\"{}\"}},\"shownMs\":{}}}"),
            self.holding, self.fell_back, self.holds, self.fallbacks, self.bursts,
            self.held_frames, self.mid_frames, self.loading_frames, self.last_ms, self.last_end, self.shown_ms)
    }
}

/// One painted box per PDF strip control, the only strip pdfState reads.
pub(crate) const STRIP_SLOTS: usize = flea::ui_logic_doc_preview::viewer::CONTROLS.len();

#[derive(Default)]
pub(crate) struct Observations {
    pub close: Option<Bounds<Pixels>>,
    pub close_hovered: bool,
    pub surface: Option<Bounds<Pixels>>,
    pub picture: Option<Bounds<Pixels>>,
    pub strip: [Option<Bounds<Pixels>>; STRIP_SLOTS],
    pub strip_hovered: [bool; STRIP_SLOTS],
    pub swap: Swap,
    pub revealed_early: bool,
    pub landed: bool,
    pub empty_frames: u64,
    pub reveal_caps: u64,
    pub cap_start: Option<u64>,
    pub shown_seq: u64,
    sequence: u64,
}

pub(crate) type Shared = Rc<RefCell<Observations>>;

impl Observations {
    pub fn opened(&mut self, now: u64, ready: bool) {
        if let Some(start) = self.swap.loading_since.take() {
            self.swap.shown_ms = now.saturating_sub(start);
        }
        self.close = None;
        self.close_hovered = false;
        self.surface = None;
        self.picture = None;
        self.strip = [None; STRIP_SLOTS];
        self.strip_hovered = [false; STRIP_SLOTS];
        self.revealed_early = false;
        self.landed = ready;
        self.sequence += 1;
        self.shown_seq = self.sequence;
        self.cap_start = None;
        if !ready {
            self.swap.begin(now);
            self.sequence += 1;
            self.cap_start = Some(self.sequence);
        } else {
            self.swap.release(now, "landed", false);
        }
    }

    pub fn revealed(&mut self, now: u64, ready: bool, cap: bool) {
        self.revealed_early |= !ready;
        self.landed |= ready;
        self.reveal_caps += u64::from(cap);
        self.swap
            .release(now, if ready { "landed" } else { "expired" }, !ready);
    }

    pub fn frame(&mut self, visible: bool, ready: bool, holding: bool, loading: bool) {
        self.empty_frames += u64::from(visible && !ready && !loading);
        self.swap.frame(visible, ready, holding, loading);
    }

    pub fn closed(&mut self, now: u64) {
        self.close = None;
        self.close_hovered = false;
        self.swap.release(now, "cancelled", false);
        self.revealed_early = false;
        self.landed = false;
        self.cap_start = None;
    }

    pub fn open_json(&self, revealed: bool) -> String {
        let cap = self
            .cap_start
            .map_or_else(|| "null".into(), |seq| format!("{{\"seq\":{seq}}}"));
        format!("{{\"revealed\":{revealed},\"revealedEarly\":{},\"landed\":{},\"lookEmptyFrames\":{},\"revealCaps\":{},\"revealCapStart\":{cap},\"shownSeq\":{}}}",
            self.revealed_early, self.landed, self.empty_frames, self.reveal_caps, self.shown_seq)
    }
}

pub(super) fn close_slot(
    slot: gpui::Stateful<gpui::Div>,
    observations: &Shared,
) -> gpui::Stateful<gpui::Div> {
    use gpui::{canvas, prelude::*};
    let bounds = observations.clone();
    let hover = observations.clone();
    slot.child(
        canvas(
            |bounds, _, _| bounds,
            move |_, rect, _, _| {
                bounds.borrow_mut().close = Some(rect);
            },
        )
        .absolute()
        .size_full(),
    )
    .on_hover(move |over, _, _| hover.borrow_mut().close_hovered = *over)
}

/// Bounds from the actual laid-out element, for passive IPC readers.
pub(super) fn bounds(
    observations: &Shared,
    record: impl Fn(&mut Observations, Bounds<Pixels>) + 'static,
) -> gpui::AnyElement {
    use gpui::{canvas, prelude::*};
    let observations = observations.clone();
    canvas(
        |bounds, _, _| bounds,
        move |_, bounds, _, _| record(&mut observations.borrow_mut(), bounds),
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

impl super::QuickLook {
    /// Clears the painted boxes before a render, so a box reads only while this frame draws its element.
    pub(crate) fn forget_painted_bounds(&self) {
        let mut observed = self.observations.borrow_mut();
        observed.surface = None;
        observed.picture = None;
        observed.strip = [None; STRIP_SLOTS];
    }

    pub(crate) fn observation_time(&mut self, cx: &gpui::App) -> u64 {
        let now = cx.background_executor().now();
        let origin = self.observation_origin.get_or_insert(now);
        now.saturating_duration_since(*origin).as_millis() as u64
    }

    pub(crate) fn content_ready(&self, cx: &gpui::App) -> bool {
        self.subject.as_ref().is_some_and(|subject| {
            if let Some(doc) = subject.body.doc() {
                return doc.ready(cx);
            }
            match &subject.body {
                super::Body::Text { count, .. } => !count.is_empty(),
                super::Body::Image { .. } | super::Body::Link { .. } | super::Body::Raw { .. } => {
                    self.svg
                        .as_ref()
                        .is_some_and(|picture| picture.image.is_some() || picture.refusal.is_some())
                }
                _ => self.revealed,
            }
        })
    }

    /// An interim is drawable, but only the original raster (or its refusal) lands the picture.
    pub(crate) fn content_landed(&self, cx: &gpui::App) -> bool {
        let image = self.subject.as_ref().is_some_and(|subject| {
            matches!(
                subject.body,
                super::Body::Image { .. } | super::Body::Link { .. } | super::Body::Raw { .. }
            )
        });
        self.content_ready(cx)
            && (!image || self.svg.as_ref().is_none_or(|picture| !picture.interim))
    }

    pub(crate) fn observe_reveal(&mut self, cap: bool, cx: &gpui::App) {
        let now = self.observation_time(cx);
        let drawable = self.content_ready(cx);
        let final_ready = self.content_landed(cx);
        let mut observed = self.observations.borrow_mut();
        let landed = observed.landed;
        observed.revealed(now, drawable, cap);
        observed.landed = landed || final_ready;
    }

    pub(crate) fn reveal_cap(&mut self, cx: &mut gpui::Context<Self>) {
        self.observe_reveal(true, cx);
        self.mark_revealed(cx);
    }
}
