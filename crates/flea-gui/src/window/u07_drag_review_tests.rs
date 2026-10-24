//! A canceled chrome drag must not become a new tap on returning to its origin.
use crate::path::{chrome::{ChromeView, Inputs}, suggestions::Unranked, view::PathView};
use gpui::{point, px, AppContext, Modifiers, MouseButton, TestAppContext};
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn review_u07_drag_beyond_the_oracle_threshold_cannot_rearm_a_tap(cx: &mut TestAppContext) {
    let (chrome, cx) = cx.add_window_view(|_, cx| {
        let path = cx.new(|cx| PathView::new(Default::default(), Box::new(Unranked), cx));
        ChromeView::new(path, Inputs {
            view: super::startup::View::List,
            back: false,
            up: true,
            filter: true,
            sort: true,
            live: true,
            corner: 0.,
        }, cx)
    });
    cx.simulate_resize(gpui::size(px(1100.), px(27.)));
    cx.run_until_parked();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _events = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            seen.borrow_mut().push(*action)
        })
    });
    let at = chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].bounds.unwrap().center());
    let outside = point(px(2.), px(26.));
    assert!(f32::from(at.x - outside.x).abs() > 500.);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 1,
        modifiers: Modifiers::none(),
        first_mouse: false,
    });
    cx.run_until_parked();
    assert!(chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].pressed));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    assert!(!chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].pressed));
    cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(seen.borrow().is_empty(),
        "ChromeButton.qml:17 DragThreshold cancels this drag; returning cannot emit {:?}",
        seen.borrow().as_slice());
}
