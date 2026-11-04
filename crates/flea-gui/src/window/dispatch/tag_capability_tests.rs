//! Filesystem tag support follows accepted fsinfo and gates both entry paths.
use super::menus_tests::{actions, answer, home, settle};
use super::test_shell::{land, open_root, set_cursor, status};
use crate::{backend::Backend, state::Shell};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[gpui::test]
fn pb_tags_unsupported_filesystem_refuses_key_and_hides_menu_row(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    filesystem(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"vfat","free":7,"class":"usb","tagable":false}"#,
    );
    cx.simulate_keystrokes("T");
    assert_eq!(
        status(&shell, cx),
        "This filesystem does not support file tags."
    );
    assert!(!shell.read_with(cx, |shell, _| shell.menu_opened()));
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644]);
    assert!(
        shell.read_with(cx, |shell, _| shell.menu_opened()),
        "the ordinary file menu still opens"
    );
    assert!(!actions(&shell, cx).contains(&"tags".to_owned()));
    cx.simulate_keystrokes("escape");
    filesystem(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/elsewhere","fs":"btrfs","free":9,"tagable":true}"#,
    );
    cx.simulate_keystrokes("T");
    assert!(
        !shell.read_with(cx, |shell, _| shell.menu_opened()),
        "a stale filesystem reply cannot enable tagging"
    );
    filesystem(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"btrfs","free":9,"tagable":true}"#,
    );
    cx.simulate_keystrokes("T");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.menu.flyout),
        Some("tags")
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn filesystem(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx);
        })
    });
    settle(cx);
}

#[gpui::test]
fn pb_tag_join_retained_disk_facts_do_not_disable_the_next_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    filesystem(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"vfat","free":7,"tagable":false}"#,
    );
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/", home(), 2);
    assert!(
        shell.read_with(cx, |shell, _| shell.can_tag(0)),
        "retained disk facts belong to the previous folder"
    );
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("T");
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_some()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_join_fsinfo_refreshes_an_already_open_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644]);
    assert!(actions(&shell, cx).contains(&"tags".to_owned()));
    filesystem(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"vfat","free":7,"tagable":false}"#,
    );
    assert!(
        !actions(&shell, cx).contains(&"tags".to_owned()),
        "accepted capability must update the retained card"
    );
    assert!(shell.read_with(cx, |shell, _| shell.menu_opened()));
    filesystem(
        &shell,
        cx,
        r#"{"t":"fsinfo","path":"/files","fs":"btrfs","free":9,"tagable":true}"#,
    );
    assert!(actions(&shell, cx).contains(&"tags".to_owned()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
