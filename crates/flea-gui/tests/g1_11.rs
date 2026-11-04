// The rail host builds as its own module until the composition lease registers it; this target compiles it from its source.
#[path = "../src/rail/mod.rs"]
pub mod rail;

#[path = "g1_11/host.rs"]
mod host;
#[path = "g1_11/model.rs"]
mod model;

use rail::geometry::{Point, Rect};
use std::time::{Duration, Instant};

/// The board column at 1100x700: under the 27 px chrome, over the 27 px status bar.
pub fn column() -> Rect {
    Rect::new(0., 27., 1100., 646.)
}

pub fn at(origin: Instant, ms: u64) -> Instant {
    origin + Duration::from_millis(ms)
}

pub fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}
