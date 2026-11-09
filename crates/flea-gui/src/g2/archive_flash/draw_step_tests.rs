//! The FLASH step action's tests: press, wash, frame and chrome through the live harness.
use super::draw::StandIn;
use super::draw_flash_tests::LiveFlash;
use super::draw_step::ActionFrame;
use super::flash::{StepAction, StepMark, StepRow, SEL_IMAGE_ROW};
use crate::list::bench::real_cx;
use gpui::{px, size, Modifiers, MouseButton, VisualTestContext};

fn framed_step(live: bool, cursor: bool, end_primary: bool, hover: bool, pressed: bool) -> StepRow {
    StepRow {
        mark: StepMark::Alert,
        line: String::new(),
        action: "Flash".into(),
        live,
        resting: false,
        running: false,
        hint: String::new(),
        cursor,
        ejected: false,
        end_label: "Flash again".into(),
        end_primary,
        end_visible: true,
        action_hover: hover,
        action_pressed: pressed,
        cancel_live: true,
        share: 0.0,
        byte: Vec::new(),
    }
}

#[test]
fn the_step_frame_ignores_hover_and_press() {
    for hover in [false, true] {
        for pressed in [false, true] {
            for focused in [false, true] {
                let dim = framed_step(false, false, false, hover, pressed);
                assert_eq!(
                    StandIn::action_frame(&dim, StepAction::Flash, focused),
                    ActionFrame::Muted,
                    "a dimmed Flash keeps its muted frame"
                );
                let live = framed_step(true, true, false, hover, pressed);
                let live_want = if focused {
                    ActionFrame::Foreground
                } else {
                    ActionFrame::Accent
                };
                assert_eq!(
                    StandIn::action_frame(&live, StepAction::Flash, focused),
                    live_want,
                    "a live Flash swaps accent for foreground under focus"
                );
                let unset = framed_step(false, false, false, hover, pressed);
                assert_eq!(
                    StandIn::action_frame(&unset, StepAction::Cancel, focused),
                    ActionFrame::Muted,
                    "an unset Cancel keeps its muted frame"
                );
                let set = framed_step(false, true, false, hover, pressed);
                let set_want = if focused {
                    ActionFrame::Foreground
                } else {
                    ActionFrame::Muted
                };
                assert_eq!(
                    StandIn::action_frame(&set, StepAction::Cancel, focused),
                    set_want,
                    "a set Cancel swaps muted for foreground under focus"
                );
                let plain = framed_step(false, false, false, hover, pressed);
                assert_eq!(
                    StandIn::action_frame(&plain, StepAction::End, focused),
                    ActionFrame::Muted,
                    "a plain End keeps its muted frame"
                );
                let primary = framed_step(true, true, true, hover, pressed);
                let primary_want = if focused {
                    ActionFrame::Foreground
                } else {
                    ActionFrame::Accent
                };
                assert_eq!(
                    StandIn::action_frame(&primary, StepAction::End, focused),
                    primary_want,
                    "a primary End swaps accent for foreground under focus"
                );
            }
        }
    }
}

#[test]
fn the_step_action_wears_the_oracles_focus_frame() {
    // DialogButton.qml:41-46: focused and available draws the focus frame, else an available primary accentFrame, else mutedFrame.
    let mut wrong = Vec::new();
    let focused_live_flash = framed_step(true, true, false, false, false);
    if StandIn::action_frame(&focused_live_flash, StepAction::Flash, true)
        != ActionFrame::Foreground
    {
        wrong.push(format!(
            "focused live Flash {:?}",
            StandIn::action_frame(&focused_live_flash, StepAction::Flash, true)
        ));
    }
    let focused_flash_again = framed_step(true, true, true, false, false);
    if StandIn::action_frame(&focused_flash_again, StepAction::End, true) != ActionFrame::Foreground
    {
        wrong.push(format!(
            "focused Flash again End {:?}",
            StandIn::action_frame(&focused_flash_again, StepAction::End, true)
        ));
    }
    let unfocused_eject = framed_step(true, false, false, false, false);
    if StandIn::action_frame(&unfocused_eject, StepAction::End, false) != ActionFrame::Muted {
        wrong.push(format!(
            "unfocused Eject {:?}",
            StandIn::action_frame(&unfocused_eject, StepAction::End, false)
        ));
    }
    let mut flush_cancel = framed_step(false, true, false, false, false);
    flush_cancel.running = true;
    flush_cancel.cancel_live = false;
    if StandIn::action_frame(&flush_cancel, StepAction::Cancel, true) != ActionFrame::Muted {
        wrong.push(format!(
            "focused dimmed flush Cancel {:?}",
            StandIn::action_frame(&flush_cancel, StepAction::Cancel, true)
        ));
    }
    assert!(
        wrong.is_empty(),
        "frames that differ from v0.3.12: {}",
        wrong.join("; ")
    );
}

#[test]
fn hover_and_press_lay_the_live_cancel_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::live_cancel());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let cancel = visual
        .debug_bounds("op2-flash-cancel")
        .expect("running draws Cancel, not Flash");
    visual.simulate_mouse_move(cancel.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-hover").is_some(),
        "a live Cancel hover lays the 0.08 ink wash"
    );
    visual.simulate_mouse_down(cancel.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "a live Cancel press lays the 0.14 ink wash"
    );
    visual.simulate_mouse_up(cancel.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "a release lifts the Cancel press wash"
    );
}

#[test]
fn hover_and_press_lay_the_failed_end_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::failed_end());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let end = visual
        .debug_bounds("op2-flash-end")
        .expect("a failure draws its end button");
    visual.simulate_mouse_move(end.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-hover").is_some(),
        "an available End hover lays the 0.08 ink wash"
    );
    visual.simulate_mouse_down(end.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "an available End press lays the 0.14 ink wash"
    );
    visual.simulate_mouse_up(end.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "a release lifts the End press wash"
    );
}

#[test]
fn a_press_held_outside_the_action_draws_the_resting_control() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::two_fitting());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let image = visual
        .debug_bounds(SEL_IMAGE_ROW)
        .expect("the image row carries its selector");
    let first = visual
        .debug_bounds("op2-flash-drive-0")
        .expect("drive rows carry their selectors");
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(live.flash.live(), "the chosen drive keeps the action live")
        })
        .unwrap();
    let action = visual
        .debug_bounds("op2-flash-action")
        .expect("rest draws the Flash action");
    let rest = action;
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    let hovering = visual
        .debug_bounds("op2-flash-wash-hover")
        .expect("hover lays the 0.08 ink wash");
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    super::draw_motion_tests::advance(&mut visual, 150);
    let pressing = visual
        .debug_bounds("op2-flash-wash-press")
        .expect("press lays the 0.14 ink wash");
    assert!(
        visual
            .debug_bounds("op2-flash-action")
            .expect("press keeps the action")
            .size
            == rest.size,
        "press moves nothing"
    );
    assert!(
        pressing.origin.x > hovering.origin.x,
        "the press wash starts a whole pixel deeper"
    );
    assert!(
        pressing.origin.y > hovering.origin.y,
        "the press wash starts a whole pixel lower"
    );
    assert!(
        pressing.origin.x + pressing.size.width < hovering.origin.x + hovering.size.width,
        "the press wash ends a whole pixel sooner"
    );
    assert!(
        pressing.origin.y + pressing.size.height < hovering.origin.y + hovering.size.height,
        "the press wash ends a whole pixel higher"
    );
    visual.simulate_mouse_move(image.center(), Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "a press held outside lays no press wash"
    );
    assert!(
        visual.debug_bounds("op2-flash-wash-hover").is_none(),
        "a press held outside lays no hover wash"
    );
    let action = visual
        .debug_bounds("op2-flash-action")
        .expect("the resting control stays drawn");
    assert!(
        action.size == rest.size,
        "a press held outside moves nothing"
    );
    visual.simulate_mouse_up(image.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                !live.flash.view(&live.i18n).step.action_pressed,
                "a release outside clears the press"
            );
            assert!(
                live.sends().is_empty(),
                "a press released outside starts no flash"
            );
        })
        .unwrap();
}

#[path = "draw_step_press_tests.rs"]
mod press;
