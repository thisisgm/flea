//! Oracle drag-distance boundaries and gesture renewal through actual pointer events.
use super::u07_motion_tests::{control, down, open};
use crate::{columns::preview_host::START_DRAG_DISTANCE, path::mark::press_scale};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[gpui::test]
fn u07_taps_cancel_only_beyond_either_axis_of_the_original_platform_distance(
    cx: &mut TestAppContext,
) {
    let (chrome, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let at = control(&chrome, cx).bounds.unwrap().center();
    let threshold = START_DRAG_DISTANCE;
    assert_eq!(threshold, 10., "the existing Qt platform-distance contract");
    for (x, y, active) in [
        (threshold - 0.25, 0., true),
        (threshold, 0., true),
        (-threshold, 0., true),
        (threshold + 0.25, 0., false),
        (-threshold - 0.25, 0., false),
        (0., threshold, true),
        (0., -threshold, true),
        (0., threshold + 0.25, false),
        (0., -threshold - 0.25, false),
        (threshold, threshold, true),
    ] {
        let count = seen.borrow().len();
        let moved = at + point(px(x), px(y));
        assert!(control(&chrome, cx).bounds.unwrap().contains(&moved));
        cx.simulate_mouse_move(at, None, Modifiers::none());
        down(cx, at);
        assert_eq!(control(&chrome, cx).press_origin, Some(at));
        cx.executor().advance_clock(Duration::from_millis(20));
        let now = cx.executor().now();
        let before = press_scale(control(&chrome, cx).press, now, false).0;
        cx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::none());
        cx.run_until_parked();
        let moved_control = control(&chrome, cx);
        assert_eq!(moved_control.pointer_down, active, "({x}, {y})");
        assert_eq!(moved_control.pressed, active, "({x}, {y})");
        assert!((press_scale(moved_control.press, now, false).0 - before).abs() < 0.000001);
        if !active {
            cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
            cx.run_until_parked();
            assert!(!control(&chrome, cx).pointer_down && !control(&chrome, cx).pressed);
        }
        cx.simulate_mouse_up(
            if active { moved } else { at },
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert_eq!(
            seen.borrow().len(),
            count + usize::from(active),
            "({x}, {y})"
        );
        assert!(!control(&chrome, cx).pointer_down);
        assert_eq!(control(&chrome, cx).press_origin, None);
    }
    assert!(seen.borrow().iter().all(|action| *action == "search"));
}

#[gpui::test]
fn u07_drag_distance_uses_the_actual_down_position_and_never_resets_on_moves(
    cx: &mut TestAppContext,
) {
    let (chrome, cx) = open(cx);
    let centre = control(&chrome, cx).bounds.unwrap().center();
    let origin = centre - point(px(8.), px(0.));
    cx.simulate_mouse_move(origin, None, Modifiers::none());
    down(cx, origin);
    assert_eq!(control(&chrome, cx).press_origin, Some(origin));
    for step in 1..=11 {
        let moved = origin + point(px(step as f32), px(0.));
        assert!(control(&chrome, cx).bounds.unwrap().contains(&moved));
        cx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(control(&chrome, cx).pointer_down, step <= 10, "step {step}");
    }
    cx.simulate_mouse_move(origin, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    assert!(!control(&chrome, cx).pressed);
    cx.simulate_mouse_up(origin, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn u07_leaving_bounds_cancels_even_below_threshold_and_a_fresh_press_can_activate(
    cx: &mut TestAppContext,
) {
    let (chrome, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let bounds = control(&chrome, cx).bounds.unwrap();
    let origin = point(bounds.left() + px(1.), bounds.center().y);
    let outside = origin - point(px(2.), px(0.));
    assert!(!bounds.contains(&outside));
    for return_inside in [true, false] {
        cx.simulate_mouse_move(origin, None, Modifiers::none());
        down(cx, origin);
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
        cx.run_until_parked();
        assert!(!control(&chrome, cx).pointer_down && !control(&chrome, cx).pressed);
        let release = if return_inside { origin } else { outside };
        cx.simulate_mouse_move(release, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(release, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert!(seen.borrow().is_empty());
    }
    cx.simulate_mouse_move(origin, None, Modifiers::none());
    let now = cx.executor().now();
    let rebound = press_scale(control(&chrome, cx).press, now, false).0;
    down(cx, origin);
    assert!(control(&chrome, cx).pointer_down && control(&chrome, cx).pressed);
    assert!((press_scale(control(&chrome, cx).press, now, false).0 - rebound).abs() < 0.000001);
    cx.simulate_mouse_up(origin, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(seen.borrow().as_slice(), ["search"]);
}

#[gpui::test]
fn u07_release_position_checks_threshold_even_without_a_preceding_move(cx: &mut TestAppContext) {
    let (chrome, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let at = control(&chrome, cx).bounds.unwrap().center();
    down(cx, at);
    let release = at + point(px(START_DRAG_DISTANCE + 0.25), px(0.));
    assert!(control(&chrome, cx).bounds.unwrap().contains(&release));
    cx.simulate_mouse_up(release, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(seen.borrow().is_empty());
    assert!(!control(&chrome, cx).pointer_down && !control(&chrome, cx).pressed);
}
