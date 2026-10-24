//! Pointer interruption and reduced-motion contracts through the live chrome handlers.
use crate::path::{
    chrome::{ChromeView, Control, Inputs},
    mark::{press_scale, Press},
    suggestions::Unranked,
    view::PathView,
};
use gpui::{
    point, px, AppContext, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

pub(super) fn open(cx: &mut TestAppContext) -> (Entity<ChromeView>, &mut VisualTestContext) {
    let (chrome, cx) = cx.add_window_view(|_, cx| {
        let path = cx.new(|cx| PathView::new(Default::default(), Box::new(Unranked), cx));
        ChromeView::new(
            path,
            Inputs {
                view: super::startup::View::List,
                back: false,
                up: true,
                filter: true,
                sort: true,
                live: true,
                corner: 0.,
            },
            cx,
        )
    });
    cx.simulate_resize(gpui::size(px(1100.), px(27.)));
    cx.run_until_parked();
    (chrome, cx)
}

pub(super) fn control(chrome: &Entity<ChromeView>, cx: &VisualTestContext) -> Control {
    chrome.read_with(cx, |view, _| view.frame.borrow().controls[2])
}

pub(super) fn down(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 1,
        modifiers: Modifiers::none(),
        first_mouse: false,
    });
    cx.run_until_parked();
}

#[test]
fn review_u07_reduced_motion_keeps_the_reference_unscaled_mark() {
    let now = std::time::Instant::now();
    assert_eq!(
        press_scale(
            Some(Press {
                down: true,
                since: now,
                from: 1.
            }),
            now,
            true
        ),
        (1., false),
        "ui/ChromeButton.qml:50 suppresses press scale under reduced motion"
    );
}

#[test]
fn review_u07_releasing_a_partial_press_keeps_scale_continuous() {
    let down = std::time::Instant::now();
    let release = down + Duration::from_millis(20);
    let mut control = Control::default();
    control.set_pressed(true, down, false);
    let (before, _) = press_scale(control.press, release, false);
    control.set_pressed(false, release, false);
    let (after, _) = press_scale(control.press, release, false);
    assert!(
        (before - after).abs() < 0.000001,
        "release must continue from the drawn scale: before={before}, after={after}"
    );
}

#[gpui::test]
fn u07_drag_off_back_and_outside_release_cannot_reactivate_a_canceled_tap(cx: &mut TestAppContext) {
    let (chrome, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let at = control(&chrome, cx).bounds.unwrap().center();
    let outside = point(px(2.), px(26.));
    cx.simulate_mouse_move(at, None, Modifiers::none());
    down(cx, at);
    cx.executor().advance_clock(Duration::from_millis(20));
    let now = cx.executor().now();
    let (before, _) = press_scale(control(&chrome, cx).press, now, false);
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    let off = control(&chrome, cx);
    assert!(!off.pointer_down && !off.pressed && !off.hovered);
    assert!((press_scale(off.press, now, false).0 - before).abs() < 0.000001);
    cx.executor().advance_clock(Duration::from_millis(10));
    let now = cx.executor().now();
    let (rebound, _) = press_scale(control(&chrome, cx).press, now, false);
    cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    let back = control(&chrome, cx);
    assert!(!back.pointer_down && !back.pressed && back.hovered);
    assert!((press_scale(back.press, now, false).0 - rebound).abs() < 0.000001);
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(seen.borrow().is_empty());
    down(cx, at);
    let up = chrome.read_with(cx, |view, _| {
        view.frame.borrow().controls[1].bounds.unwrap().center()
    });
    cx.simulate_mouse_move(up, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    assert!(!chrome.read_with(cx, |view, _| view.frame.borrow().controls[1].pressed));
    cx.simulate_mouse_up(up, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(!control(&chrome, cx).pointer_down);
    cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(!control(&chrome, cx).pressed);
    assert!(
        seen.borrow().is_empty(),
        "no activation after an outside release or a foreign press"
    );
}

#[gpui::test]
fn u07_disabling_or_hiding_a_held_control_retires_the_gesture(cx: &mut TestAppContext) {
    let (chrome, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let at = control(&chrome, cx).bounds.unwrap().center();
    cx.simulate_mouse_move(at, None, Modifiers::none());
    down(cx, at);
    cx.executor().advance_clock(Duration::from_millis(20));
    chrome.update(cx, |view, cx| {
        view.inputs.live = false;
        cx.notify();
    });
    cx.run_until_parked();
    let disabled = control(&chrome, cx);
    assert!(!disabled.hovered && !disabled.pressed && !disabled.pointer_down);
    chrome.update(cx, |view, cx| {
        view.inputs.live = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(control(&chrome, cx).hovered);
    assert!(!control(&chrome, cx).pressed);
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(seen.borrow().is_empty());
    down(cx, at);
    chrome.update(cx, |view, cx| {
        view.inputs.view = super::startup::View::Grid;
        cx.notify();
    });
    cx.run_until_parked();
    let hidden = control(&chrome, cx);
    assert!(!hidden.visible && !hidden.hovered && !hidden.pressed && !hidden.pointer_down);
    chrome.update(cx, |view, cx| {
        view.inputs.view = super::startup::View::List;
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(seen.borrow().is_empty());
}

#[gpui::test]
fn u07_reduced_motion_never_shrinks_a_live_press_or_release(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (chrome, cx) = open(cx);
    let at = control(&chrome, cx).bounds.unwrap().center();
    down(cx, at);
    for elapsed in [0, 20, 150, 1000] {
        cx.executor().advance_clock(Duration::from_millis(elapsed));
        assert_eq!(
            press_scale(control(&chrome, cx).press, cx.executor().now(), true),
            (1., false)
        );
    }
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        press_scale(control(&chrome, cx).press, cx.executor().now(), true),
        (1., false)
    );
}

#[test]
fn u07_elapsed_easing_is_bounded_and_idle_tweens_do_not_request_frames() {
    let now = std::time::Instant::now();
    assert_eq!(press_scale(None, now, false), (1., false));
    let mut control = Control::default();
    control.set_pressed(true, now, false);
    assert_eq!(
        press_scale(control.press, now - Duration::from_millis(1), false),
        (1., true)
    );
    control.set_pressed(false, now, false);
    assert_eq!(press_scale(control.press, now, false), (1., false));
    control.set_pressed(true, now, false);
    assert_eq!(
        press_scale(control.press, now + Duration::from_secs(30), false),
        (0.96, false)
    );
    control.set_pressed(false, now + Duration::from_secs(30), false);
    assert_eq!(
        press_scale(control.press, now + Duration::from_secs(60), false),
        (1., false)
    );
}

#[gpui::test]
fn u07_live_early_release_and_repress_keep_scale_continuous(cx: &mut TestAppContext) {
    let (chrome, cx) = open(cx);
    let at = control(&chrome, cx).bounds.unwrap().center();
    down(cx, at);
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
    let now = cx.executor().now();
    let (before, moving) = press_scale(control(&chrome, cx).press, now, false);
    assert!(moving && before > 0.96 && before < 1., "{before}");
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let (after, moving) = press_scale(control(&chrome, cx).press, now, false);
    assert!(
        moving && (before - after).abs() < 0.000001,
        "before={before}, after={after}"
    );
    cx.executor().advance_clock(Duration::from_millis(10));
    cx.run_until_parked();
    let now = cx.executor().now();
    let (rebound, _) = press_scale(control(&chrome, cx).press, now, false);
    assert!(rebound > after && rebound < 1.);
    down(cx, at);
    let (repress, moving) = press_scale(control(&chrome, cx).press, now, false);
    assert!(moving && (rebound - repress).abs() < 0.000001);
    cx.executor().advance_clock(Duration::from_millis(20));
    let (deeper, moving) = press_scale(control(&chrome, cx).press, cx.executor().now(), false);
    assert!(moving && deeper < repress && deeper > 0.96);
    cx.executor().advance_clock(Duration::from_millis(130));
    assert_eq!(
        press_scale(control(&chrome, cx).press, cx.executor().now(), false),
        (0.96, false)
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(150));
    assert_eq!(
        press_scale(control(&chrome, cx).press, cx.executor().now(), false),
        (1., false)
    );
}

#[gpui::test]
fn review_u07_hover_clears_when_chrome_stops_taking_input(cx: &mut TestAppContext) {
    use crate::path::{
        chrome::{ChromeView, Inputs},
        suggestions::Unranked,
        view::PathView,
    };
    use gpui::AppContext;
    let (chrome, cx) = cx.add_window_view(|_, cx| {
        let path = cx.new(|cx| PathView::new(Default::default(), Box::new(Unranked), cx));
        ChromeView::new(
            path,
            Inputs {
                view: super::startup::View::List,
                back: false,
                up: true,
                filter: true,
                sort: true,
                live: true,
                corner: 0.,
            },
            cx,
        )
    });
    cx.simulate_resize(gpui::size(px(1100.), px(27.)));
    cx.run_until_parked();
    let at = chrome.read_with(cx, |view, _| {
        view.frame.borrow().controls[2].bounds.unwrap().center()
    });
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    assert!(chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].hovered));
    chrome.update(cx, |view, cx| {
        view.inputs.live = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        !chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].hovered),
        "inputLive=false must retire an existing hover without another pointer move"
    );
}
