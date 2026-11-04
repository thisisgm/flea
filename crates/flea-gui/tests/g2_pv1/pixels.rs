//! The board scale and the own-pixel checks the raster geometry tests share.
use super::boards::rect;
use gpui::VisualTestContext;

/// The boards are drawn one device pixel to a logical pixel, where a large raster's fit is the board's.
pub fn at_board_scale(cx: &mut VisualTestContext) {
    cx.simulate_scale_factor_change(1.);
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

/// A raster at its own pixels: `want` device pixels each way at the window's scale, never enlarged.
pub fn assert_own_pixels(cx: &mut VisualTestContext, selector: &'static str, want: (f32, f32)) {
    let scale = cx.update(|window, _| window.scale_factor());
    let (_, _, w, h) = rect(cx, selector);
    for (side, want) in [(w, want.0), (h, want.1)] {
        let pixels = side * scale;
        assert!(
            pixels <= want + 1e-3 && pixels > want - 0.01,
            "{selector} draws {pixels} device pixels for {want} at {scale}"
        );
    }
}

/// The picture's centre sits within a logical pixel of its room's centre.
pub fn assert_centred(cx: &mut VisualTestContext, selector: &'static str, room: &'static str) {
    let centre = |(x, y, w, h): (f32, f32, f32, f32)| (x + w / 2., y + h / 2.);
    let (picture, room) = (centre(rect(cx, selector)), centre(rect(cx, room)));
    assert!(
        (picture.0 - room.0).abs() <= 1. && (picture.1 - room.1).abs() <= 1.,
        "{selector} is centred: {picture:?} in {room:?}"
    );
}
