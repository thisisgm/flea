//! FleaWindow's geometry and colour strings, as ui/Ipc.qml's readers answer them.
use gpui::{Bounds, Hsla, Pixels};

// Math.round(): half rounds up, so a negative half lands where QML's does.
pub(crate) fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

/// FleaWindow.rectOf(): "x y width height" in window pixels, the far edges rounded before the size is taken.
pub fn rect_of(x: f32, y: f32, width: f32, height: f32) -> String {
    let (left, top) = (round(x), round(y));
    format!(
        "{left} {top} {} {}",
        round(x + width) - left,
        round(y + height) - top
    )
}

/// FleaWindow.centreOf(): the box's middle as whole pixels.
pub fn centre_of(x: f32, y: f32, width: f32, height: f32) -> String {
    format!("{} {}", round(x + width / 2.), round(y + height / 2.))
}

pub(super) fn rect_of_bounds(bounds: Bounds<Pixels>) -> String {
    rect_of(
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

pub(super) fn centre_of_bounds(bounds: Bounds<Pixels>) -> String {
    centre_of(
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

/// QML's color string: "#rrggbb" when opaque, "#aarrggbb" otherwise.
pub fn colour(value: Hsla) -> String {
    let rgba = value.to_rgb();
    let byte = |channel: f32| (channel.clamp(0., 1.) * 255.).round() as u8;
    let (r, g, b, a) = (byte(rgba.r), byte(rgba.g), byte(rgba.b), byte(rgba.a));
    if a == u8::MAX {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{a:02x}{r:02x}{g:02x}{b:02x}")
    }
}
