//! Menu geometry: row heights, the width fit, placement in the work area, wheel steps and the half-row scroll cue.
use super::entry::Entry;
use super::rows;

mod metrics;
pub use metrics::Metrics;

// The board's menu rows: 37 tall, separators 10, 6 between the frame's outer edge and the first row.
pub const ROW: i32 = 37;
pub const SEP: i32 = 10;
pub const PAD: i32 = 6;
pub const MENU_W: f32 = 257.;
pub const PAD_X: f32 = 14.;
pub const GAP: f32 = 9.;
pub const SLOT: f32 = 18.;
pub const CHEVRON: f32 = 12.;
// A fade reaches round(37 x 0.7) past the padding, and a cursor row keeps that much clear of it.
pub const CLEAR: i32 = 26;
// The most of that clearance a scroll cue may take, so the cursor's label stays clear of the fade.
pub const CUE_GIVE: i32 = CLEAR / 2;
// One wheel notch in raw angleDelta units.
pub const NOTCH_UNITS: i32 = 120;

pub fn height(entry: &Entry) -> i32 {
    Metrics::default().height(entry)
}

pub fn content_height(items: &[Entry]) -> i32 {
    Metrics::default().content_height(items)
}

pub fn row_top(items: &[Entry], index: usize) -> i32 {
    Metrics::default().row_top(items, index)
}

/// What one row lays out, measured once per build by the presenter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowParts {
    pub label: f32,
    // A state word or caption, which keeps the chevron's column so every word ends on one edge.
    pub word: f32,
    pub key: f32,
    pub chevron: bool,
}

// The width a row needs whole: frame, padding, slot and gap, label, then each trailing part.
pub fn row_width(parts: RowParts) -> f32 {
    Metrics::default().row_width(parts)
}

// The menu width, or the widest row's own when a label and its hint need more, never past the work area.
pub fn fit_width(widest: f32, work_width: f32) -> f32 {
    Metrics::default().fit_width(widest, work_width)
}

// A flyout is at least its parent's width.
pub fn flyout_width(parent: f32, widest: f32, work_width: f32) -> f32 {
    Metrics::default().flyout_width(parent, widest, work_width)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

// The frame at a point, flipped then shifted, inside the work area's inset; its height stops at the work area.
pub fn place(point: (f32, f32), width: f32, content: i32, work: Rect, inset: f32) -> Rect {
    Metrics::default().place(point, width, content, work, inset)
}

// Beside the parent where it fits, else flipped to its left, else slid only as far as the edge needs.
pub fn place_flyout(
    frame: Rect,
    width: f32,
    content: i32,
    row_screen_top: f32,
    work: Rect,
    inset: f32,
) -> Rect {
    Metrics::default().place_flyout(frame, width, content, row_screen_top, work, inset)
}

// Raw angleDelta folded into whole notches: the remainder is kept, dropped on a flip and reset on open.
pub fn notch_steps(accum: i32, delta: i32) -> (i32, i32) {
    if delta == 0 {
        return (0, accum);
    }
    let accum = if accum != 0 && (accum < 0) != (delta < 0) {
        0
    } else {
        accum
    };
    let total = accum + delta;
    let notches = total / NOTCH_UNITS;
    (-notches, total - notches * NOTCH_UNITS)
}

// Gained touchpad travel folded into whole rows, the leftover riding to the next event.
pub fn touch_steps(accum: f32, gained: f32, row_height: f32) -> (i32, f32) {
    let row = row_height.max(1.);
    let mut total = accum + gained;
    let mut steps = 0;
    while total <= -row {
        steps += 1;
        total += row;
    }
    while total >= row {
        steps -= 1;
        total -= row;
    }
    (steps, total)
}

// The least scroll that keeps a row clear of either edge fade.
pub fn reveal(items: &[Entry], index: usize, content_y: i32, view_h: i32) -> i32 {
    Metrics::default().reveal(items, index, content_y, view_h)
}

type Cut = Option<(usize, i32)>;

// The item each edge of the view cuts and how much of it still shows.
pub fn edge_cuts(items: &[Entry], content_y: i32, view_h: i32) -> (Cut, Cut) {
    Metrics::default().edge_cuts(items, content_y, view_h)
}

// Every clipped edge cuts a row at half; an edge at the menu's own start or end needs nothing.
pub fn cue_ok(items: &[Entry], content_y: i32, view_h: i32) -> bool {
    Metrics::default().cue_ok(items, content_y, view_h)
}

// The kept row may reach into a clipped edge's fade by the cue's give at most.
pub fn keeps_clear(items: &[Entry], keep: Option<usize>, content_y: i32, view_h: i32) -> bool {
    Metrics::default().keeps_clear(items, keep, content_y, view_h)
}

/// The nearest scroll and view height that cut every clipped edge's row at half, keeping the cursor clear.
pub fn menu_cut(items: &[Entry], content_y: i32, view_h: i32, keep: Option<usize>) -> (i32, i32) {
    Metrics::default().menu_cut(items, content_y, view_h, keep)
}

// The scroll a cursor move lands on: revealed first, then cut so a half row says the menu goes on.
pub fn scroll_for(items: &[Entry], cursor: usize, content_y: i32, view_h: i32) -> (i32, i32) {
    Metrics::default().scroll_for(items, cursor, content_y, view_h)
}

// Which edge fades draw: one only where rows lie past that edge.
pub fn fades(items: &[Entry], content_y: i32, view_h: i32) -> (bool, bool) {
    Metrics::default().fades(items, content_y, view_h)
}
