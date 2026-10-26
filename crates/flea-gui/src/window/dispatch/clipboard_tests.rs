//! Copy path, the folder path, the Copy as and Paste as keys and the image paste run in the production Shell from real keystrokes.
use super::test_shell::{keystroke, land, open, row, sent, set_cursor, status};
use crate::{backend::Backend, state::Shell};
use flea::keys::{self, Frontend};
use gpui::{Entity, TestAppContext, VisualTestContext};

// The table's own row for an action, pressed as the platform delivers it.
fn press(cx: &mut VisualTestContext, preset: &str, action: &str, mods: &str, key: &str) {
    let binding = keys::binding_rows(preset, Frontend::Gui)
        .find(|b| b.action == action && b.mods == mods && b.key == key)
        .unwrap_or_else(|| panic!("{preset} binds {mods}+{key} to {action}"));
    cx.simulate_event(keystroke(binding));
    cx.run_until_parked();
}

// A backend reply line, parsed by the production transport and handed to the Shell as the reader does.
fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    let event = Backend::test_reply_line(line);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0, event, window, cx)));
    cx.run_until_parked();
}

fn home(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let rows = vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ];
    land(shell, cx, "/files", rows, 1);
    set_cursor(shell, cx, 2);
}

fn text(cx: &mut VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn copy_path_and_the_folder_path_reach_the_text_clipboard(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    home(&shell, cx);
    sent();
    press(cx, "default", "copyPath", "ctrlshift", "C");
    let asked = sent();
    assert!(
        asked
            .iter()
            .any(|line| line == r#"{"c":"paths","rows":[2]}"#),
        "the cursor row is named by the backend: {asked:?}"
    );
    let paths = vec!["/files/notes.txt".to_owned()];
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, crate::backend::Event::Paths(paths), window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(text(cx).as_deref(), Some("/files/notes.txt"));
    press(cx, "default", "copydirpath", "text", "Y");
    assert_eq!(text(cx).as_deref(), Some("/files"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn an_empty_clipboard_pastes_an_image_and_paste_as_says_there_is_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    home(&shell, cx);
    press(cx, "default", "pasteAs", "text", "P");
    assert_eq!(
        status(&shell, cx),
        "There is nothing to paste; y copies and x cuts."
    );
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
    sent();
    press(cx, "default", "paste", "ctrl", "V");
    let mut lines = sent();
    if lines.iter().any(|line| line == r#"{"c":"clipGet"}"#) {
        reply(
            &shell,
            cx,
            r#"{"t":"clip","op":"get","ok":true,"clip":"","paths":[],"token":"","skipped":0}"#,
        );
        lines = sent();
    }
    assert!(
        lines.iter().any(|line| line
            .starts_with(r#"{"c":"pasteimage","dest":"/files","name":"Pasted image "#)
            && line.ends_with(r#".png"}"#)),
        "{lines:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn c_opens_the_cursor_menu_on_its_copy_as_flyout(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    home(&shell, cx);
    sent();
    press(cx, "default", "copyAs", "text", "c");
    let id = shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.menu.flyout,
            Some("copyAs"),
            "the flyout waits for the menu's rows"
        );
        shell
            .menu
            .captured
            .as_ref()
            .expect("the cursor menu opened")
            .id
    });
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    for _ in 0..4 {
        let lines = sent();
        if lines
            .iter()
            .any(|line| line.contains(r#""op":"applications""#))
        {
            reply(
                &shell,
                cx,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
                ),
            );
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        assert_eq!(
            menu.flyout.as_ref().map(|flyout| flyout.action.as_str()),
            Some("copyAs")
        );
        assert!(shell.menu.flyout.is_none());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn c_on_an_empty_folder_says_there_is_no_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    press(cx, "default", "copyAs", "text", "c");
    assert_eq!(
        status(&shell, cx),
        "No item under the cursor to open a menu on"
    );
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
