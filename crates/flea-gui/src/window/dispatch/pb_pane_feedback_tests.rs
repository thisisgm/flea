//! Pane feedback through the mounted production Root and its status readers.
use super::{
    listing_pointer_tests::row_point,
    menus_tests::settle,
    test_shell::{asked, land, lists, open_root_with, row},
    Order,
};
use crate::{
    backend::{Backend, Event},
    window::{input::ActionEvent, u07_tests::read},
};
use gpui::{Modifiers, MouseButton, MouseDownEvent, TestAppContext};

#[gpui::test]
fn pb_pane_f4_outside_list_explains_the_required_view(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"grid""#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    cx.simulate_keystrokes("f4");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Autofit needs the list view."
    );
    assert_eq!(read(&shell, cx, "statusError", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_unhandled_action_reports_unavailable(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(
                Order::Action(ActionEvent {
                    action: "unhandled-test-action",
                    seat: Default::default(),
                    context: flea::keys::Context::Listing,
                    pair_armed: false,
                }),
                window,
                cx,
            );
        })
    });
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "That action is not available yet"
    );
    assert_eq!(read(&shell, cx, "statusError", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_busy_key_says_loading_and_keeps_the_request(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    cx.simulate_keystrokes("enter");
    let count = lists(&shell, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "A directory is already loading."
    );
    assert_eq!(asked(&shell, cx), "/files/folder");
    assert_eq!(lists(&shell, cx), count);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_busy_row_press_says_loading_without_opening(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    let position = row_point(&shell, cx, 0);
    cx.simulate_keystrokes("enter");
    let count = lists(&shell, cx);
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 1,
        modifiers: Modifiers::none(),
        first_mouse: false,
    });
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "A directory is already loading."
    );
    assert_eq!(asked(&shell, cx), "/files/folder");
    assert_eq!(lists(&shell, cx), count);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_ey178_locked_navigation_uses_the_board_sentence_in_both_places(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![row("Locked", true)], 1);
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::Refused {
                    operation: "scan".into(),
                    path: "/files/Locked".into(),
                    message: "permission denied".into(),
                    mode: 0o040000,
                },
                window,
                cx,
            )
        })
    });
    settle(cx);
    let sentence = "You don’t have permission to open this folder.";
    assert_eq!(read(&shell, cx, "stateMessage", &[]), sentence);
    assert_eq!(read(&shell, cx, "lastMessage", &[]), sentence);
    assert_eq!(read(&shell, cx, "statusError", &[]), "true");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_storage_class_is_readable_without_a_preview(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    for (path, class, expected) in [("/files", "usb", "usb"), ("/stale", "network", "usb")] {
        let wire = format!(
            r#"{{"t":"fsinfo","path":"{path}","fs":"ext4","free":42,"class":"{class}","system":false}}"#
        );
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.receive(0, Backend::test_reply_line(&wire), window, cx)
            })
        });
        assert_eq!(read(&shell, cx, "storageClass", &[]), expected);
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
