//! EY-200: a chrome mark held for the press-and-hold interval is a long press, so its release never navigates.
use super::u07_motion_tests::{down, open};
use gpui::{Modifiers, MouseButton, TestAppContext};
use std::{cell::RefCell, rc::Rc, time::Duration};

// QStyleHints::mousePressAndHoldInterval, as path/tap.rs LONG_PRESS.
const LONG_PRESS: Duration = Duration::from_millis(800);

#[gpui::test]
fn ey200_up_held_past_the_long_press_interval_does_not_navigate(cx: &mut TestAppContext) {
    let (chrome, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let at = chrome
        .read_with(cx, |view, _| view.frame.borrow().controls[1].bounds)
        .unwrap()
        .center();
    for (held, taps) in [(LONG_PRESS - Duration::from_millis(1), 1), (LONG_PRESS, 0)] {
        let before = seen.borrow().len();
        cx.simulate_mouse_move(at, None, Modifiers::none());
        down(cx, at);
        cx.executor().advance_clock(held);
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(seen.borrow().len() - before, taps, "held {held:?}");
    }
    assert!(seen.borrow().iter().all(|action| *action == "up"));
}
