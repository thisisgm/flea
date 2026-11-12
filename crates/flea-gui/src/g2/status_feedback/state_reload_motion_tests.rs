//! A held press uses the same frame animation as the operations buttons.
use super::{
    hero_tests::english,
    state::{Facts, StateMessage},
};
use gpui::{px, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::time::Duration;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn mtp_critic_reload_press_starts_at_rest_and_animates_on_virtual_time(cx: &mut TestAppContext) {
    let catalog = english();
    let (_, cx) = cx.add_window_view(|window, cx| {
        window.set_rem_size(px(14.));
        let mut state = StateMessage::new();
        state.set(Some(Facts::waiting(&catalog)), cx);
        state
    });
    draw(cx);
    let button = cx.debug_bounds("fb1-reload").unwrap();
    let at = button.center();
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    draw(cx);
    let left = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            let scale = window.scale_factor();
            window
                .painted_quads()
                .iter()
                .filter(|quad| quad.border_widths.top.0 > 0.)
                .map(|quad| quad.bounds.origin.x.0 / scale)
                .find(|x| (*x - f32::from(button.left())).abs() < 3.)
                .unwrap()
        })
    };
    assert_eq!(
        left(cx),
        f32::from(button.left()),
        "F5: press starts before the animated inset"
    );
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_file_ops::card::PRESS_MS,
    ));
    draw(cx);
    assert_eq!(left(cx), f32::from(button.left()) + 1.);
}
