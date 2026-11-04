//! Reported centre pixels must hit both the real viewport and its rounded IPC rectangle.
use super::centre_inside;
use gpui::{point, px, size, Bounds};

#[test]
fn f7_fractional_centres_keep_reported_pixels_inside_the_viewport() {
    let area = Bounds::new(point(px(0.), px(54.4)), size(px(100.), px(100.)));
    assert!(!centre_inside(area, (0., 42.2, 100., 24.)));
    assert!(!centre_inside(area, (0., 42., 100., 24.)));
    assert!(centre_inside(area, (0., 43., 100., 24.)));
    let narrow_end = Bounds::new(point(px(0.), px(54.)), size(px(100.), px(100.4)));
    assert!(!centre_inside(narrow_end, (0., 142., 100., 24.)));
}
