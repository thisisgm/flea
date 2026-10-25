//! The unlock card grows with the text size, as the other dialog cards do (#310).
use super::view_quad_tests::{card, close, frames, paint};
use gpui::{px, TestAppContext};

#[gpui::test]
fn the_card_grows_with_the_text_size_like_the_other_dialog_cards(cx: &mut TestAppContext) {
    let (_view, cx, d) = card(cx, None);
    let mut heights = Vec::new();
    for rem in [14., 21.] {
        cx.update(|window, _| window.set_rem_size(px(rem)));
        let (unlock, _) = frames(&paint(cx), &d);
        heights.push(unlock.size.height.0 / d.scale);
    }
    // The board's 30 px button at its 14 px body, and half again at a 21 px body (#310).
    assert!(
        close(heights[0], 30.),
        "button height at the board body: {}",
        heights[0]
    );
    assert!(
        close(heights[1], 45.),
        "button height at a 21 px body: {}",
        heights[1]
    );
}
