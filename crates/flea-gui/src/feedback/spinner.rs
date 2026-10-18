//! The Omarchy spiral as the loading mark: a dash crawls along the brand path as a few axis aligned quads, never a path or a rotation.
use gpui::{
    fill, point, px, size, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

/// The brand path `M21 21H3V3h18v14H7V7h10v6h-6` on the 24 unit grid, outer corner to centre.
const SPIRAL: [(f32, f32); 10] = [
    (21., 21.),
    (3., 21.),
    (3., 3.),
    (21., 3.),
    (21., 17.),
    (7., 17.),
    (7., 7.),
    (17., 7.),
    (17., 13.),
    (11., 13.),
];
/// Distance along the centreline to each vertex; the whole spiral is 114 units.
const AT: [f32; 10] = [0., 18., 36., 54., 68., 82., 92., 102., 108., 114.];
pub const GRID: f32 = 24.;
const LENGTH: f32 = 114.;
// The brand's stroke of 2 dashed 30 on and 27 off in stroke widths: one dash and one gap are the centreline exactly.
const DASH: f32 = 60.;
const HALF: f32 = 1.;
/// One walk of the dash offset from 0 to -57 stroke widths, looping, as the QML crawl ran.
pub const PERIOD_MS: u64 = 1600;
pub const TRAVEL: f32 = 57.;
/// The hero mark's side, board States at 48, and the gap to its word.
pub const HERO: f32 = 48.;
pub const GAP: f32 = 9.;
// Device pixel edges are quantized to 1/256 so coverage arithmetic is exact in f32.
const QUANTUM: f32 = 256.;

/// How a bench run draws the mark: crawling as shipped, not at all for the floor, or still at one offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arm {
    Crawl,
    Hidden,
    Pinned(f32),
}

/// Counts a pane's mark keeps for the IPC describe and the headless gate.
#[derive(Default)]
pub struct Counters {
    /// Frames the mark painted while shown.
    pub frames: Cell<usize>,
    /// Dash offset writes, one per painted frame.
    pub writes: Cell<usize>,
    /// Frames the mark asked for; none while hidden or still.
    pub requests: Cell<usize>,
    /// Pane renders while the mark was already shown.
    pub others: Cell<usize>,
}

fn bump(counter: &Cell<usize>) {
    counter.set(counter.get() + 1);
}

/// One pane's mark: shown by the pane's render, painted by the window overlay, reset by a hide.
#[derive(Default)]
pub struct Crawl {
    shown: Cell<bool>,
    since: Cell<Option<Instant>>,
    offset: Cell<f32>,
    slot: Cell<Bounds<Pixels>>,
    ink: Cell<Hsla>,
    pub counters: Counters,
}

impl Crawl {
    pub fn shown(&self) -> bool {
        self.shown.get()
    }

    #[cfg(test)]
    pub(crate) fn slot(&self) -> Bounds<Pixels> {
        self.slot.get()
    }

    #[cfg(test)]
    pub(crate) fn ink(&self) -> Hsla {
        self.ink.get()
    }

    /// The dash offset in stroke widths, 0 at rest and down to -57 at the end of a period.
    pub fn offset(&self) -> f32 {
        self.offset.get()
    }

    /// A hide stops the crawl and puts the dash back to rest, so the next show starts at 0.
    pub fn show(&self, shown: bool, ink: Hsla) {
        self.shown.set(shown);
        self.ink.set(ink);
        if !shown {
            self.since.set(None);
            self.offset.set(0.);
        }
    }

    /// A pane render while the mark is already on screen, which a crawl frame should never cause.
    pub fn rendered(&self) {
        if self.shown.get() {
            bump(&self.counters.others);
        }
    }

    /// Paints the mark in its slot and asks for the next frame only while shown and moving.
    pub fn paint(&self, moving: bool, pinned: Option<f32>, window: &mut Window, cx: &App) {
        if !self.shown.get() {
            return;
        }
        let now = cx.background_executor().now();
        let since = self.since.get().unwrap_or(now);
        self.since.set(Some(since));
        let phase = match pinned {
            Some(offset) => (-offset / TRAVEL).rem_euclid(1.),
            None if moving => phase(now.saturating_duration_since(since)),
            None => 0.,
        };
        self.offset
            .set(if phase == 0. { 0. } else { -TRAVEL * phase });
        bump(&self.counters.writes);
        bump(&self.counters.frames);
        let slot = self.slot.get();
        let scale = window.scale_factor();
        let side = slot.size.width.min(slot.size.height);
        let unit = f32::from(side) * scale / GRID;
        let left = (f32::from(slot.origin.x + (slot.size.width - side) / 2.) * scale).round();
        let top = (f32::from(slot.origin.y + (slot.size.height - side) / 2.) * scale).round();
        let ink = self.ink.get();
        cells(phase, unit, |[x0, y0, x1, y1], coverage| {
            let bounds = Bounds::from_corners(
                point(px((left + x0) / scale), px((top + y0) / scale)),
                point(px((left + x1) / scale), px((top + y1) / scale)),
            );
            window.paint_quad(fill(bounds, ink.opacity(coverage)));
        });
        if moving && pinned.is_none() {
            bump(&self.counters.requests);
            window.request_animation_frame();
        }
    }
}

/// Where in its period the crawl is after this long on screen, 0 to 1.
pub fn phase(elapsed: Duration) -> f32 {
    (elapsed.as_millis() % u128::from(PERIOD_MS)) as f32 / PERIOD_MS as f32
}

/// One dash at this phase as device pixel rectangles and their coverage, relative to the mark's corner.
pub fn cells(phase: f32, unit: f32, mut emit: impl FnMut([f32; 4], f32)) {
    let start = LENGTH * phase;
    let end = start + DASH;
    run(start, end.min(LENGTH), unit, &mut emit);
    if end > LENGTH {
        run(0., end - LENGTH, unit, &mut emit);
    }
}

// Square caps and miter corners are one more half stroke at each end; a run carried over a corner starts past the square its predecessor drew.
fn run(from: f32, to: f32, unit: f32, emit: &mut impl FnMut([f32; 4], f32)) {
    for at in 0..SPIRAL.len() - 1 {
        let (near, far) = (from.max(AT[at]), to.min(AT[at + 1]));
        if near >= far {
            continue;
        }
        let low = near - AT[at] + if from < AT[at] { HALF } else { -HALF };
        let high = far - AT[at] + HALF;
        let ((ax, ay), (bx, by)) = (SPIRAL[at], SPIRAL[at + 1]);
        let length = AT[at + 1] - AT[at];
        let (dx, dy) = ((bx - ax) / length, (by - ay) / length);
        let (px0, py0, px1, py1) = (ax + dx * low, ay + dy * low, ax + dx * high, ay + dy * high);
        let (wide, tall) = (HALF * dy.abs(), HALF * dx.abs());
        let rect = [
            (px0.min(px1) - wide) * unit,
            (py0.min(py1) - tall) * unit,
            (px0.max(px1) + wide) * unit,
            (py0.max(py1) + tall) * unit,
        ];
        cover(rect, emit);
    }
}

fn cover([x0, y0, x1, y1]: [f32; 4], emit: &mut impl FnMut([f32; 4], f32)) {
    for (top, bottom, down) in spans(y0, y1).into_iter().flatten() {
        for (left, right, across) in spans(x0, x1).into_iter().flatten() {
            emit([left, top, right, bottom], down * across);
        }
    }
}

// Whole device pixels in one span, plus a partly covered pixel at either end with its coverage.
fn spans(low: f32, high: f32) -> [Option<(f32, f32, f32)>; 3] {
    let (low, high) = (
        (low * QUANTUM).round() / QUANTUM,
        (high * QUANTUM).round() / QUANTUM,
    );
    let (first, last) = (low.ceil(), high.floor());
    if first > last {
        return [Some((last, last + 1., high - low)), None, None];
    }
    [
        (first > low).then_some((first - 1., first, first - low)),
        (last > first).then_some((first, last, 1.)),
        (high > last).then_some((last, last + 1., high - last)),
    ]
}

/// The mark's place in the pane's layout; it records its bounds and paints nothing, so a crawl frame leaves the pane alone.
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
        _: &mut Window,
        _: &mut App,
    ) {
        self.0.slot.set(bounds);
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
