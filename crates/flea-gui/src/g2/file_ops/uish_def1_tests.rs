//! Held progress must not take a listing's pointer before its card draws.
use super::atomic_tests::{advance, drawn, line, open_with};
use crate::backend::Backend;
use gpui::{point, px, MouseButton, TestAppContext, VisualTestContext};

#[gpui::test]
fn uish_def1_fast_script_job_keeps_the_next_pointer_menu(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_lines();
    let handle = open_with(cx, backend);
    assert!(!drawn(cx, handle));
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"action","head":"Running script","state":"running"}}"#,
    );
    assert!(!drawn(cx, handle), "progress has not drawn during its hold");
    let at = handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            point(
                list.drawn.bounds.left() + px(40.),
                list.drawn.bounds.top() + px(10.),
            )
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        at,
        MouseButton::Right,
        Default::default(),
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_up(
        at,
        MouseButton::Right,
        Default::default(),
    );
    handle
        .update(cx, |root, _, cx| {
            assert!(
                root.shell.read(cx).menu.captured.is_some(),
                "the next right click captures its menu"
            );
        })
        .unwrap();
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"action","head":"Script finished","state":"done"}}"#,
    );
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .update(cx, |shell, cx| shell.retire_menu(window, cx));
        })
        .unwrap();
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":14,"kind":"action","head":"Script failed","state":"failed","reason":"exit 2"}}"#,
    );
    advance(cx, handle, 200);
    assert!(drawn(cx, handle), "the failed script keeps its Jobs card");
    let inside = handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let frame = shell
                .overlay
                .as_ref()
                .unwrap()
                .read(cx)
                .feedback_paint
                .get("frame")
                .unwrap();
            point(frame.bounds.left() + px(10.), frame.bounds.top() + px(10.))
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        inside,
        MouseButton::Right,
        Default::default(),
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_up(
        inside,
        MouseButton::Right,
        Default::default(),
    );
    handle
        .update(cx, |root, _, cx| {
            assert!(
                root.shell.read(cx).menu.captured.is_none(),
                "the card owns a press inside itself"
            );
        })
        .unwrap();
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .drawn
                    .bounds
                    .contains(&inside));
                let press = gpui::MouseDownEvent {
                    position: inside,
                    button: MouseButton::Right,
                    click_count: 1,
                    ..Default::default()
                };
                shell.menu_pointer_open(0, &press, window, cx);
                assert!(
                    shell.menu.captured.is_none(),
                    "the listing itself refuses a press on the painted Jobs frame"
                );
            });
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        at,
        MouseButton::Right,
        Default::default(),
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_up(
        at,
        MouseButton::Right,
        Default::default(),
    );
    handle
        .update(cx, |root, _, cx| {
            assert!(
                root.shell.read(cx).menu.captured.is_some(),
                "a failed script leaves the listing menu available beside its card"
            );
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            assert!(
                shell.menu.captured.is_none(),
                "Escape closes the menu first"
            );
            assert!(
                shell.jobs_card_showing(cx),
                "Escape keeps the failed Jobs card"
            );
        })
        .unwrap();
}
