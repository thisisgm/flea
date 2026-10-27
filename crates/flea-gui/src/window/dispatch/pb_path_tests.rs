//! Path parity through a mounted production window and its input handlers.
use super::test_shell::{asked, land, lists, open_root, open_root_with, row, set_cursor};
use crate::window::u07_tests::{press, read};
use gpui::TestAppContext;

#[gpui::test]
fn pb_path_enter_refuses_a_loaded_row_hidden_by_the_filter(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("beta", false)],
        1,
    );
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.filter.shown = Some(vec![0].into());
        host.pane.cursor = 1;
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(
        super::test_shell::status(&shell, cx),
        "That row is hidden by the filter."
    );
    assert_eq!(asked(&shell, cx), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn alpha(shell: &gpui::Entity<crate::state::Shell>, cx: &mut gpui::VisualTestContext) {
    land(shell, cx, "/files", vec![row("alpha", true)], 1);
    cx.simulate_keystrokes("enter");
    land(shell, cx, "/files/alpha", vec![row("child", true)], 2);
}

#[gpui::test]
fn pb_path_plain_address_is_inert_and_double_click_edits(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","addressBar":"path""#, None);
    alpha(&shell, cx);
    assert_eq!(read(&shell, cx, "crumbText", &[]), "/files/alpha");
    assert_eq!(read(&shell, cx, "crumbCount", &[]), "1");
    let centre = read(&shell, cx, "crumbCentre", &["0"]);
    assert!(!centre.is_empty());
    press(cx, &centre, 1);
    assert_eq!(asked(&shell, cx), "/files/alpha");
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    press(cx, &centre, 2);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "true");
    cx.simulate_keystrokes("escape");
    let settings = read(&shell, cx, "chromeButtonCentre", &["sliders"]);
    press(cx, &settings, 1);
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let choice = shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .read(cx)
            .probe
            .centre("choice:addressBar:1", "pane")
    });
    assert!(
        !choice.is_empty(),
        "the Breadcrumb choice is drawn in Settings"
    );
    press(cx, &choice, 1);
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert_eq!(read(&shell, cx, "crumbCount", &[]), "3");
    let centre = read(&shell, cx, "crumbCentre", &["1"]);
    press(cx, &centre, 1);
    assert_eq!(asked(&shell, cx), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_path_covered_crumbs_do_not_navigate_or_open_the_editor(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    alpha(&shell, cx);
    let parent = read(&shell, cx, "crumbCentre", &["1"]);
    let leaf = read(&shell, cx, "crumbCentre", &["2"]);
    assert!(!parent.is_empty() && !leaf.is_empty());
    for (centre, taps) in [(&parent, 1), (&leaf, 2)] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.settings_action("settings", window, cx);
            })
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert_eq!(read(&shell, cx, "settingsOpen", &[]), "true");
        let before = lists(&shell, cx);
        press(cx, centre, taps);
        assert_eq!(
            lists(&shell, cx),
            before,
            "covered ancestors cannot navigate"
        );
        assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_path_own_trash_lands_on_the_first_deleted_rows_replacement(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![
            row("alpha", true),
            row("beta", true),
            row("gamma", true),
            row("omega", true),
        ],
        1,
    );
    set_cursor(&shell, cx, 2);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 2]
    });
    let before = lists(&shell, cx);
    cx.simulate_keystrokes("delete");
    super::fileops_tests::reply(&shell, cx, r#"{"t":"trashed","ok":2,"failed":0}"#);
    assert!(
        lists(&shell, cx) > before,
        "own trash must anchor its reread"
    );
    land(
        &shell,
        cx,
        "/files",
        vec![row("beta", true), row("omega", true)],
        2,
    );
    assert_eq!(read(&shell, cx, "cursor", &[]), "0");
    assert_eq!(read(&shell, cx, "selectedIndices", &[]), "0");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_path_failed_trash_keeps_the_cursor_file(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("beta", true), row("gamma", true)],
        1,
    );
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("delete");
    super::fileops_tests::reply(
        &shell,
        cx,
        r#"{"t":"trashed","ok":0,"failed":1,"err":"Denied"}"#,
    );
    land(
        &shell,
        cx,
        "/files",
        vec![row("beta", true), row("gamma", true)],
        2,
    );
    assert_eq!(read(&shell, cx, "cursor", &[]), "1");
    assert_eq!(read(&shell, cx, "selectedIndices", &[]), "1");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_path_delete_reply_after_navigation_does_not_clear_the_new_folders_marks(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("beta", true)],
        1,
    );
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("delete");
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/alpha", vec![row("child", true)], 2);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0]
    });
    let before = lists(&shell, cx);
    super::fileops_tests::reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    assert_eq!(lists(&shell, cx), before);
    assert_eq!(read(&shell, cx, "selectedIndices", &[]), "0");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_path_permanent_delete_uses_its_reviewed_rows_anchor(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("beta", true), row("gamma", true)],
        1,
    );
    let mut sent = super::fileops_tests::wire(&shell, cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("shift-delete");
    let snapshot = sent();
    let id = flea::json::field_usize(&snapshot[0], "id").unwrap();
    super::fileops_tests::reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[16384]}}"#),
    );
    super::fileops_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"prepareDelete","id":{id},"ok":true,"token":77,"count":1,"bytes":12}}"#
        ),
    );
    cx.simulate_keystrokes("tab enter");
    assert!(sent().iter().any(|line| line.contains(r#""op":"delete""#)));
    super::fileops_tests::reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"delete","id":{id},"ok":true,"deleted":1,"failed":0,"cancelled":false,"error":"","remaining":[]}}"#
        ),
    );
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("gamma", true)],
        2,
    );
    assert_eq!(read(&shell, cx, "cursor", &[]), "1");
    assert_eq!(read(&shell, cx, "selectedIndices", &[]), "1");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
