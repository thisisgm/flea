//! Step press margins and pointer re-entry.
use super::*;

#[test]
fn the_press_margin_is_the_oracles_rounded_two_percent() {
    // DialogButton.qml:78 insetX: round(box.width * (1 - PRESS_SCALE) / 2) with PRESS_SCALE 0.96.
    assert_eq!(
        StandIn::press_margin(100., 1.),
        2.,
        "the board-wide Flash 128GB earns the two-pixel margin"
    );
    assert_eq!(
        StandIn::press_margin(30., 1.),
        1.,
        "the thirty-pixel action earns one pixel top and bottom"
    );
    assert_eq!(StandIn::press_margin(100., 0.), 0., "rest draws no margin");
}

#[test]
fn the_press_chrome_insets_frame_and_wash_by_the_oracles_rounded_margins() {
    for live in [true, false] {
        let mut cx = real_cx();
        cx.update(|cx| cx.set_reduce_motion(true));
        let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
            if live {
                LiveFlash::live_flash_action()
            } else {
                LiveFlash::live_cancel()
            }
        });
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        visual.run_until_parked();
        let action = if live {
            visual
                .debug_bounds("op2-flash-action")
                .expect("the live step draws its Flash action")
        } else {
            visual
                .debug_bounds("op2-flash-cancel")
                .expect("running draws Cancel")
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
        let press = visual
            .debug_bounds("op2-flash-wash-press")
            .expect("press lays the wash");
        assert_eq!(
            f32::from(press.size.width),
            board_w - 2. * (inset_x + ring),
            "the wash sits a frame width inside the frame"
        );
        assert_eq!(
            f32::from(press.size.height),
            board_h - 2. * (inset_y + ring),
            "the wash sits a frame width inside the frame"
        );
        let frame = visual
            .debug_bounds("op2-flash-frame")
            .expect("press draws its own frame");
        assert_eq!(
            f32::from(frame.size.width),
            board_w - 2. * inset_x,
            "the frame gives a rounded margin each side"
        );
        assert_eq!(
            f32::from(frame.size.height),
            board_h - 2. * inset_y,
            "the frame gives a rounded margin top and bottom"
        );
        assert_eq!(
            f32::from(frame.origin.x - action.origin.x),
            inset_x,
            "the pressed frame leaves the resting edge"
        );
        assert_eq!(
            f32::from(press.origin.x - frame.origin.x),
            ring,
            "the wash starts a frame width inside the frame"
        );
        assert_eq!(
            f32::from(press.origin.y - frame.origin.y),
            ring,
            "the wash starts a frame width lower than the frame"
        );
        visual.simulate_mouse_up(action.center(), MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        assert!(
            visual.debug_bounds("op2-flash-wash-press").is_none(),
            "a release lifts the press wash"
        );
    }
}

#[test]
fn a_press_moved_inside_the_action_keeps_the_press_wash() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
        LiveFlash::live_flash_action()
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let action = visual
        .debug_bounds("op2-flash-action")
        .expect("the live step draws its Flash action");
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "a press lays the 0.14 ink wash"
    );
    let inside = action.center() + gpui::point(px(3.), px(0.));
    assert!(
        action.contains(&inside),
        "the moved point is still inside the action"
    );
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
    let first = visual
        .debug_bounds("op2-flash-drive-0")
        .expect("drive rows carry their selectors");
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    let action = visual
        .debug_bounds("op2-flash-action")
        .expect("the live step draws its Flash action");
    let image = visual
        .debug_bounds(SEL_IMAGE_ROW)
        .expect("the image row carries its selector");
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "a press lays the 0.14 ink wash"
    );
    visual.simulate_mouse_move(image.center(), Some(MouseButton::Left), Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "a press dragged outside lays no press wash"
    );
    let back = action.center() + gpui::point(px(-3.), px(0.));
    assert!(
        action.contains(&back),
        "the return point is still inside the action"
    );
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
                live.sends()
                    .iter()
                    .any(|line| line.contains(r#""c":"flash""#)),
                "a release inside the action after a drag still flashes"
            );
        })
        .unwrap();
}
