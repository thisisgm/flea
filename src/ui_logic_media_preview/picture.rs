//! Where a decoded picture sits in its box, and the size a decoder is asked for.
use super::frame::{MAX_HEIGHT, MAX_WIDTH};

/// A picture fitted inside a box without cropping, centred: x, y, width, height.
pub fn fit(width: f32, height: f32, box_w: f32, box_h: f32) -> [f32; 4] {
    if width <= 0. || height <= 0. || box_w <= 0. || box_h <= 0. {
        return [0., 0., 0., 0.];
    }
    let scale = (box_w / width).min(box_h / height);
    let (w, h) = (width * scale, height * scale);
    [(box_w - w) / 2., (box_h - h) / 2., w, h]
}

/// The decoded size for a box: the clip's own, shrunk to fit the box's device pixels and the wire's bounds, rounded up to even pixels.
pub fn decode_size(width: u32, height: u32, box_w: u32, box_h: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    let limit_w = box_w.clamp(2, MAX_WIDTH) as f64;
    let limit_h = box_h.clamp(2, MAX_HEIGHT) as f64;
    let scale = (limit_w / f64::from(width))
        .min(limit_h / f64::from(height))
        .min(1.);
    let even = |value: f64| ((value.ceil() as u32).div_ceil(2) * 2).max(2);
    (
        even(f64::from(width) * scale).min(MAX_WIDTH),
        even(f64::from(height) * scale).min(MAX_HEIGHT),
    )
}
