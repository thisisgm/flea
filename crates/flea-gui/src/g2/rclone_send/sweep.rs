//! The check bar: rclone check reports no progress, so an accent segment sweeps a muted track until the job leaves checking.
use flea::ui_logic_rclone_send::{sweep, SWEEP_SHARE};
use gpui::{
    fill, point, px, relative, size, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};
use std::{cell::Cell, rc::Rc};

/// The board's bar is 6 px tall on a track at a quarter of muted-frame.
pub const HEIGHT: f32 = 6.;
pub const TRACK_OPACITY: f32 = 0.25;
/// Where the rclone-checking board pins the segment's left edge, as a share of the track.
pub const BOARD_AT: f32 = 0.38;

/// Counts the bar keeps for tests and the headless gate.
#[derive(Default)]
pub struct Counters {
    pub frames: Cell<usize>,
    /// Frames the bar asked for; none while pinned.
    pub requests: Cell<usize>,
}

/// One checking job's sweep; it starts on its first painted frame and lives as long as the job checks.
#[derive(Default)]
pub struct Sweep {
    since: Cell<Option<std::time::Instant>>,
    /// A still segment at this left share, for the board scene and paint tests.
    pinned: Cell<Option<f32>>,
    /// The segment's last painted left share and width share.
    drawn: Cell<(f32, f32)>,
    pub counters: Counters,
}

impl Sweep {
    pub fn pin(&self, at: Option<f32>) {
        self.pinned.set(at);
    }

    pub fn drawn(&self) -> (f32, f32) {
        self.drawn.get()
    }

    fn segment(&self, cx: &App) -> (f32, f32) {
        if let Some(at) = self.pinned.get() {
            return (at, SWEEP_SHARE.min(1. - at));
        }
        let now = cx.background_executor().now();
        let since = self.since.get().unwrap_or(now);
        self.since.set(Some(since));
        sweep(now.saturating_duration_since(since).as_millis() as u64)
    }
}

/// Paints one job's check bar in the width its block gives it, in the caller's muted-frame and accent-frame roles.
pub struct SweepBar {
    pub sweep: Rc<Sweep>,
    pub track: Hsla,
    pub segment: Hsla,
}

impl IntoElement for SweepBar {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for SweepBar {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), px(HEIGHT).into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let sweep = &self.sweep;
        let (left, width) = sweep.segment(cx);
        sweep.drawn.set((left, width));
        sweep.counters.frames.set(sweep.counters.frames.get() + 1);
        window.paint_quad(fill(bounds, self.track.opacity(TRACK_OPACITY)));
        if width > 0. {
            let x = bounds.origin.x + bounds.size.width * left;
            let segment = Bounds::new(
                point(x, bounds.origin.y),
                size(bounds.size.width * width, bounds.size.height),
            );
            window.paint_quad(fill(segment, self.segment));
        }
        if sweep.pinned.get().is_none() {
            sweep
                .counters
                .requests
                .set(sweep.counters.requests.get() + 1);
            window.request_animation_frame();
        }
    }
}
