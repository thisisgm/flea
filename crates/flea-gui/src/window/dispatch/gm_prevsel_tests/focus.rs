use super::*;

#[gpui::test]
fn gm_prevsel_a_preview_press_takes_the_keyboard_back_from_the_path_bar(cx: &mut TestAppContext) {
    let (shell, cx, _, start, end) = code_preview(cx, false);
    cx.simulate_keystrokes("ctrl-l");
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "pathBarOpen", &[]),
        "true"
    );
    drag(cx, start, end);
    cx.simulate_keystrokes("ctrl-c");
    pump(cx);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("héllo world\n第二 line".into()),
        "a press in the preview leaves the path bar, as any press in the pane does"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn gm_prevsel_copy_in_the_path_bar_keeps_the_field_text(cx: &mut TestAppContext) {
    let (shell, cx, _, start, end) = code_preview(cx, false);
    drag(cx, start, end);
    cx.simulate_keystrokes("ctrl-l");
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "pathBarOpen", &[]),
        "true"
    );
    cx.simulate_keystrokes("ctrl-c");
    pump(cx);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("/files/".into()),
        "the path field copies its own text, never the preview range"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn gm_prevsel_a_press_outside_a_focused_preview_gives_the_listing_its_keys(
    cx: &mut TestAppContext,
) {
    let (shell, cx, view, start, end) = code_preview(cx, false);
    drag(cx, start, end);
    // A scroll bar press is a press on the column that no text takes, so the column takes the keyboard.
    let focus = shell.read_with(cx, |shell, cx| {
        shell.selection_preview(0).unwrap().read(cx).focus.clone()
    });
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.selection.range().is_none()));
    assert!(cx.update(|window, _| focus.is_focused(window)));
    cx.simulate_keystrokes("ctrl-c");
    let outside = point(px(5.), px(690.));
    cx.simulate_mouse_down(outside, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert!(!cx.update(|window, _| focus.is_focused(window)));
    let _ = pump(cx);
    cx.simulate_keystrokes("ctrl-c");
    assert!(
        pump(cx)
            .iter()
            .any(|line| line.contains("\"c\":\"paths\"") && line.contains("\"rows\":[0]")),
        "a press outside the column takes its key table along with the keyboard"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        flea::keys::Context::Listing
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
