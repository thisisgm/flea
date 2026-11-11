//! Flash pointer actions through the rendered card.
use super::*;

#[test]
fn clicks_on_flash_rows_reach_the_oracles_entries() {
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
    let second = visual
        .debug_bounds("op2-flash-drive-1")
        .expect("drive rows carry their selectors");
    visual.simulate_click(image.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(
                live.picks(),
                1,
                "a left release on the image row asks for the picker"
            );
            assert!(
                live.flash.view(&live.i18n).image.cursor,
                "the cursor moves to the image row first"
            );
        })
        .unwrap();
    visual.simulate_click(second.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(
                live.flash.selected(),
                Some(1),
                "a left release on a drive row chooses it"
            );
        })
        .unwrap();
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    let action = visual
        .debug_bounds("op2-flash-action")
        .expect("rest draws the Flash action");
    assert!(
        visual.debug_bounds("op2-flash-wash-hover").is_none(),
        "rest lays no hover wash"
    );
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "rest lays no press wash"
    );
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(live.flash.view(&live.i18n).step.action_hover)
        })
        .unwrap();
    assert!(
        visual.debug_bounds("op2-flash-wash-hover").is_some(),
        "hover lays the 0.08 ink wash"
    );
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "hover lays no press wash"
    );
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(live.flash.view(&live.i18n).step.action_pressed)
        })
        .unwrap();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_some(),
        "press lays the 0.14 ink wash"
    );
    visual.simulate_mouse_up(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                !live.flash.view(&live.i18n).step.action_pressed,
                "a release clears the press"
            );
        })
        .unwrap();
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "a release lifts the press wash"
    );
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_up(image.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                !live.flash.view(&live.i18n).step.action_pressed,
                "a release outside the action clears the press"
            );
        })
        .unwrap();
    visual.simulate_click(action.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                live.sends()
                    .iter()
                    .any(|line| line.contains(r#""c":"flash""#) && line.contains(r#""dev":"sda""#)),
                "a left release on the action flashes the chosen drive"
            );
            live.flash
                .start_transfer(&live.i18n, 1, 4294967296, true, 0);
            assert!(
                live.flash.running(),
                "the backend answer starts the transfer"
            );
        })
        .unwrap();
    let second = visual
        .debug_bounds("op2-flash-drive-1")
        .expect("drive rows keep their selectors");
    visual.simulate_click(second.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(
                live.flash.selected(),
                Some(0),
                "no choice moves while a flash runs"
            );
        })
        .unwrap();
}

#[test]
fn a_click_on_the_dimmed_flush_cancel_in_a_real_window_sends_nothing() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::dimmed_cancel());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let cancel = visual
        .debug_bounds("op2-flash-cancel")
        .expect("the running step draws Cancel");
    visual.simulate_mouse_move(cancel.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("op2-flash-wash-hover").is_none(),
        "a dimmed hover lays no wash"
    );
    assert!(
        visual.debug_bounds("op2-flash-wash-press").is_none(),
        "a dimmed hover lays no press wash"
    );
    visual.simulate_click(cancel.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(
                live.step_taps, 0,
                "a click on the dimmed Cancel reaches no tap"
            );
            assert!(
                live.sends()
                    .iter()
                    .all(|line| !line.contains("flashcancel")),
                "a click on the dimmed Cancel sends no flashcancel"
            );
        })
        .unwrap();
}

#[test]
fn a_click_on_the_live_flash_action_reaches_only_its_tap() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| {
        LiveFlash::live_flash_action()
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let action = visual
        .debug_bounds("op2-flash-action")
        .expect("the live step draws its Flash action");
    visual.simulate_click(action.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(
                live.step_taps, 1,
                "a click on the live action reaches its tap once"
            );
            let sends = live.sends();
            assert!(
                matches!(sends.as_slice(), [line] if line.contains(r#""c":"flash""#)),
                "the one tap sends exactly one flash request: {sends:?}"
            );
        })
        .unwrap();
}
