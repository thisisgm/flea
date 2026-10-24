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
                assert_eq!(StandIn::action_frame(&dim, StepAction::Flash, focused), ActionFrame::Muted, "a dimmed Flash keeps its muted frame");
                let live = framed_step(true, true, false, hover, pressed);
                let live_want = if focused { ActionFrame::Foreground } else { ActionFrame::Accent };
                assert_eq!(StandIn::action_frame(&live, StepAction::Flash, focused), live_want, "a live Flash swaps accent for foreground under focus");
                let unset = framed_step(false, false, false, hover, pressed);
                assert_eq!(StandIn::action_frame(&unset, StepAction::Cancel, focused), ActionFrame::Muted, "an unset Cancel keeps its muted frame");
                let set = framed_step(false, true, false, hover, pressed);
                let set_want = if focused { ActionFrame::Foreground } else { ActionFrame::Muted };
                assert_eq!(StandIn::action_frame(&set, StepAction::Cancel, focused), set_want, "a set Cancel swaps muted for foreground under focus");
                let plain = framed_step(false, false, false, hover, pressed);
                assert_eq!(StandIn::action_frame(&plain, StepAction::End, focused), ActionFrame::Muted, "a plain End keeps its muted frame");
                let primary = framed_step(true, true, true, hover, pressed);
                let primary_want = if focused { ActionFrame::Foreground } else { ActionFrame::Accent };
                assert_eq!(StandIn::action_frame(&primary, StepAction::End, focused), primary_want, "a primary End swaps accent for foreground under focus");
            }
        }
    }
}

#[test]
fn the_step_action_wears_the_oracles_focus_frame() {
    // DialogButton.qml:41-46: focused and available draws the focus frame, else an available primary accentFrame, else mutedFrame.
    let mut wrong = Vec::new();
    let focused_live_flash = framed_step(true, true, false, false, false);
    if StandIn::action_frame(&focused_live_flash, StepAction::Flash, true) != ActionFrame::Foreground {
        wrong.push(format!("focused live Flash {:?}", StandIn::action_frame(&focused_live_flash, StepAction::Flash, true)));
    }
    let focused_flash_again = framed_step(true, true, true, false, false);
    if StandIn::action_frame(&focused_flash_again, StepAction::End, true) != ActionFrame::Foreground {
        wrong.push(format!("focused Flash again End {:?}", StandIn::action_frame(&focused_flash_again, StepAction::End, true)));
    }
    let unfocused_eject = framed_step(true, false, false, false, false);
    if StandIn::action_frame(&unfocused_eject, StepAction::End, false) != ActionFrame::Muted {
        wrong.push(format!("unfocused Eject {:?}", StandIn::action_frame(&unfocused_eject, StepAction::End, false)));
    }
    let mut flush_cancel = framed_step(false, true, false, false, false);
    flush_cancel.running = true;
    flush_cancel.cancel_live = false;
    if StandIn::action_frame(&flush_cancel, StepAction::Cancel, true) != ActionFrame::Muted {
        wrong.push(format!("focused dimmed flush Cancel {:?}", StandIn::action_frame(&flush_cancel, StepAction::Cancel, true)));
    }
    assert!(wrong.is_empty(), "frames that differ from v0.3.12: {}", wrong.join("; "));
}

#[test]
fn hover_and_press_lay_the_live_cancel_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::live_cancel());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let cancel = visual.debug_bounds("op2-flash-cancel").expect("running draws Cancel, not Flash");
    visual.simulate_mouse_move(cancel.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-hover").is_some(), "a live Cancel hover lays the 0.08 ink wash");
    visual.simulate_mouse_down(cancel.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_some(), "a live Cancel press lays the 0.14 ink wash");
    visual.simulate_mouse_up(cancel.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a release lifts the Cancel press wash");
}

#[test]
fn hover_and_press_lay_the_failed_end_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::failed_end());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let end = visual.debug_bounds("op2-flash-end").expect("a failure draws its end button");
    visual.simulate_mouse_move(end.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-hover").is_some(), "an available End hover lays the 0.08 ink wash");
    visual.simulate_mouse_down(end.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_some(), "an available End press lays the 0.14 ink wash");
    visual.simulate_mouse_up(end.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a release lifts the End press wash");
}

#[test]
fn a_press_held_outside_the_action_draws_the_resting_control() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::two_fitting());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let image = visual.debug_bounds(SEL_IMAGE_ROW).expect("the image row carries its selector");
    let first = visual.debug_bounds("op2-flash-drive-0").expect("drive rows carry their selectors");
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    window.update(&mut cx, |live, _, _| assert!(live.flash.live(), "the chosen drive keeps the action live")).unwrap();
    let action = visual.debug_bounds("op2-flash-action").expect("rest draws the Flash action");
    let rest = action;
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    let hovering = visual.debug_bounds("op2-flash-wash-hover").expect("hover lays the 0.08 ink wash");
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    super::draw_motion_tests::advance(&mut visual, 150);
    let pressing = visual.debug_bounds("op2-flash-wash-press").expect("press lays the 0.14 ink wash");
    assert!(visual.debug_bounds("op2-flash-action").expect("press keeps the action").size == rest.size, "press moves nothing");
    assert!(pressing.origin.x > hovering.origin.x, "the press wash starts a whole pixel deeper");
    assert!(pressing.origin.y > hovering.origin.y, "the press wash starts a whole pixel lower");
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
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a press held outside lays no press wash");
    assert!(visual.debug_bounds("op2-flash-wash-hover").is_none(), "a press held outside lays no hover wash");
    let action = visual.debug_bounds("op2-flash-action").expect("the resting control stays drawn");
    assert!(action.size == rest.size, "a press held outside moves nothing");
    visual.simulate_mouse_up(image.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(!live.flash.view(&live.i18n).step.action_pressed, "a release outside clears the press");
            assert!(live.sends().is_empty(), "a press released outside starts no flash");
        })
        .unwrap();
}

#[test]
fn the_press_margin_is_the_oracles_rounded_two_percent() {
    // DialogButton.qml:78 insetX: round(box.width * (1 - PRESS_SCALE) / 2) with PRESS_SCALE 0.96.
    assert_eq!(StandIn::press_margin(100., 1.), 2., "the board-wide Flash 128GB earns the two-pixel margin");
    assert_eq!(StandIn::press_margin(30., 1.), 1., "the thirty-pixel action earns one pixel top and bottom");
    assert_eq!(StandIn::press_margin(100., 0.), 0., "rest draws no margin");
}

#[test]
fn the_press_chrome_insets_frame_and_wash_by_the_oracles_rounded_margins() {
    for live in [true, false] {
        let mut cx = real_cx();
        cx.update(|cx| cx.set_reduce_motion(true));
        let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
            if live { LiveFlash::live_flash_action() } else { LiveFlash::live_cancel() }
        });
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        visual.run_until_parked();
        let action = if live {
            visual.debug_bounds("op2-flash-action").expect("the live step draws its Flash action")
        } else {
            visual.debug_bounds("op2-flash-cancel").expect("running draws Cancel")
        };
        visual.simulate_mouse_move(action.center(), None, Modifiers::none());
        visual.run_until_parked();
        visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        let board_w = f32::from(action.size.width);
        let board_h = f32::from(action.size.height);
        let inset_x = StandIn::press_margin(board_w, 1.);
        let inset_y = StandIn::press_margin(board_h, 1.);
        // The live Flash is primary, and this theme draws accent_frame equal to foreground, so it wears the 2-pixel ring; the secondary Cancel wears a hairline frame.
        let ring = if live { 2. } else { 1. };
        let press = visual.debug_bounds("op2-flash-wash-press").expect("press lays the wash");
        assert_eq!(f32::from(press.size.width), board_w - 2. * (inset_x + ring), "the wash sits a frame width inside the frame");
        assert_eq!(
            f32::from(press.size.height),
            board_h - 2. * (inset_y + ring),
            "the wash sits a frame width inside the frame"
        );
        let frame = visual.debug_bounds("op2-flash-frame").expect("press draws its own frame");
        assert_eq!(f32::from(frame.size.width), board_w - 2. * inset_x, "the frame gives a rounded margin each side");
        assert_eq!(f32::from(frame.size.height), board_h - 2. * inset_y, "the frame gives a rounded margin top and bottom");
        assert_eq!(f32::from(frame.origin.x - action.origin.x), inset_x, "the pressed frame leaves the resting edge");
        assert_eq!(f32::from(press.origin.x - frame.origin.x), ring, "the wash starts a frame width inside the frame");
        assert_eq!(f32::from(press.origin.y - frame.origin.y), ring, "the wash starts a frame width lower than the frame");
        visual.simulate_mouse_up(action.center(), MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a release lifts the press wash");
    }
}

#[test]
fn a_press_moved_inside_the_action_keeps_the_press_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::live_flash_action());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").expect("the live step draws its Flash action");
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_some(), "a press lays the 0.14 ink wash");
    let inside = action.center() + gpui::point(px(3.), px(0.));
    assert!(action.contains(&inside), "the moved point is still inside the action");
    visual.simulate_mouse_move(inside, Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "DialogButton.qml:32 pressShown: a press held inside the button keeps the press wash"
    );
    visual.simulate_mouse_up(inside, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
}

#[test]
fn a_press_dragged_out_and_back_in_regains_the_press_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::two_fitting());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let first = visual.debug_bounds("op2-flash-drive-0").expect("drive rows carry their selectors");
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").expect("the live step draws its Flash action");
    let image = visual.debug_bounds(SEL_IMAGE_ROW).expect("the image row carries its selector");
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_some(), "a press lays the 0.14 ink wash");
    visual.simulate_mouse_move(image.center(), Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a press dragged outside lays no press wash");
    let back = action.center() + gpui::point(px(-3.), px(0.));
    assert!(action.contains(&back), "the return point is still inside the action");
    visual.simulate_mouse_move(back, Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "DialogButton.qml:32 pressShown: a press dragged back inside regains the press wash"
    );
    visual.simulate_mouse_up(back, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                live.sends().iter().any(|line| line.contains(r#""c":"flash""#)),
                "a release inside the action after a drag still flashes"
            );
        })
        .unwrap();
}
