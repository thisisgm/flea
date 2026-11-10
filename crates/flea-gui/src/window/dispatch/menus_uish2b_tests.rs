use super::super::test_shell::{self, set_cursor};
use super::*;

#[gpui::test]
fn fxuish2b_menu_rename_opens_before_validation_and_keeps_snapshot_for_commit(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    let id = captured(&shell, cx);
    let _ = test_shell::sent();
    choose(&shell, cx, "rename");
    assert_eq!(read(&shell, cx, "renameEditorLive", &[]), "true");
    assert!(test_shell::sent()
        .iter()
        .all(|line| !line.contains("\"op\":\"validate\"") && !line.contains("\"op\":\"close\"")));
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    let requests = test_shell::sent();
    assert!(
        requests.iter().any(|line| line.contains("\"c\":\"rename\"")
            && flea::json::field_usize(line, "menuId") == Some(id as usize)),
        "{requests:?}"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "rename".into(),
                    path: "/files/notes.txt".into(),
                    message: "Selected item changed; reopen the menu.".into(),
                    mode: 0,
                },
                window,
                cx,
            )
        })
    });
    assert_eq!(read(&shell, cx, "renameEditorLive", &[]), "true");
    assert!(read(&shell, cx, "renameState", &[]).contains("Selected item changed"));
    cx.simulate_keystrokes("escape");
    assert_eq!(read(&shell, cx, "renameEditorLive", &[]), "false");
    assert!(test_shell::sent()
        .iter()
        .any(|line| line.contains("\"op\":\"close\"")
            && flea::json::field_usize(line, "id") == Some(id as usize)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fxuish2b_menu_rename_that_cannot_open_closes_its_snapshot(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    let id = captured(&shell, cx);
    // The captured row has left the held window, so no editor can take the snapshot.
    shell.update(cx, |shell, _| {
        shell.menu.captured.as_mut().unwrap().cursor = 99
    });
    let _ = test_shell::sent();
    choose(&shell, cx, "rename");
    assert_eq!(read(&shell, cx, "renameEditorLive", &[]), "false");
    assert!(test_shell::sent()
        .iter()
        .any(|line| line.contains("\"op\":\"close\"")
            && flea::json::field_usize(line, "id") == Some(id as usize)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
