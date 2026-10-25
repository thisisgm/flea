//! The grid's tile maths, ui/js/GridGeometry.js and ui/GridArea.qml's placement, with no window or text system.

/// Tile rows kept past the viewport, GridGeometry.cacheRows.
pub const CACHE_ROWS: usize = 2;
/// The six thumbnail stops in their walk order, Density.THUMB_SIZES.
pub const THUMB_STOPS: [(&str, f32); 6] = [
    ("small", 48.),
    ("medium", 64.),
    ("large", 96.),
    ("xlarge", 128.),
    ("huge", 192.),
    ("largest", 256.),
];
/// A mark never grows past 128 px, centred in the larger slots, Density.GLYPH_CAP.
pub const GLYPH_CAP: f32 = 128.;

/// One of the six thumbnail sizes, by its index in THUMB_STOPS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbSize(usize);

impl ThumbSize {
    pub const MEDIUM: Self = Self(1);

    /// The stop a saved preview.thumbSize names; anything else is medium, as Density.thumbPixelsFor answers.
    pub fn named(name: &str) -> Self {
        Self(
            THUMB_STOPS
                .iter()
                .position(|stop| stop.0 == name)
                .unwrap_or(1),
        )
    }
    pub fn name(self) -> &'static str {
        THUMB_STOPS[self.0].0
    }
    pub fn pixels(self) -> f32 {
        THUMB_STOPS[self.0].1
    }
    /// The stop this many steps away, clamped at both ends as GridArea.qml's zoomWheel clamps.
    pub fn stepped(self, steps: i32) -> Self {
        Self((self.0 as i64 + steps as i64).clamp(0, THUMB_STOPS.len() as i64 - 1) as usize)
    }
}

/// The four density stops, Density.STOPS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    Tight,
    #[default]
    Compact,
    Normal,
    Comfortable,
}

impl Density {
    pub fn named(name: &str) -> Self {
        match name {
            "tight" => Self::Tight,
            "normal" => Self::Normal,
            "comfortable" => Self::Comfortable,
            _ => Self::Compact,
        }
    }
    fn ratio(self) -> f32 {
        match self {
            Self::Tight => 0.,
            Self::Compact => 0.5,
            Self::Normal => 1.,
            Self::Comfortable => 1.5,
        }
    }
}

/// Theme tokens the grid is sized from, at one text size; the theme producer replaces board() with live values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub gap: f32,
    pub pad_x: f32,
    pub hairline: f32,
    pub min_cell: f32,
    pub caption_line: f32,
    pub chrome: f32,
    pub clip_mark: f32,
    pub body_small: f32,
    pub caption: f32,
    pub file_row: f32,
}

impl Metrics {
    /// Text size 14, Compact: Theme.qml's tokens as the grid board measures them (cell 147 by 134 at 908 wide).
    pub fn board() -> Self {
        Self {
            gap: 9.,
            pad_x: 14.,
            hairline: 1.,
            min_cell: 146.,
            caption_line: 12. * 15. / 11.,
            chrome: 27.,
            clip_mark: 12.,
            body_small: 13.,
            caption: 12.,
            file_row: 31.,
        }
    }
    /// Two caption lines, Theme.grid.captionHeight.
    pub fn caption_height(&self) -> f32 {
        2. * self.caption_line
    }
}

/// Density.gridPadY: the vertical pad tracks density with Compact anchored at the horizontal pad.
pub fn grid_pad_y(pad_x: f32, density: Density) -> f32 {
    (pad_x * (0.5 + density.ratio())).round()
}

/// Width less one gap and the scroll lane, so no tile sits under the bar; columnsFor(800, 146, 96, 7, 8, 0) is 5.
pub fn columns_for(width: f32, min_cell: f32, tile: f32, pad_x: f32, gap: f32, lane: f32) -> usize {
    let cell = min_cell.max(tile + 2. * pad_x);
    ((width - gap - lane) / cell).floor().max(1.) as usize
}

/// cellWidthFor(800, 5, 8, 0) is 158, five cells filling the 792 beside the gap.
pub fn cell_width_for(width: f32, columns: usize, gap: f32, lane: f32) -> f32 {
    ((width - gap - lane) / columns.max(1) as f32)
        .floor()
        .max(1.)
}

/// A cell is its slot, the name gap, two caption lines and the pad above and below; cellHeightFor(128, 9, 33, 14) is 198.
pub fn cell_height_for(thumb: f32, gap: f32, caption_height: f32, pad_y: f32) -> f32 {
    (thumb + gap + caption_height + 2. * pad_y).max(1.)
}

/// The window a grid holds, in whole tile rows: the list's buffer in pixels rebuilt in tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowPlan {
    pub columns: usize,
    pub visible: usize,
    pub buffer: usize,
    pub size: usize,
    pub margin: usize,
}

/// windowFor(18, 11, 150, 26, 124, 25) is 576 items of buffer, a 1350 window and a margin of two tile rows.
pub fn window_for(
    columns: usize,
    visible: usize,
    buffer_rows: usize,
    row_height: f32,
    cell_height: f32,
    refetch: usize,
) -> WindowPlan {
    let across = columns.max(1);
    let rows = ((buffer_rows as f32 * row_height / cell_height.max(1.)).ceil() as usize).max(1);
    let visible = visible.max(1);
    WindowPlan {
        columns: across,
        visible,
        buffer: across * rows,
        size: across * (visible + 2 * rows),
        margin: refetch.max(2 * across),
    }
}

/// The window the visible items owe once they come within the margin of an edge the held rows have more beyond; first may sit one row above 0 while the top margin shows.
pub fn drift_ask(
    plan: &WindowPlan,
    first: i64,
    last: i64,
    held: usize,
    length: usize,
    total: usize,
) -> Option<(usize, usize)> {
    let end = held + length;
    let behind = held > 0 && first - (held as i64) < plan.margin as i64;
    let ahead = end < total && (end as i64) - last < plan.margin as i64;
    (length == 0 || behind || ahead)
        .then(|| ((first - plan.buffer as i64).max(0) as usize, plan.size))
}

/// The window a far cursor landing owes before the view moves, None for a step or a landing the held rows reach.
pub fn landing_ask(
    plan: &WindowPlan,
    cursor: usize,
    first: i64,
    total: usize,
    held: usize,
    length: usize,
) -> Option<(usize, usize)> {
    let columns = plan.columns as i64;
    let row = cursor as i64 / columns;
    let seen = first.div_euclid(columns);
    if (row - seen).abs() <= plan.visible as i64 {
        return None;
    }
    let (before, after) = if row > seen {
        (plan.visible + 1, CACHE_ROWS + 1)
    } else {
        (CACHE_ROWS + 1, plan.visible + 1)
    };
    let start = ((row - before as i64) * columns - plan.margin as i64).max(0) as usize;
    let end = total.min((row as usize + after) * plan.columns + plan.margin);
    (start < held || end > held + length).then(|| (start, end.saturating_sub(start)))
}

/// A box in view pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
    pub fn right(&self) -> f32 {
        self.x + self.width
    }
}

/// Qt anchors.horizontalCenter start: an odd width's centre rounds up, so a 64 px slot sits 42 px into a 147 px cell.
pub fn centred_x(outer: f32, inner: f32) -> f32 {
    let half = |width: f32| {
        if width as i32 % 2 == 1 {
            (width + 1.) / 2.
        } else {
            width / 2.
        }
    };
    half(outer) - half(inner)
}

/// A decoded picture aspect-fit in its slot and centred on whole pixels, GridTile.qml's PreserveAspectFit in a slot-sized Image.
pub fn fit_picture(slot: Rect, width: u32, height: u32) -> Rect {
    if width == 0 || height == 0 {
        return Rect {
            x: slot.x,
            y: slot.y,
            width: 0.,
            height: 0.,
        };
    }
    let scale = (slot.width / width as f32).min(slot.height / height as f32);
    let (w, h) = (
        (width as f32 * scale).round().max(1.),
        (height as f32 * scale).round().max(1.),
    );
    Rect {
        x: slot.x + ((slot.width - w) / 2.).floor(),
        y: slot.y + ((slot.height - h) / 2.).floor(),
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_cell_is_147_by_134_at_908_wide() {
        let m = Metrics::board();
        let pad_y = grid_pad_y(m.pad_x, Density::Compact);
        let columns = columns_for(908., m.min_cell, 64., m.pad_x, m.gap, m.pad_x);
        assert_eq!(columns, 6);
        assert_eq!(cell_width_for(908., columns, m.gap, m.pad_x), 147.);
        assert_eq!(
            cell_height_for(64., m.gap, m.caption_height().ceil(), pad_y),
            134.
        );
        assert_eq!(
            [
                grid_pad_y(14., Density::Tight),
                pad_y,
                grid_pad_y(14., Density::Comfortable)
            ],
            [7., 14., 28.]
        );
        assert_eq!(
            (
                ThumbSize::named("huge").pixels(),
                ThumbSize::named("nope"),
                ThumbSize::MEDIUM.stepped(9).name()
            ),
            (192., ThumbSize::MEDIUM, "largest")
        );
    }
}
