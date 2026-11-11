//! The live window keeps a closing menu card drawn for its 140 ms fade while the listing under it already takes the pointer.
use super::{
    element::PAINTED,
    join_tests::{open, read, ready},
};
use crate::window::root::Root;
use gpui::{point, px, AppContext, MouseButton, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

// The card frames one real draw of the live window painted.
fn painted(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> usize {
    PAINTED.with(|painted| painted.borrow_mut().clear());
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    PAINTED.with(|painted| painted.borrow().len())
}

#[gpui::test]
fn live_menu_close_fades_without_holding_the_pointer(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    ready(cx, &mut fixture);
    assert_eq!(painted(cx, handle), 0, "the open fade starts at opacity 0");
    assert!(
        handle
            .update(cx, |root, _, cx| root
                .shell
                .read(cx)
                .menu
                .menu
                .borrow()
                .fades
                .card_frame
                .animating)
            .unwrap(),
        "the open fade asks for its next frame itself"
    );
    cx.executor().advance_clock(Duration::from_millis(200));
    assert_eq!(painted(cx, handle), 1, "the settled card is drawn once");
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    assert_eq!(painted(cx, handle), 1, "the closing card is still drawn");
    cx.executor().advance_clock(Duration::from_millis(70));
    assert_eq!(painted(cx, handle), 1, "and still drawn halfway through");
    // A right click inside the closing card reaches the listing at once.
    let under = handle
        .update(cx, |root, _, cx| {
            let menu = root.shell.read(cx).menu.menu.clone();
            let menu = menu.borrow();
            let (card, _) = menu.revealed_card().expect("the closing card");
            point(
                px(card.frame.x + card.frame.w / 2.),
                px(card.frame.y + card.frame.h / 2.),
            )
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        under,
        MouseButton::Right,
        Default::default(),
    );
    ready(cx, &mut fixture);
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    painted(cx, handle);
    cx.executor().advance_clock(Duration::from_millis(140));
    assert_eq!(painted(cx, handle), 0, "a closed card is gone after 140 ms");
}
