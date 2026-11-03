use super::draw::StandIn;
use super::draw_flash_tests::LiveFlash;
use crate::list::bench::real_cx;
use gpui::{point, px, size, Modifiers, MouseButton, VisualTestContext};

#[test]
fn r7_dragged_outside_frame_returns_to_rest() {
    let mut cx = real_cx();
    cx.update(|cx| cx.set_reduce_motion(true));
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
        LiveFlash::live_flash_action()
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").unwrap();
    let outside = point(px(20.), px(20.));
    let resting_frame = visual.debug_bounds("op2-flash-frame").unwrap();
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert_ne!(
        visual.debug_bounds("op2-flash-frame").unwrap(),
        resting_frame
    );
    visual.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none());
    assert_eq!(
        visual.debug_bounds("op2-flash-frame").unwrap(),
        resting_frame,
        "DialogButton pressShown false restores the frame as well as removing the wash"
    );
}

#[test]
fn r7_release_outside_window_clears_the_press() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
        LiveFlash::live_flash_action()
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").unwrap();
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    let outside = point(px(-10.), px(-10.));
    visual.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                !live.flash.view(&live.i18n).step.action_pressed,
                "a release outside the window must clear the action's press"
            );
        })
        .unwrap();
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "returning with no held button must not draw a press wash"
    );
}

#[test]
fn r7_a_left_release_after_an_outside_release_does_not_flash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::two_fitting());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let first = visual.debug_bounds("op2-flash-drive-0").unwrap();
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").unwrap();
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    let outside = point(px(-10.), px(-10.));
    visual.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    let image = visual.debug_bounds("op2-flash-image-row").unwrap();
    visual.simulate_mouse_down(image.center(), MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(action.center(), Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                live.sends().is_empty(),
                "a release with no new action press must not flash: {:?}",
                live.sends()
            );
        })
        .unwrap();
}

#[test]
fn r7_the_step_frame_keeps_the_live_radius() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
        LiveFlash::live_flash_action()
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let frame = visual.debug_bounds("op2-flash-frame").unwrap();
    window
        .update(&mut cx, |_, window, _| {
            let scaled = frame.scale(window.scale_factor());
            let quads = window.painted_quads();
            let painted = quads
                .iter()
                .find(|q| q.bounds == scaled && q.border_widths.top.0 > 0.)
                .unwrap();
            assert_eq!(
                painted.corner_radii.top_left.0,
                8. * window.scale_factor(),
                "the Flash frame must paint the live radius, not just name it in a root selector"
            );
        })
        .unwrap();
}

#[test]
fn r7_the_press_animation_starts_at_the_resting_margin() {
    assert_eq!(
        StandIn::press_margin(100., 0.),
        0.,
        "at animation progress zero the chrome must still have its resting margin"
    );
}
