//! Permanent deletion uses the reviewed producer snapshot.
use super::*;

#[gpui::test]
fn shift_delete_reviews_its_own_snapshot_and_opens_the_permanent_card(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("shift-delete");
    let snapshot = sent();
    assert_eq!(snapshot.len(), 1);
    let id = flea::json::field_usize(&snapshot[0], "id").unwrap();
    assert_eq!(
        snapshot[0],
        format!(
            r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":1,"rows":[2],"cursor":2}}"#
        )
    );
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"prepareDelete","id":{id}}}"#
        )]
    );
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"prepareDelete","ok":true,"token":77,"count":1,"bytes":12}}"#
        ),
    );
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the permanent-delete card is up"
    );
    // The card's Delete sends the reviewed token on the same snapshot.
    cx.simulate_keystrokes("tab enter");
    let deleted = sent();
    assert!(
        deleted
            .iter()
            .any(|line| line
                == &format!(r#"{{"c":"menuaction","op":"delete","id":{id},"token":77}}"#)),
        "{deleted:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_refused_review_says_why_and_closes_its_snapshot(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("shift-delete");
    let id = flea::json::field_usize(&sent()[0], "id").unwrap();
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":false,"error":"Selected items changed; reopen the menu."}}"#
        ),
    );
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)]
    );
    assert_eq!(
        status(&shell, cx),
        "Selected items changed; reopen the menu."
    );
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// S9G-3: once the permanent card has closed, a press on a surface that takes no focus (the status bar) keeps the list's keys.
#[gpui::test]
fn a_press_on_the_status_bar_after_a_closed_card_keeps_the_list_keys(cx: &mut TestAppContext) {
    // The window root draws the card layer over the panes, so a stray press reaches it as on the desktop.
    let (shell, cx) = super::super::test_shell::open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("shift-delete");
    let id = flea::json::field_usize(&sent()[0], "id").unwrap();
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"prepareDelete","ok":true,"token":77,"count":1,"bytes":12}}"#
        ),
    );
    assert!(shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    cx.simulate_keystrokes("escape");
    super::super::menus_tests::settle(cx);
    let focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| shell.read_with(cx, |shell, _| shell.focus.is_focused(window)))
    };
    assert!(focused(cx), "Cancel hands the keys back to the list");
    let status =
        cx.update(|window, cx| shell.read_with(cx, |shell, _| shell.geometry(window).status));
    let at = gpui::point(
        gpui::px(status.x + status.width / 2.),
        gpui::px(status.y + status.height / 2.),
    );
    for _ in 0..2 {
        cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        super::super::menus_tests::settle(cx);
        assert!(
            focused(cx),
            "a status bar press leaves the keys on the list"
        );
    }
    let before = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    cx.simulate_keystrokes("j");
    let after = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert_eq!(after, before + 1, "j still moves the cursor");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
