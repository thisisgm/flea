//! Run with FLEA_CUSTOM_PREVIEW=1 to exercise the native popout through the browser's keys.
use super::super::test_shell::{land, set_cursor};
use super::{draw, file, opened};
use gpui::{Modifiers, TestAppContext, VisualTestContext, point};

#[gpui::test]
fn custom_port_native_popout_navigation_keeps_the_browser_cursor(cx: &mut TestAppContext) {
    if !crate::window::quick_look::custom::enabled() {
        return;
    }
    cx.executor().allow_parking();
    let (shell, cx, _) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("one.txt", "text-plain"), file("two.txt", "text-plain")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let view = shell.read_with(cx, |shell, cx| {
        let host = shell.quick_look.as_ref().unwrap();
        assert!(host.custom.window.is_some());
        assert_eq!(
            host.view.read(cx).custom.as_ref().unwrap().path(),
            Some("/files/one.txt")
        );
        host.view.clone()
    });
    view.update(cx, |view, cx| {
        view.action("cursorDown", cx);
    });
    draw(cx);
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
        assert_eq!(
            view.read(cx).custom.as_ref().unwrap().path(),
            Some("/files/two.txt")
        );
    });
    view.update(cx, |view, cx| {
        view.action("escape", cx);
    });
    draw(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(!shell.quick_look_open(cx));
        assert!(shell.quick_look.as_ref().unwrap().custom.window.is_none());
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
    });
    super::close(&shell, cx);
}

#[gpui::test]
fn custom_port_preview_toolbar_mouse_controls_toggle(cx: &mut TestAppContext) {
    if !crate::window::quick_look::custom::enabled() {
        return;
    }
    cx.executor().allow_parking();
    let (shell, cx, _) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file("one.txt", "text-plain"), file("two.txt", "text-plain")],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);

    let handle = shell.read_with(cx, |shell, _| {
        shell.quick_look.as_ref().unwrap().custom.window.unwrap()
    });
    let mut popup = VisualTestContext::from_window(handle.into(), &cx.cx);
    popup.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    for (selector, field, expected) in [
        ("preview-toggleInfo", "info", true),
        ("preview-toggleFilmstrip", "filmstrip", false),
    ] {
        let bounds = popup
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing {selector}"));
        popup.simulate_click(
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            ),
            Modifiers::none(),
        );
        popup.run_until_parked();
        shell.read_with(&mut popup, |shell, cx| {
            let custom = shell
                .quick_look
                .as_ref()
                .unwrap()
                .view
                .read(cx)
                .custom
                .as_ref()
                .unwrap();
            match field {
                "info" => assert_eq!(custom.info, expected),
                "filmstrip" => assert_eq!(custom.filmstrip, expected),
                _ => unreachable!(),
            }
        });
    }
    super::close(&shell, &mut popup);
}
