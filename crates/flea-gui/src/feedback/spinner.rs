//! Omi loading marks, sampled before paint and repainted above the cached pane.
#[path = "spinner_player.rs"]
mod player;
use gpui::{
    fill, point, px, size, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};
pub(crate) use player::request_frame;
pub use player::{device_side, Mode, Motion, OMI_GRID_PX};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub const HERO: f32 = 66.;
pub const GAP: f32 = 9.;
// Theme.qml heroMarkSize is bodySmall times this, rounded.
const HERO_MARK_RATIO: f32 = 3.7;

/// Animation, an undrawn floor, or a fixed time in seconds into the mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arm {
    Crawl,
    Hidden,
    Pinned(f32),
}

#[derive(Default)]
pub struct Counters {
    /// Frames painted while shown.
    pub frames: Cell<usize>,
    /// Successful preparations that sample the player and write its offset.
    pub writes: Cell<usize>,
    /// Moving frames asking the shared window scheduler for another frame.
    pub requests: Cell<usize>,
    /// Pane renders while already shown.
    pub others: Cell<usize>,
}

fn bump(counter: &Cell<usize>) {
    counter.set(counter.get() + 1);
}

/// One persistent player, hidden without sampling or requesting frames.
#[derive(Default)]
pub struct Crawl {
    shown: Cell<bool>,
    reduced: Cell<bool>,
    mode: Cell<Mode>,
    motion: RefCell<Option<Motion>>,
    offset: Cell<f32>,
    slot: Cell<Bounds<Pixels>>,
    mask: Cell<ContentMask<Pixels>>,
    ink: Cell<Hsla>,
    /// The retained legacy spiral painter; production listing loaders select Omi.
    spiral: Cell<bool>,
    spiral_side: Cell<f32>,
    since: Cell<Option<std::time::Instant>>,
    pub counters: Counters,
}

// Spinner.qml's crawl clock, in seconds and stroke units; spiral_seconds inverts spiral_phase on it.
const SPIRAL_STILL_S: f32 = 1.;
const SPIRAL_PERIOD: f32 = 57.;
const SPIRAL_CYCLE_S: f32 = 1.6;

/// Spinner.qml holds still for its first second, then walks one 57 unit dash period per 1.6 s.
pub fn spiral_phase(seconds: f32) -> f32 {
    ((seconds - SPIRAL_STILL_S).max(0.) / SPIRAL_CYCLE_S * SPIRAL_PERIOD).rem_euclid(SPIRAL_PERIOD)
}

/// The time into the crawl at which `spiral_phase` reads `phase`, for a mark held still on one frame.
pub fn spiral_seconds(phase: f32) -> f32 {
    SPIRAL_STILL_S + phase.rem_euclid(SPIRAL_PERIOD) / SPIRAL_PERIOD * SPIRAL_CYCLE_S
}

impl Crawl {
    pub fn shown(&self) -> bool {
        self.shown.get()
    }
    /// Reduced-motion policy used by the last successful player preparation.
    pub(crate) fn reduced(&self) -> bool {
        self.reduced.get()
    }
    pub fn mode(&self) -> Mode {
        self.mode.get()
    }
    pub fn label(&self) -> &'static str {
        self.mode.get().label()
    }
    pub fn set(&self, mode: Mode) {
        if self.mode.replace(mode) != mode {
            if let Some(motion) = self.motion.borrow_mut().as_mut() {
                motion.set(mode);
            }
        }
    }
    pub(crate) fn slot(&self) -> Bounds<Pixels> {
        self.slot.get()
    }
    pub(crate) fn ink(&self) -> Hsla {
        self.ink.get()
    }
    /// Retained IPC field: elapsed mode seconds, zero at rest or hidden.
    pub fn offset(&self) -> f32 {
        self.offset.get()
    }
    pub fn show(&self, shown: bool, ink: Hsla) {
        let was = self.shown.replace(shown);
        self.ink.set(ink);
        if was && !shown {
            *self.motion.borrow_mut() = None;
            self.offset.set(0.);
            self.since.set(None);
        }
    }
    /// Selects the retained v0.3.12 spiral painter for an explicit caller.
    pub fn show_spiral(&self, shown: bool, ink: Hsla) {
        self.spiral.set(true);
        self.show(shown, ink);
    }
    pub fn is_spiral(&self) -> bool {
        self.spiral.get()
    }
    pub fn rendered(&self) {
        if self.shown.get() {
            bump(&self.counters.others);
        }
    }
    /// Samples upstream rects before paint; the pinned and reduced-motion arms never schedule.
    pub fn prepare(&self, moving: bool, pinned: Option<f32>, window: &mut Window, cx: &mut App) {
        if !self.shown.get() || !window.is_visible() {
            return;
        }
        self.reduced.set(cx.reduce_motion());
        let pinned = if cx.reduce_motion() { None } else { pinned };
        if self.spiral.get() {
            let face = crate::list::Typeface::for_window(window, cx);
            self.spiral_side
                .set((f32::from(face.body_small) * HERO_MARK_RATIO).round());
            let seconds = match pinned {
                Some(seconds) => seconds.max(0.),
                None if moving && !cx.reduce_motion() => {
                    let now = cx.background_executor().now();
                    let since = self.since.get().unwrap_or(now);
                    self.since.set(Some(since));
                    now.saturating_duration_since(since).as_secs_f32()
                }
                None => {
                    self.since.set(None);
                    0.
                }
            };
            self.offset.set(seconds);
            bump(&self.counters.writes);
            if moving && pinned.is_none() && !cx.reduce_motion() {
                bump(&self.counters.requests);
                player::request_frame(window, cx);
            }
            return;
        }
        let slot = self.slot.get();
        let side =
            device_side(f32::from(slot.size.width.min(slot.size.height)) * window.scale_factor());
        let mut state = self.motion.borrow_mut();
        let motion = state.get_or_insert_with(|| Motion::new(self.mode.get()));
        motion.sample(
            moving && !cx.reduce_motion(),
            pinned,
            side,
            cx.background_executor().now(),
        );
        self.offset.set(motion.seconds());
        bump(&self.counters.writes);
        if moving && pinned.is_none() && !cx.reduce_motion() {
            bump(&self.counters.requests);
            player::request_frame(window, cx);
        }
    }
    /// Paints the prepared rects without player work or heap allocation.
    pub fn paint(&self, window: &mut Window) {
        if !self.shown.get() || !window.is_visible() {
            return;
        }
        if self.spiral.get() {
            bump(&self.counters.frames);
            window.with_content_mask(Some(self.mask.get()), |window| self.paint_spiral(window));
            return;
        }
        let motion = self.motion.borrow();
        let Some(motion) = motion.as_ref() else {
            return;
        };
        bump(&self.counters.frames);
        window.with_content_mask(Some(self.mask.get()), |window| {
            paint_rects(motion, self.slot.get(), self.ink.get(), window);
        });
    }
    fn paint_spiral(&self, window: &mut Window) {
        let ink = self.ink.get();
        self.spiral_rects(window.scale_factor(), |quad, coverage| {
            window.paint_quad(fill(quad, ink.opacity(coverage)));
        });
    }
    /// The spiral's dash at the current phase, as logical bounds and pixel coverage, without allocating.
    pub(crate) fn spiral_rects(&self, scale: f32, mut emit: impl FnMut(Bounds<Pixels>, f32)) {
        use crate::g2::status_feedback::brand::{dashed, GRID};
        let slot = self.slot.get();
        let side = px(self.spiral_side.get()).min(slot.size.width.min(slot.size.height));
        let unit = f32::from(side) * scale / GRID;
        let left = (f32::from(slot.origin.x + (slot.size.width - side) / 2.) * scale).round();
        let top = (f32::from(slot.origin.y + (slot.size.height - side) / 2.) * scale).round();
        dashed(
            spiral_phase(self.offset.get()),
            unit,
            |[x0, y0, x1, y1], coverage| {
                let quad = Bounds::from_corners(
                    point(px((left + x0) / scale), px((top + y0) / scale)),
                    point(px((left + x1) / scale), px((top + y1) / scale)),
                );
                emit(quad, coverage);
            },
        );
    }
    pub fn place(&self, bounds: Bounds<Pixels>, window: &Window) {
        self.slot.set(bounds);
        self.mask.set(window.content_mask());
    }
}

/// Device-pixel bounds, rounded at each edge after mapping the kit's full view.
pub fn mapped(
    rect: &omi::Rect,
    view: [f64; 4],
    slot: Bounds<Pixels>,
    scale: f32,
) -> Bounds<Pixels> {
    let side = device_side(f32::from(slot.size.width.min(slot.size.height)) * scale);
    let left = (f32::from(slot.origin.x) * scale
        + (f32::from(slot.size.width) * scale - side) / 2.)
        .round();
    let top = (f32::from(slot.origin.y) * scale
        + (f32::from(slot.size.height) * scale - side) / 2.)
        .round();
    let unit = f64::from(side) / view[2];
    let x = |v: f64| px((f64::from(left) + (v - view[0]) * unit).round() as f32 / scale);
    let y = |v: f64| px((f64::from(top) + (v - view[1]) * unit).round() as f32 / scale);
    Bounds::from_corners(
        point(x(rect.x), y(rect.y)),
        point(x(rect.x + rect.w), y(rect.y + rect.h)),
    )
}

pub fn paint_rects(motion: &Motion, slot: Bounds<Pixels>, ink: Hsla, window: &mut Window) {
    for rect in motion.rects() {
        window.paint_quad(fill(
            mapped(rect, motion.view(), slot, window.scale_factor()),
            ink.opacity(rect.opacity as f32),
        ));
    }
}

/// The loading stack reserves one caption line, so a changed message cannot recenter the mark.
pub fn hero(
    mark: Rc<Crawl>,
    caption: gpui::Div,
    typeface: &crate::list::Typeface,
    window: &Window,
) -> gpui::Div {
    use gpui::prelude::*;
    #[cfg(test)]
    let caption = super::loading_caption_tests::watch_caption(caption, Rc::clone(&mark));
    let line = window
        .text_style()
        .line_height
        .to_pixels(typeface.caption.into(), window.rem_size())
        .round();
    let caption_offset = caption_lift(typeface.body_small);
    gpui::div()
        .flex()
        .flex_col()
        .items_center()
        .h(px(HERO + GAP) + line)
        .flex_shrink_0()
        .gap(px(GAP))
        .child(Slot(mark))
        .child(
            gpui::div()
                .relative()
                .top(-caption_offset)
                .flex_shrink_0()
                .child(caption),
        )
}

/// LoadingState.qml centres its caption under heroMarkSize; the lift stops at GAP so it never enters Omi's slot.
pub(super) fn caption_lift(body_small: Pixels) -> Pixels {
    px(((HERO - (f32::from(body_small) * HERO_MARK_RATIO).round()) / 2.).min(GAP))
}

/// The mark's place in the pane's layout; it records its bounds and its pane's clip and paints nothing, so a crawl frame leaves the pane alone.
pub struct Slot(pub Rc<Crawl>);

impl IntoElement for Slot {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Slot {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(("omi-loading-slot", Rc::as_ptr(&self.0) as u64).into())
    }
    fn a11y_role(&self) -> Option<gpui::Role> {
        Some(gpui::Role::Image)
    }
    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        node.set_label(self.0.label());
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
            size: size(px(HERO).into(), px(HERO).into()),
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        self.0.place(bounds, window);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}
