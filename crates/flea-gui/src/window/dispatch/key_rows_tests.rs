//! EY-55 and EY-56: G jumps plainly to the last row in every view, and P with an empty clipboard says why.
use super::menus_tests::{answer, home, settle};
use super::test_shell::{asked, asked_on, land, land_on, open_root_with};
use crate::{
    backend::Backend,
    state::Shell,
    window::{navigate::Step, u07_tests::read},
};
use gpui::{Entity, TestAppContext, VisualTestContext};

// The sweep's key rows: a native root launch selecting notes.txt (row 2 of 4), every pane listed.
pub(super) fn launched<'a>(
    cx: &'a mut TestAppContext,
    view: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open_root_with(
        cx,
        &format!(r#""keys":"default","view":"{view}","escapeUp":true"#),
        Some("/files/notes.txt"),
    );
    land(&shell, cx, "/files", home(), 1);
    if view == "dual" {
        let right = asked_on(&shell, cx, 1);
        land_on(&shell, cx, 1, &right, home(), 4, 1);
    }
    settle(cx);
    assert_eq!(state(&shell, cx), (2, vec![2]), "the asked row is selected");
    (shell, cx)
}

// The focused pane's cursor and marks.
pub(super) fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, Vec<usize>) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[shell.focus_side].as_ref().unwrap();
        (host.pane.cursor, host.selected.clone())
    })
}

// menus_tests' regular-file mode bits for the snapshot answer.
const FILE: u32 = 0o100644;

// The backend's clipboard watch reporting a copied path, as the dg1 join tests deliver it.
fn copied(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let line = r#"{"t":"clip","op":"changed","clip":"copy","paths":["/other/a.txt"],"token":"T1"}"#;
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx)
        })
    });
    settle(cx);
}

// Marks.js follow: G is cursorLast, a plain move, so the landed lone row follows the cursor to the last row.
fn g_jumps_to_the_last_row(cx: &mut TestAppContext, view: &str) {
    let (shell, cx) = launched(cx, view);
    cx.simulate_keystrokes("shift-g");
    settle(cx);
    assert_eq!(state(&shell, cx), (3, vec![3]), "G in {view}");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn g_jumps_to_the_last_row_in_list(cx: &mut TestAppContext) {
    g_jumps_to_the_last_row(cx, "list");
}

#[gpui::test]
fn g_jumps_to_the_last_row_in_columns(cx: &mut TestAppContext) {
    g_jumps_to_the_last_row(cx, "columns");
}

#[gpui::test]
fn g_jumps_to_the_last_row_in_grid(cx: &mut TestAppContext) {
    g_jumps_to_the_last_row(cx, "grid");
}

#[gpui::test]
fn g_jumps_to_the_last_row_in_dual(cx: &mut TestAppContext) {
    g_jumps_to_the_last_row(cx, "dual");
}

// The footer's shown message is in the error role.
fn said_as_error(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.shown().is_error)
    })
}

// Marks.js follow in Grid: an arrow carries the landed lone row, and Escape still climbs through it.
#[gpui::test]
fn a_grid_arrow_carries_the_landing_and_escape_climbs(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "grid");
    cx.simulate_keystrokes("left");
    settle(cx);
    assert_eq!(state(&shell, cx), (1, vec![1]), "the lone mark follows");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(
        asked(&shell, cx),
        "/",
        "Escape climbs past the carried landing"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Pane.qml openPasteAs: an empty clipboard is root.message(..., false), said before any menu opens.
#[gpui::test]
fn p_with_an_empty_clipboard_says_why(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    cx.simulate_keystrokes("shift-p");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
    assert!(!said_as_error(&shell, cx), "the info role");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "There is nothing to paste; y copies and x cuts."
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Pane.qml openPasteAs: with paths on the clipboard, P opens the cursor's menu on its Paste as flyout.
#[gpui::test]
fn p_with_paths_on_the_clipboard_opens_paste_as(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    copied(&shell, cx);
    cx.simulate_keystrokes("shift-p");
    assert_eq!(read(&shell, cx, "contextMenuSubmenuEntries", &[]), "");
    answer(&shell, cx, &[FILE]);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert!(
        read(&shell, cx, "contextMenuSubmenuEntries", &[]).contains("Link"),
        "the Paste as flyout is open: {}",
        read(&shell, cx, "contextMenuSubmenuEntries", &[])
    );
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Focus.js pasteAs: Recent is a history, so P refuses there before any menu, whatever the clipboard holds.
#[gpui::test]
fn p_in_recent_refuses_before_any_menu(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    copied(&shell, cx);
    let recent = flea::ui_logic_pk1::RECENT;
    let done = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.step(0, Step::Open(recent.into()), window, cx)
        })
    });
    assert_eq!(done, Ok(true));
    land(&shell, cx, recent, home(), 2);
    cx.simulate_keystrokes("shift-p");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert!(!said_as_error(&shell, cx), "the info role");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Recent cannot take a paste. Open a folder first"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
