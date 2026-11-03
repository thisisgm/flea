//! A pending outline line rides only the re-list of its own folder, and leaving List sends the mode.
use super::test_shell::{land, open, row, sent};
use gpui::{TestAppContext, VisualTestContext};

fn key(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
}

#[gpui::test]
fn tree_outline_leaving_list_sends_the_mode_in_place_of_the_list(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        1,
    );
    sent();
    key(cx, "ctrl-right");
    sent();
    let mut folder = row("A", true);
    folder.open = true;
    land(&shell, cx, "/files", vec![folder, row("end.txt", false)], 2);
    sent();
    key(cx, "ctrl-3");
    let commands = sent();
    assert!(
        commands
            .iter()
            .any(|line| line.contains("\"c\":\"outline\"") && line.contains("\"enabled\":false")),
        "{commands:?}"
    );
    assert!(!commands
        .iter()
        .any(|line| flea::json::field_str(line, "c").as_deref() == Some("list")));
}

fn ride(
    cx: &mut TestAppContext,
    line: &str,
    mode: bool,
) -> (Vec<String>, Option<usize>, Option<usize>) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("B", true)],
        1,
    );
    sent();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().outline_request = Some(crate::window::outline::Request {
            folder: "/files".into(),
            line: line.into(),
            mode,
        });
    });
    key(cx, "enter");
    let commands = sent();
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().outline_request.is_none());
    });
    let list = commands
        .iter()
        .position(|sent| flea::json::field_str(sent, "path").as_deref() == Some("/files/A"));
    let outline = commands
        .iter()
        .position(|sent| flea::json::field_str(sent, "c").as_deref() == Some("outline"));
    assert!(
        list.is_some(),
        "a list of another folder is never replaced: {commands:?}"
    );
    (commands, list, outline)
}

#[gpui::test]
fn tree_outline_row_toggle_of_a_folder_left_behind_is_dropped(cx: &mut TestAppContext) {
    let line = "{\"c\":\"outline\",\"row\":1,\"open\":true,\"listing\":1,\"first\":350}";
    let (commands, _, outline) = ride(cx, line, false);
    assert_eq!(outline, None, "{commands:?}");
}

#[gpui::test]
fn tree_outline_mode_switch_reaches_the_backend_before_a_list_elsewhere(cx: &mut TestAppContext) {
    let line = "{\"c\":\"outline\",\"enabled\":false,\"first\":350}";
    let (commands, list, outline) = ride(cx, line, true);
    assert!(outline.is_some() && outline < list, "{commands:?}");
}
