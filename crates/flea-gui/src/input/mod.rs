//! Native input policy shared by the mounted view and the window's read-only observers.
mod actions;
pub(crate) use actions::follow_cursor;
mod band;
mod element;
pub mod focus;
mod pointer;
mod pointer_press;
pub mod scroll;
pub mod selection;
// The oracle tests keep their input::slow path while the rename owner supplies the shared policy.
pub use crate::window::rename::slow::policy as slow;
pub(crate) mod views;
mod wheel;
pub use band::{update_band, EDGE_RATE};
pub(crate) use wheel::delivery as wheel_delivery;

// Preserve the current wheel default while keeping its applied policy explicit.
pub(crate) const PLATFORM_SCROLL_GAIN: f32 = 3.;
pub(crate) const WHEEL_LINES: u32 = PLATFORM_SCROLL_GAIN as u32;
// Theme.qml scroll: a wheel line is notchPx 24, times the multiplier 4; Scroll.js moves touchpad pixels times TOUCH_GAIN.
pub(crate) const NOTCH_PX: f32 = 24.;
pub(crate) const WHEEL_MULTIPLIER: f32 = 4.;
pub(crate) const TOUCH_GAIN: f32 = 2.5;

pub(crate) use element::grid_surface;
pub use element::{surface, surface_with, Painted};
use gpui::{Bounds, Pixels, Point};
#[cfg(test)]
pub(crate) use pointer::Press;
use selection::Selection;

/// Filter.js:241-251: the row a wheel pull left, valid while its cursor and listing are unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pull {
    pub from: usize,
    pub to: usize,
    pub listing: usize,
}

pub type HitBoxes = std::rc::Rc<[(usize, Bounds<Pixels>)]>;

/// A notch's OutCubic glide: the drawn position eases from where it stood to the written one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    pub from: f32,
    pub to: f32,
    pub elapsed: f32,
}

/// The drawn position while a glide runs; the written one once it lands.
pub fn glided(glide: Option<Glide>, at: f32) -> f32 {
    glide.map_or(at, |glide| {
        let t = f64::from(glide.elapsed) / scroll::RETURN_MS;
        glide.from + (at - glide.from) * scroll::out_cubic(t) as f32
    })
}

/// Half a pixel: a row edge within it of the viewport edge counts as drawn whole, so float drift never cuts a row.
const WHOLE_SLACK: f32 = 0.5;

/// The first and last row lines wholly inside the viewport at `scroll`; the floor rows when no line fits whole.
fn whole_lines(frame: Frame, scroll: f32) -> (usize, usize) {
    let (grow, row) = (frame.grow, frame.row);
    // Grid lines start below the grid's top gap; list lines start at the content top.
    let scroll = scroll - frame.cell_origin.1;
    let lines = frame.total.div_ceil(frame.columns.max(1));
    let end = scroll + frame.viewport();
    let floor = grow.position_at(scroll, row).min(lines - 1);
    let visible = ((frame.viewport() / row).ceil() as usize).max(1);
    let mut top = floor;
    if grow.top(top, row) < scroll - WHOLE_SLACK {
        top += 1;
    }
    let mut bottom = grow.position_at(end, row).min(lines - 1);
    while bottom > top && grow.top(bottom, row) + grow.height(bottom, row) > end + WHOLE_SLACK {
        bottom -= 1;
    }
    if top > bottom || grow.top(bottom, row) + grow.height(bottom, row) > end + WHOLE_SLACK {
        return (floor, (floor + visible - 1).min(lines - 1));
    }
    (top, bottom)
}

#[derive(Default)]
pub struct PaneInput {
    pub selection: Selection,
    pub shown: Option<std::rc::Rc<[usize]>>,
    pub hit_boxes: Option<HitBoxes>,
    pub hovered: Option<usize>,
    pub pointer_origin: bool,
    pub pulled: Option<Pull>,
    prepared: Option<Frame>,
    pub frame: Option<Frame>,
    pub drawn: Option<element::Painted>,
    pub band: Option<Band>,
    pub grab: Option<f32>,
    pub(crate) press: Option<pointer::Press>,
    pub(crate) tap: Option<pointer::Tap>,
    pub last_pointer: Option<Point<Pixels>>,
    /// Where the pointer last moved over this pane's own surface, nothing drawn over it.
    pub surface_pointer: Option<Point<Pixels>>,
    pub in_lane: bool,
    pub moved_at: Option<std::time::Instant>,
    pub timer: Option<gpui::Task<()>>,
    pub stepped_at: Option<std::time::Instant>,
    pub(crate) row_drag_at: Option<std::time::Instant>,
    /// The touchpad tail, rubber-band edges and their OutCubic return.
    pub kinetic: scroll::Kinetic,
    pub glide: Option<Glide>,
    /// What a touchpad stroke drew past an end, beyond the clamped position the pane keeps.
    pub over: f32,
    /// True between a touchpad stroke's Started and its Ended; only motion inside one may pass an end.
    pub stroke: bool,
    /// The clamped position the motion last wrote, so any other write ends the motion.
    pub(crate) wrote: Option<f32>,
    pub(crate) motion_at: Option<std::time::Instant>,
    epoch: Option<std::time::Instant>,
    pub(crate) queued: Option<actions::Queued>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub bounds: Bounds<Pixels>,
    pub row: f32,
    pub lane: f32,
    pub content: f32,
    pub scroll: f32,
    pub listing: usize,
    pub total: usize,
    pub columns: usize,
    pub cell_width: f32,
    /// The refused rename's band: the rows below its edited row sit lower by it.
    pub grow: crate::list::pane::Grow,
    pub cell_origin: (f32, f32),
}

impl Frame {
    /// The list's own frame: one row per cell, the header and rows below the bounds' top.
    pub fn list(
        bounds: Bounds<Pixels>,
        metrics: &crate::list::Metrics,
        pane: &crate::list::pane::Pane,
        grow: crate::list::pane::Grow,
    ) -> Self {
        Self {
            bounds,
            row: metrics.row,
            lane: metrics.lane,
            content: grow.content(pane.held.total, metrics.row) + metrics.header,
            scroll: pane.scroll_y,
            listing: pane.held.listing,
            total: pane.held.total,
            columns: 1,
            cell_width: f32::from(bounds.size.width) - metrics.lane,
            grow,
            cell_origin: (0., 0.),
        }
    }
    pub fn viewport(self) -> f32 {
        self.bounds.size.height.into()
    }
    pub fn max_scroll(self) -> f32 {
        (self.content - self.viewport()).max(0.)
    }
    pub fn local(self, point: Point<Pixels>) -> (f32, f32) {
        (
            (point.x - self.bounds.left()).into(),
            (point.y - self.bounds.top()).into(),
        )
    }
    pub fn in_lane(self, point: Point<Pixels>) -> bool {
        self.bounds.contains(&point) && point.x >= self.bounds.right() - gpui::px(self.lane)
    }
    pub fn row_at(self, point: Point<Pixels>) -> Option<usize> {
        if !self.bounds.contains(&point) || self.in_lane(point) || self.row <= 0. {
            return None;
        }
        let (_, y) = self.local(point);
        let (x, _) = self.local(point);
        let (x, y) = (x - self.cell_origin.0, y + self.scroll - self.cell_origin.1);
        if x < 0. || y < 0. {
            return None;
        }
        let column = (x / self.cell_width.max(1.)).floor() as usize;
        if column >= self.columns {
            return None;
        }
        let position = self.grow.position_at(y, self.row);
        let row = position * self.columns + column;
        (row < self.total).then_some(row)
    }
}

pub struct Band {
    pub start: Point<Pixels>,
    pub anchor: (f32, f32),
    pub end: (f32, f32),
    pub before: Vec<usize>,
    pub cursor: usize,
    pub active: bool,
    pub additive: bool,
    /// A Shift tap that never travels keeps the selection, as SelectionBand.qml does.
    pub shift: bool,
    pub direction: f32,
    pub last: Option<usize>,
    pub cells: Option<(usize, usize, usize, usize)>,
    /// The columns and row count `cells` were selected under; None until the next update rebuilds from `before`.
    pub basis: Option<(usize, usize)>,
    pub pointer: Point<Pixels>,
}

impl PaneInput {
    pub(crate) fn prepared_frame(&self) -> Option<Frame> {
        self.prepared
    }

    /// Call before preparing a view's row states so rollback and the new geometry present together.
    pub fn prepare(&mut self, frame: Frame, selected: &mut Vec<usize>, cursor: &mut usize) {
        // A rename band moves only the rows below its own row, and gestures hold content positions, so it never cancels one.
        if self.prepared.is_some_and(|old| {
            old.bounds != frame.bounds
                || old.listing != frame.listing
                || old.total != frame.total
                || old.row != frame.row
                || old.columns != frame.columns
                || old.cell_width != frame.cell_width
                || old.cell_origin != frame.cell_origin
                || old.lane != frame.lane
        }) {
            self.cancel(selected, cursor);
            self.press = None;
            // The old row is gone, but a still pointer keeps the hover; rehover re-reads the row now under it.
            self.hovered = None;
            self.moved_at = None;
            self.halt();
        }
        self.prepared = Some(frame);
    }
    /// Filter.js:254-259: the pulled-from row, only while no move, mark or new listing came since.
    pub fn take_pull(&mut self, cursor: usize, listing: usize) -> Option<usize> {
        let pull = self.pulled.take()?;
        (pull.to == cursor && pull.listing == listing).then_some(pull.from)
    }
    /// List.qml:213-231 and Filter.js:241-251: after a scroll outside a band the cursor stays in the viewport rows.
    pub fn scrolled(&mut self, scroll: f32, cursor: &mut usize) {
        let views = |frame: Frame| {
            self.shown
                .as_ref()
                .map_or(frame.total, |s| s.len().min(frame.total))
        };
        let Some(mut frame) = self.frame.filter(|&frame| views(frame) > 0) else {
            return;
        };
        // Under a filter the clamp moves over the shown rows only, never onto a hidden one.
        frame.total = views(frame);
        frame.scroll = scroll;
        let columns = frame.columns.max(1);
        let (top, bottom) = whole_lines(frame, scroll);
        let first = top * columns;
        let last = ((bottom + 1) * columns - 1).min(frame.total - 1);
        let first = first.min(last);
        let was = self.view_index(*cursor);
        let to = was.clamp(first, last);
        if to != was {
            let from = self.take_pull(*cursor, frame.listing).unwrap_or(*cursor);
            *cursor = self.index(to);
            self.pulled = Some(Pull {
                from,
                to: *cursor,
                listing: frame.listing,
            });
        }
    }
    /// The position the list draws: the glide's eased one plus a touchpad overshoot.
    pub fn drawn(&self, scroll_y: f32) -> f32 {
        glided(self.glide, scroll_y) + self.over
    }
    /// Any motion still owed a frame: a tail, a return or a glide.
    pub fn moving(&self) -> bool {
        self.kinetic.running() || self.glide.is_some()
    }
    /// A new geometry or listing ends every motion without a write; its own landing places the view.
    pub fn halt(&mut self) {
        self.kinetic.halt();
        self.glide = None;
        self.over = 0.;
        self.wrote = None;
        self.motion_at = None;
    }
    pub fn index(&self, view: usize) -> usize {
        self.shown
            .as_ref()
            .and_then(|shown| shown.get(view))
            .copied()
            .unwrap_or(view)
    }
    pub fn view_index(&self, row: usize) -> usize {
        self.shown.as_ref().map_or(row, |shown| {
            shown.binary_search(&row).unwrap_or_else(|at| at)
        })
    }
    pub fn hit(&self, point: Point<Pixels>) -> Option<usize> {
        let frame = self.frame?;
        self.hit_in(frame, point)
    }
    pub(super) fn hit_in(&self, frame: Frame, point: Point<Pixels>) -> Option<usize> {
        if !frame.bounds.contains(&point) || frame.in_lane(point) {
            return None;
        }
        if let Some(boxes) = &self.hit_boxes {
            return boxes
                .iter()
                .find(|(_, rect)| rect.contains(&point))
                .map(|(row, _)| *row);
        }
        frame.row_at(point).and_then(|row| self.drawn_row(row))
    }
    /// The listing row drawn at this position; under a filter the frame spans every held row, so a position past the drawn ones is blank ground.
    pub fn drawn_row(&self, view: usize) -> Option<usize> {
        match &self.shown {
            Some(shown) => shown.get(view).copied(),
            None => Some(view),
        }
    }
    /// Where a still pointer rests, while it owns the hover and no press, band or grab holds it; Columns reads it by track bounds.
    pub fn resting(&self) -> Option<Point<Pixels>> {
        (self.pointer_origin && self.press.is_none())
            .then_some(self.last_pointer)
            .flatten()
            .filter(|_| self.band.is_none() && self.grab.is_none())
    }
    /// Qt re-hovers what moves under a still pointer, so a wheel scroll or a released drag lights the List row now there.
    pub fn rehover(&mut self, scroll: f32) {
        let Some(at) = self
            .resting()
            .filter(|&at| self.surface_pointer == Some(at))
        else {
            return;
        };
        // The painted frame's bounds at this frame's scroll, so the frame being prepared paints the row now there.
        let Some(frame) = self.frame else {
            return;
        };
        self.hovered = self.hit_in(Frame { scroll, ..frame }, at);
    }
    pub fn keyboard(&mut self) {
        self.pointer_origin = false;
        self.hovered = None;
    }
    pub fn moved(&mut self, now: std::time::Instant) {
        self.moved_at = Some(now);
    }
    pub fn shown(&self, now: std::time::Instant) -> bool {
        self.frame.is_some_and(|frame| frame.max_scroll() > 0.5)
            && (self.in_lane
                || self.grab.is_some()
                || self
                    .moved_at
                    .is_some_and(|at| now.saturating_duration_since(at).as_millis() < 1000))
    }
    pub fn opacity(&self, now: std::time::Instant, reduced: bool) -> f32 {
        if self.shown(now) {
            return 1.;
        }
        if reduced {
            return 0.;
        }
        self.moved_at.map_or(0., |at| {
            let elapsed = now.saturating_duration_since(at).as_millis() as f32;
            ((1300. - elapsed) / 300.).clamp(0., 1.)
        })
    }
    /// A new filter answer renumbers the view rows, so a band in flight rebuilds from its starting marks.
    pub fn set_shown(&mut self, shown: Option<std::rc::Rc<[usize]>>) {
        let same = match (&self.shown, &shown) {
            (Some(old), Some(new)) => std::rc::Rc::ptr_eq(old, new),
            (old, new) => old.is_none() && new.is_none(),
        };
        if let Some(band) = self.band.as_mut().filter(|_| !same) {
            band.basis = None;
        }
        self.shown = shown;
    }
    pub fn cancel(&mut self, selected: &mut Vec<usize>, cursor: &mut usize) {
        if let Some(band) = self.band.take().filter(|band| band.active) {
            *selected = band.before;
            *cursor = band.cursor;
            self.selection.changed(selected);
        }
        self.grab = None;
        self.timer = None;
        self.stepped_at = None;
    }
}

#[cfg(test)]
mod band_delta_tests;
#[cfg(test)]
mod whole_rows_tests;
