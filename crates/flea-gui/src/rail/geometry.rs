//! Rail geometry at body 14 (design README): the width stops, the row pitch that bounds paint, the reveal strip and what the panes are owed.
use super::Mode;

// ui/js/Places.js WIDTH_STOPS; Settings offers exactly these.
pub const WIDTH_STOPS: [f32; 4] = [160., 192., 224., 256.];
pub const DEFAULT_WIDTH: f32 = 192.;
// .fb .rr height; the places producer lays out and hit tests its own rows, headings and gaps.
pub const ROW: f32 = 29.;
// Theme.spacing.rowPaddingX: the strip at the window's left edge that answers the pointer.
pub const EDGE: f32 = 14.;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: width.max(0.),
            height: height.max(0.),
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }
}

/// The saved width snapped to the nearest stop; anything unreadable is the 192 default.
pub fn width_stop(value: Option<f64>) -> f32 {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return DEFAULT_WIDTH;
    };
    let mut nearest = WIDTH_STOPS[0];
    for stop in &WIDTH_STOPS[1..] {
        if (f64::from(*stop) - value).abs() < (f64::from(nearest) - value).abs() {
            nearest = *stop;
        }
    }
    nearest
}

/// Where everything the host owns stands for one frame, in window coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    /// The column the rail lives in, from under the chrome and tabs to the status bar.
    pub column: Rect,
    /// What the panes give up: the rail's width docked, nothing hidden or as an overlay.
    pub inset: f32,
    /// The rail itself; an overlay slides on x only, so its width and rows never change with the motion.
    pub rail: Rect,
    /// The pointer strip at the left edge, an overlay's only.
    pub strip: Rect,
    /// The widest area a row drag over an overlay is refused in; Rail::refuses_drop keeps it to the strip until the rows have a width.
    pub drop: Rect,
}

pub fn place(mode: Mode, width: f32, column: Rect, slide: f32) -> Placement {
    let width = width.max(0.);
    let (inset, x, rail_width) = match mode {
        Mode::Docked => (width, column.x, width),
        Mode::Hidden => (0., column.x, 0.),
        Mode::Overlay => (0., column.x + (slide - 1.) * width, width),
    };
    let overlay = mode == Mode::Overlay;
    let lane = |w: f32| {
        Rect::new(
            column.x,
            column.y,
            if overlay { w } else { 0. },
            column.height,
        )
    };
    Placement {
        column,
        inset,
        rail: Rect::new(x, column.y, rail_width, column.height),
        strip: lane(EDGE),
        drop: lane(EDGE.max(width)),
    }
}

/// The most rows a rail of this height can show at any scroll offset, the bound on what it builds for paint.
pub fn drawn_rows(height: f32) -> usize {
    if height <= 0. {
        0
    } else {
        (height / ROW).ceil() as usize + 1
    }
}
