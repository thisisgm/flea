//! c and P open the Copy As and Paste As flyouts as v0.3.12 openCopyAs and openPasteAs do, and the file menu offers Tags.
use super::menus_tests::{actions, answer, home, settle};
use super::test_shell::{land, open_root as open, status};
use crate::window::u07_tests::read;

const FILE: u32 = 0o100644;

#[gpui::test]
fn c_opens_the_copy_as_flyout_and_p_with_an_empty_clipboard_says_nothing_to_paste(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("c");
    let waiting = shell.read_with(cx, |shell, _| shell.menu.flyout);
    assert_eq!(
        waiting,
        Some("copyAs"),
        "the flyout waits for the menu's read"
    );
    answer(&shell, cx, &[FILE]);
    settle(cx);
    let taken = shell.read_with(cx, |shell, _| shell.menu.flyout);
    assert_eq!(taken, None, "the standing card took the waiting flyout");
    assert_eq!(
        read(&shell, cx, "contextMenuSubmenuEntries", &[]),
        "Path|Relative Path|Name|Name Without Extension|Folder Path|File URI|Shell-Quoted",
        "c opens the Copy As flyout"
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    // Pane.qml openPasteAs: an empty clipboard is said before any menu opens.
    cx.simulate_keystrokes("P");
    settle(cx);
    assert_eq!(
        status(&shell, cx),
        "There is nothing to paste; y copies and x cuts.",
        "P with nothing on the file clipboard says the empty-clipboard sentence"
    );
    let (opened, captured) = shell.read_with(cx, |shell, _| {
        (shell.menu_opened(), shell.menu.captured.is_some())
    });
    assert!(!opened && !captured, "P opened no menu and started no read");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn the_file_menu_offers_the_tags_row(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 2
        })
    });
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    assert!(
        actions(&shell, cx).contains(&"tags".to_owned()),
        "the file menu offers Tags: {:?}",
        actions(&shell, cx)
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn c_on_an_empty_listing_says_no_item_under_the_cursor(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.simulate_keystrokes("c");
    settle(cx);
    assert_eq!(
        status(&shell, cx),
        "No item under the cursor to open a menu on",
        "v0.3.12 Pane.qml openCopyAs says this when no row can open a menu"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn m_on_an_empty_listing_says_no_item_under_the_cursor_and_opens_no_menu(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.simulate_keystrokes("m");
    settle(cx);
    assert_eq!(
        status(&shell, cx),
        "No item under the cursor to open a menu on",
        "v0.3.12 Focus.js menu says this when no row is under the cursor"
    );
    let (opened, captured) = shell.read_with(cx, |shell, _| {
        (shell.menu_opened(), shell.menu.captured.is_some())
    });
    assert!(
        !opened && !captured,
        "m opened no folder menu on an empty folder"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn shift_f10_on_an_empty_listing_says_no_item_under_the_cursor_and_opens_no_menu(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.simulate_keystrokes("shift-f10");
    settle(cx);
    assert_eq!(
        status(&shell, cx),
        "No item under the cursor to open a menu on",
        "Shift+F10 is the same menu action as m"
    );
    let (opened, captured) = shell.read_with(cx, |shell, _| {
        (shell.menu_opened(), shell.menu.captured.is_some())
    });
    assert!(
        !opened && !captured,
        "Shift+F10 opened no folder menu on an empty folder"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
