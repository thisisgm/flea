//! Tree keys drive the mounted List and preserve the existing pane operation path.
use super::test_shell::{land, open, row, sent};
use crate::window::u07_tests::read;
use gpui::{TestAppContext, VisualTestContext};

fn key(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
}

#[gpui::test]
fn tree_outline_keys_open_once_and_enter_keeps_navigating(cx: &mut TestAppContext) {
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
    let commands = sent();
    assert_eq!(
        commands
            .iter()
            .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("outline"))
            .count(),
        1
    );
    assert!(!commands
        .iter()
        .any(|line| flea::json::field_str(line, "c").as_deref() == Some("list")));
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(
        &shell,
        cx,
        "/files",
        vec![folder.clone(), child, row("end.txt", false)],
        2,
    );
    assert_eq!(read(&shell, cx, "rowExpanded", &["0"]), "true");
    assert_eq!(read(&shell, cx, "rowDepth", &["1"]), "1");
    assert_eq!(read(&shell, cx, "rowNameText", &["1"]), "child.txt");
    assert_eq!(read(&shell, cx, "rowGlyph", &["0"]), "folder-open");
    // The 19:08 native capture drew chooser checkboxes beside nested files: depth is never a check slot.
    cx.run_until_parked();
    let checks = shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        (list.painted_marks.get(), list.painted_checks.get())
    });
    assert!(checks.0 > 0, "the outline frame painted its rows");
    assert_eq!(
        checks.1, 0,
        "a nested file outside the chooser paints no check slot"
    );
    sent();
    key(cx, "ctrl-right");
    assert!(
        !sent().iter().any(|line| line.contains("\"c\":\"outline\"")),
        "an open folder is a no-op"
    );
    key(cx, "enter");
    assert!(sent()
        .iter()
        .any(|line| flea::json::field_str(line, "path").as_deref() == Some("/files/A")));
}

#[gpui::test]
fn tree_outline_5000_row_count_shapes_only_the_viewport(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    sent();
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.list_requests
    });
    key(cx, "ctrl-right");
    let commands = sent();
    assert_eq!(
        commands
            .iter()
            .filter(|line| line.contains("\"c\":\"outline\""))
            .count(),
        1
    );
    let mut rows = vec![row("A", true)];
    rows[0].open = true;
    for n in 0..319 {
        let mut child = row(&format!("A/f{n}.txt"), false);
        child.depth = 1;
        rows.push(child);
    }
    super::test_shell::land_on(&shell, cx, 0, "/files", rows, 5001, 2);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.list_requests - before, 1);
        assert_eq!(host.pane.held.total, 5001);
        let state = host.list.borrow();
        assert!(
            state.rows.misses <= (host.pane.visible_rows() * 2 + 2) as u64,
            "row shaping stays on the viewport"
        );
        assert!(state.visible().len() <= host.pane.visible_rows() + 1);
    });
}

#[gpui::test]
fn tree_outline_close_reuses_the_same_listing_anchor(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(
        &shell,
        cx,
        "/files",
        vec![folder, child, row("end.txt", false)],
        1,
    );
    sent();
    key(cx, "ctrl-left");
    let commands = sent();
    assert!(commands
        .iter()
        .any(|line| line.contains("\"c\":\"outline\"") && line.contains("\"open\":false")));
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        2,
    );
    assert_eq!(read(&shell, cx, "rowExpanded", &["0"]), "false");
    assert_eq!(read(&shell, cx, "rowNameText", &["1"]), "end.txt");
}

#[gpui::test]
fn tree_outline_marks_follow_file_identity_when_rows_are_inserted(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        1,
    );
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    key(cx, "ctrl-right");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(
        &shell,
        cx,
        "/files",
        vec![folder, child, row("end.txt", false)],
        2,
    );
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [2]);
        assert_eq!(host.pane.cursor, 0);
    });
}

#[gpui::test]
fn tree_outline_nested_rename_edits_the_leaf_and_sends_the_real_source(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(&shell, cx, "/files", vec![folder, child], 1);
    super::test_shell::set_cursor(&shell, cx, 1);
    sent();
    key(cx, "f2");
    shell.read_with(cx, |shell, _| {
        let edit = shell.rename.edit.as_ref().unwrap();
        assert_eq!(edit.source, "/files/A/child.txt");
        assert_eq!(edit.entry.text, "child.txt");
    });
    cx.simulate_input("renamed");
    key(cx, "enter");
    assert!(sent().iter().any(|line| line
        == r#"{"c":"rename","path":"/files/A/child.txt","to":"renamed.txt","menuId":0}"#));
    super::fileops_tests::reply(
        &shell,
        cx,
        r#"{"t":"renamed","ok":true,"path":"/files/A/renamed.txt"}"#,
    );
    let commands = sent();
    assert!(commands
        .iter()
        .any(
            |line| flea::json::field_str(line, "c").as_deref() == Some("list")
                && flea::json::field_str(line, "select").as_deref() == Some("/files/A/renamed.txt")
        ));
    let mut folder = row("A", true);
    folder.open = true;
    let mut renamed = row("A/renamed.txt", false);
    renamed.depth = 1;
    land(&shell, cx, "/files", vec![folder, renamed], 2);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 1);
        assert_eq!(host.selected, [1]);
    });
}

#[gpui::test]
fn tree_outline_copy_path_and_trash_use_the_nested_row_numbering(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(&shell, cx, "/files", vec![folder, child], 1);
    super::test_shell::set_cursor(&shell, cx, 1);
    sent();
    key(cx, "ctrl-shift-c");
    assert!(sent()
        .iter()
        .any(|line| line.contains(r#""c":"paths","rows":[1]"#)));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Paths(vec!["/files/A/child.txt".into()]),
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("/files/A/child.txt")
    );
    key(cx, "delete");
    assert!(sent()
        .iter()
        .any(|line| line == r#"{"c":"trash","rows":[1],"menuId":0,"listing":1}"#));
}

#[gpui::test]
fn tree_outline_quick_look_requests_the_nested_file(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = super::preview_tests::opened(cx);
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = super::preview_tests::file("A/child.txt", "text-plain");
    child.depth = 1;
    land(&shell, cx, "/files", vec![folder, child], 1);
    super::test_shell::set_cursor(&shell, cx, 1);
    key(cx, "space");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    let asked = super::preview_tests::sent(&mut requests, "code");
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0].get("path").and_then(flea::jsondoc::Json::as_str),
        Some("/files/A/child.txt")
    );
    key(cx, "escape");
    super::preview_tests::close(&shell, cx);
}

#[gpui::test]
fn tree_outline_drag_payload_uses_the_nested_file_path(cx: &mut TestAppContext) {
    let (shell, cx) = super::test_shell::open_root(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(&shell, cx, "/files", vec![folder, child], 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    let rect = read(&shell, cx, "rowNameRect", &["1"]);
    let values: Vec<f32> = rect
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let from = gpui::point(
        gpui::px(values[0] + 5.),
        gpui::px(values[1] + values[3] / 2.),
    );
    cx.simulate_mouse_move(from, None, gpui::Modifiers::default());
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            let gpui::ExternalDragPayload::Mime(payload) =
                shell.file_drag_payload(window, cx).unwrap()
            else {
                panic!("file drag has a MIME payload")
            };
            let uris = payload.data.get("text/uri-list").unwrap();
            assert_eq!(&*uris, b"file:///files/A/child.txt\r\n");
            shell.forget_file_drags(window, cx);
        })
    });
    cx.simulate_mouse_up(from, gpui::MouseButton::Left, gpui::Modifiers::default());
}

#[gpui::test]
fn tree_outline_nested_mark_keeps_its_mark_and_toggles_on_release(cx: &mut TestAppContext) {
    let (shell, cx) = super::test_shell::open_root(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/empty", true);
    child.depth = 1;
    land(&shell, cx, "/files", vec![folder.clone(), child.clone()], 1);
    let values: Vec<f32> = read(&shell, cx, "rowRect", &["1"])
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let at = gpui::point(
        gpui::px(values[0] + 14. + 32. + 11.),
        gpui::px(values[1] + values[3] / 2.),
    );
    let rest = read(&shell, cx, "rowGlyph", &["1"]);
    sent();
    cx.simulate_mouse_move(at, None, gpui::Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        read(&shell, cx, "rowGlyph", &["1"]),
        rest,
        "nothing changes under the pointer"
    );
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    assert_eq!(read(&shell, cx, "rowGlyph", &["1"]), rest);
    assert!(!sent().iter().any(|line| line.contains("\"c\":\"outline\"")));
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(sent()
        .iter()
        .any(|line| line.contains("\"c\":\"outline\",\"rows\":[1],\"open\":true,\"anchor\":0")));
    child.open = true;
    land(&shell, cx, "/files", vec![folder, child], 2);
    assert_eq!(read(&shell, cx, "rowGlyph", &["1"]), "folder-open");
    assert_eq!(
        read(&shell, cx, "cursor", &[]),
        "0",
        "a mark click leaves the cursor where it was"
    );
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(sent()
        .iter()
        .any(|line| line.contains("\"c\":\"outline\",\"rows\":[1],\"open\":false")));
}

#[path = "outline_selection_tests.rs"]
mod selection_tests;
