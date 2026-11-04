//! What the last frame painted inside a text field's frame, for tests that pin the field's marks.
use gpui::{point, px, size, Bounds, Pixels, ScaledPixels, Window};

#[derive(Debug, Default)]
pub(crate) struct Marks {
    pub washes: Vec<Bounds<Pixels>>,
    pub carets: Vec<Bounds<Pixels>>,
    pub underlines: Vec<Bounds<Pixels>>,
    /// One device pixel in window pixels; painted edges snap to the nearest one.
    pub device: Pixels,
}

impl Marks {
    /// Whether a painted edge stands where a window-pixel edge lands once snapped.
    pub(crate) fn lands(&self, painted: Pixels, edge: Pixels) -> bool {
        (painted - edge).abs() <= self.device * 0.5
    }
}

/// Every quad inside `frame` but the frame's own: 1 px wide ones are carets, 1 px tall ones underlines, the rest washes.
pub(crate) fn marks(window: &Window, frame: Bounds<Pixels>) -> Marks {
    let scale = window.scale_factor();
    let unscale = |at: ScaledPixels| px(at.as_f32() / scale);
    let mut marks = Marks {
        device: px(1. / scale),
        ..Marks::default()
    };
    for quad in window.painted_quads() {
        let at = quad.bounds;
        let bounds = Bounds::new(
            point(unscale(at.origin.x), unscale(at.origin.y)),
            size(unscale(at.size.width), unscale(at.size.height)),
        );
        let slack = marks.device * 0.5;
        let own = marks.lands(bounds.left(), frame.left())
            && marks.lands(bounds.top(), frame.top())
            && marks.lands(bounds.right(), frame.right())
            && marks.lands(bounds.bottom(), frame.bottom());
        let inside = bounds.left() >= frame.left() - slack
            && bounds.top() >= frame.top() - slack
            && bounds.right() <= frame.right() + slack
            && bounds.bottom() <= frame.bottom() + slack;
        if own || !inside {
            continue;
        }
        if bounds.size.width == px(1.) {
            marks.carets.push(bounds);
        } else if bounds.size.height == px(1.) {
            marks.underlines.push(bounds);
        } else {
            marks.washes.push(bounds);
        }
    }
    marks
}
