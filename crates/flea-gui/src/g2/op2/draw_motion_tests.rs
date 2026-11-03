use super::draw_flash_tests::LiveFlash;
use crate::list::bench::real_cx;
use gpui::{point, px, size, Bounds, Modifiers, MouseButton, Pixels, VisualTestContext};
use std::time::Duration;

pub(super) fn advance(visual: &mut VisualTestContext, ms: u64) {
    visual.executor().advance_clock(Duration::from_millis(ms));
    visual.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    visual.run_until_parked();
}

/// The pressed chrome's margin a side (Buttons.js PRESS_SCALE 0.96), the press length and the first sample.
const PRESS_MARGIN: f32 = 0.02;
const PRESS_MS: u64 = 150;
const STEP_MS: u64 = 15;

fn frame(visual: &mut VisualTestContext) -> Bounds<Pixels> {
    visual
        .debug_bounds("op2-flash-frame")
        .expect("the frame is drawn")
}

#[test]
fn r7_an_animated_press_has_initial_intermediate_and_end_bounds() {
    for reduced in [false, true] {
        let mut cx = real_cx();
        cx.update(|cx| cx.set_reduce_motion(reduced));
        let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
            LiveFlash::live_flash_action()
        });
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        visual.run_until_parked();
        let action = visual.debug_bounds("op2-flash-action").unwrap();
        let rest = frame(&mut visual);
        visual.simulate_mouse_move(action.center(), None, Modifiers::none());
        visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        let end_x = (f32::from(action.size.width) * PRESS_MARGIN).round();
        let end_y = (f32::from(action.size.height) * PRESS_MARGIN).round();
        assert_eq!(
            f32::from(frame(&mut visual).origin.x - rest.origin.x),
            if reduced { end_x } else { 0. },
            "a new press starts at rest unless reduced motion requests the end state"
        );
        advance(&mut visual, STEP_MS);
        let eased = 1.0_f32 - (1.0_f32 - STEP_MS as f32 / PRESS_MS as f32).powi(3);
        let sample = frame(&mut visual);
        assert_eq!(
            f32::from(sample.origin.x - rest.origin.x),
            if reduced {
                end_x
            } else {
                (end_x * eased).round()
            },
            "the intermediate horizontal margin follows OutCubic"
        );
        assert_eq!(
            f32::from(sample.origin.y - rest.origin.y),
            if reduced {
                end_y
            } else {
                (end_y * eased).round()
            },
            "the intermediate vertical margin follows OutCubic"
        );
        advance(&mut visual, PRESS_MS - STEP_MS);
        let end = frame(&mut visual);
        assert_eq!(
            f32::from(end.origin.x - rest.origin.x),
            end_x,
            "150 ms reaches the rounded horizontal end state"
        );
        assert_eq!(
            f32::from(end.origin.y - rest.origin.y),
            end_y,
            "150 ms reaches the rounded vertical end state"
        );
        assert_eq!(end.size.width, rest.size.width - px(2. * end_x));
        assert_eq!(end.size.height, rest.size.height - px(2. * end_y));
    }
}

#[test]
fn r7_exit_and_return_animate_the_frame_with_both_motion_settings() {
    for reduced in [false, true] {
        let mut cx = real_cx();
        cx.update(|cx| cx.set_reduce_motion(reduced));
        let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
            LiveFlash::live_flash_action()
        });
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        visual.run_until_parked();
        let action = visual.debug_bounds("op2-flash-action").unwrap();
        let rest = frame(&mut visual);
        visual.simulate_mouse_move(action.center(), None, Modifiers::none());
        visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
        advance(&mut visual, PRESS_MS);
        let shrunk = frame(&mut visual);
        assert_ne!(shrunk, rest, "a held press reaches its inset");
        visual.simulate_mouse_move(
            point(px(20.), px(20.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        visual.run_until_parked();
        assert!(visual.debug_bounds("op2-flash-wash-press").is_none());
        if reduced {
            assert_eq!(
                frame(&mut visual),
                rest,
                "reduced motion restores the frame immediately on exit"
            );
        } else {
            assert_eq!(
                frame(&mut visual),
                shrunk,
                "an exit starts the reverse animation at the previous frame"
            );
        }
        advance(&mut visual, PRESS_MS);
        assert_eq!(
            frame(&mut visual),
            rest,
            "pressShown false restores the frame after the animation"
        );
        visual.simulate_mouse_move(action.center(), Some(MouseButton::Left), Modifiers::none());
        advance(&mut visual, PRESS_MS);
        assert_eq!(
            frame(&mut visual),
            shrunk,
            "returning during the same press shrinks the frame again"
        );
        assert!(visual.debug_bounds("op2-flash-wash-press").is_some());
        visual.simulate_mouse_up(
            point(px(20.), px(20.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        advance(&mut visual, PRESS_MS);
        assert_eq!(
            frame(&mut visual),
            rest,
            "release outside leaves the resting frame"
        );
    }
}

#[test]
fn r7_capture_loss_cancels_press_ownership() {
    for loss in ["blur", "pointer", "escape"] {
        let mut cx = real_cx();
        let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::two_fitting());
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        visual.update(|window, _| window.activate_window());
        visual.run_until_parked();
        assert!(
            visual.update(|window, _| window.is_window_active()),
            "the blur fixture starts active"
        );
        let drive = visual.debug_bounds("op2-flash-drive-0").unwrap();
        visual.simulate_click(drive.center(), Modifiers::none());
        let action = visual.debug_bounds("op2-flash-action").unwrap();
        visual.simulate_mouse_move(action.center(), None, Modifiers::none());
        visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        match loss {
            "blur" => visual.deactivate_window(),
            "escape" => visual.simulate_keystrokes("escape"),
            _ => visual.simulate_mouse_move(action.center(), None, Modifiers::none()),
        }
        window
            .update(&mut cx, |live, _, _| {
                assert!(
                    !live.flash.view(&live.i18n).step.action_pressed,
                    "capture loss clears the model's paint state ({loss})"
                );
            })
            .unwrap();
        visual.simulate_mouse_up(action.center(), MouseButton::Left, Modifiers::none());
        window
            .update(&mut cx, |live, _, _| {
                assert!(
                    live.sends().is_empty(),
                    "a release after capture loss never flashes"
                );
            })
            .unwrap();
    }
}
