//! Menu geometry: row heights, the width fit, placement in the work area, wheel steps and the half-row scroll cue.
use super::entry::Entry;
use super::rows;

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
    if entry.separator {
        SEP
    } else {
        ROW
    }
}

pub fn content_height(items: &[Entry]) -> i32 {
    items.iter().map(height).sum()
}

pub fn row_top(items: &[Entry], index: usize) -> i32 {
    items[..index.min(items.len())].iter().map(height).sum()
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
    let mut width = 2. + PAD_X + SLOT + GAP + parts.label + PAD_X;
    if parts.word > 0. {
        width += GAP + parts.word;
    }
    if parts.key > 0. {
        width += GAP + parts.key;
    }
    if parts.chevron || parts.word > 0. {
        width += GAP + CHEVRON;
    }
    width
}

// The menu width, or the widest row's own when a label and its hint need more, never past the work area.
pub fn fit_width(widest: f32, work_width: f32) -> f32 {
    MENU_W.max(widest.ceil()).min(work_width).max(0.)
}

// A flyout is at least its parent's width.
pub fn flyout_width(parent: f32, widest: f32, work_width: f32) -> f32 {
    parent
        .max(MENU_W)
        .max(widest.ceil())
        .min(work_width)
        .max(0.)
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
    let h = ((content + 2 * PAD) as f32)
        .min(work.h - 2. * inset)
        .max(0.);
    let x = work.x + rows::clamp(point.0 - work.x, width, work.w);
    let y =
        work.y + inset + rows::clamp(point.1 - work.y - inset, h, (work.h - 2. * inset).max(0.));
    Rect { x, y, w: width, h }
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
    let h = ((content + 2 * PAD) as f32)
        .min(work.h - 2. * inset)
        .max(0.);
    let right = work.x + work.w;
    let x = if frame.x + frame.w + width <= right {
        frame.x + frame.w
    } else if frame.x - width >= work.x {
        frame.x - width
    } else {
        (right - inset - width).max(work.x)
    };
    let y = (row_screen_top - PAD as f32)
        .min(work.y + work.h - inset - h)
        .max(work.y + inset);
    Rect { x, y, w: width, h }
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
    let top = row_top(items, index);
    let end = content_height(items) - view_h;
    if end <= 0 {
        return 0;
    }
    if top < content_y + CLEAR {
        return (top - CLEAR).max(0);
    }
    if top + ROW > content_y + view_h - CLEAR {
        return end.min(top + ROW - view_h + CLEAR);
    }
    content_y
}

type Cut = Option<(usize, i32)>;

// The item each edge of the view cuts and how much of it still shows.
pub fn edge_cuts(items: &[Entry], content_y: i32, view_h: i32) -> (Cut, Cut) {
    let (mut top, mut bottom, mut y) = (None, None, 0);
    for (index, item) in items.iter().enumerate() {
        let h = height(item);
        if y < content_y && content_y < y + h {
            top = Some((index, y + h - content_y));
        }
        if y < content_y + view_h && content_y + view_h < y + h {
            bottom = Some((index, content_y + view_h - y));
        }
        y += h;
    }
    (top, bottom)
}

fn half(items: &[Entry], cut: Cut) -> bool {
    cut.is_some_and(|(index, shown)| {
        !items[index].separator && (ROW / 2 - 1..ROW - ROW / 2 + 2).contains(&shown)
    })
}

// Every clipped edge cuts a row at half; an edge at the menu's own start or end needs nothing.
pub fn cue_ok(items: &[Entry], content_y: i32, view_h: i32) -> bool {
    let (top, bottom) = edge_cuts(items, content_y, view_h);
    (content_y <= 0 || half(items, top))
        && (content_y + view_h >= content_height(items) || half(items, bottom))
}

// The kept row may reach into a clipped edge's fade by the cue's give at most.
pub fn keeps_clear(items: &[Entry], keep: Option<usize>, content_y: i32, view_h: i32) -> bool {
    let Some(index) = keep else { return true };
    let top = row_top(items, index) - content_y;
    let reach = CLEAR - CUE_GIVE;
    (content_y == 0 || top >= reach)
        && (content_y + view_h >= content_height(items) || top + ROW <= view_h - reach)
}

fn balance(items: &[Entry], content_y: i32, view_h: i32) -> i32 {
    let (top, bottom) = edge_cuts(items, content_y, view_h);
    [top, bottom]
        .iter()
        .flatten()
        .map(|(_, shown)| (2 * shown - ROW).abs())
        .sum()
}

/// The nearest scroll and view height that cut every clipped edge's row at half, keeping the cursor clear.
pub fn menu_cut(items: &[Entry], content_y: i32, view_h: i32, keep: Option<usize>) -> (i32, i32) {
    let end = content_height(items) - view_h;
    if end <= 0 {
        return (0, view_h);
    }
    let fits = |y: i32, h: i32| cue_ok(items, y, h) && keeps_clear(items, keep, y, h);
    if fits(content_y, view_h) {
        return (content_y, view_h);
    }
    let best = (0..=end).filter(|y| fits(*y, view_h)).min_by_key(|y| {
        (
            (y - content_y).abs() / ROW,
            balance(items, *y, view_h),
            (y - content_y).abs(),
        )
    });
    if let Some(y) = best {
        return (y, view_h);
    }
    // Rows overflowing by less than half a row cannot scroll to a half row, so the view stops at one instead.
    let trim = (view_h - ROW + 1..view_h)
        .rev()
        .filter(|h| fits(content_y, *h))
        .min_by_key(|h| (balance(items, content_y, *h), -h));
    match trim {
        Some(h) => (content_y, h),
        None => (content_y, view_h),
    }
}

// The scroll a cursor move lands on: revealed first, then cut so a half row says the menu goes on.
pub fn scroll_for(items: &[Entry], cursor: usize, content_y: i32, view_h: i32) -> (i32, i32) {
    menu_cut(
        items,
        reveal(items, cursor, content_y, view_h),
        view_h,
        Some(cursor),
    )
}

// Which edge fades draw: one only where rows lie past that edge.
pub fn fades(items: &[Entry], content_y: i32, view_h: i32) -> (bool, bool) {
    (content_y > 0, content_y + view_h < content_height(items))
}
