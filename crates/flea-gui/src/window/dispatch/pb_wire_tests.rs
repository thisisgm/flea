//! Backend replies are read back through the mounted window after a real menu choice.
use super::{
    menus_tests::{answer, choose, entry, validated},
    test_shell::{asked, land, open_root_with, sent, status},
};
use crate::{backend::Event, state::Shell};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn receive(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Event::Producer(line.into()), window, cx);
        })
    });
    cx.run_until_parked();
}

fn link_menu(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, usize) {
    let (shell, cx) = open_root_with(
        cx,
        r#""view":"list","keys":"default","hiddenActions":[]"#,
        None,
    );
    let mut link = entry("shortcut", 0o120777);
    link.link = Some("/target/note.txt".into());
    land(&shell, cx, "/files", vec![link], 1);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o120777]);
    choose(&shell, cx, "showOriginal");
    validated(&shell, cx, "showOriginal", &["/files/shortcut"]);
    let requests: Vec<Json> = sent()
        .iter()
        .filter_map(|line| jsondoc::parse(line).ok())
        .collect();
    let request = requests
        .iter()
        .find(|doc| doc.get("c").and_then(Json::as_str) == Some("linktarget"))
        .expect("Show Original must ask the backend to resolve the link");
    assert_eq!(
        request.get("path").and_then(Json::as_str),
        Some("/files/shortcut")
    );
    let id = request.get("id").and_then(Json::as_f64).unwrap() as usize;
    (shell, cx, id)
}

#[gpui::test]
fn pb_wire_show_original_reports_a_missing_target(cx: &mut TestAppContext) {
    let (shell, cx, id) = link_menu(cx);
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","path":"/files/shortcut","directory":"","name":"","id":{id}}}"#
        ),
    );
    assert_eq!(
        status(&shell, cx),
        "That link points to an item that no longer exists."
    );
    assert_eq!(asked(&shell, cx), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_wire_show_original_reveals_only_its_own_reply(cx: &mut TestAppContext) {
    let (shell, cx, id) = link_menu(cx);
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","path":"/files/shortcut","directory":"/foreign","name":"wrong","id":{}}}"#,
            id + 1
        ),
    );
    assert_eq!(asked(&shell, cx), "/files");
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","path":"/files/shortcut","directory":"/target","name":"note.txt","id":{id}}}"#
        ),
    );
    assert_eq!(asked(&shell, cx), "/target");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .pending_select
            .clone()),
        Some("/target/note.txt".into())
    );
    land(&shell, cx, "/target", vec![entry("note.txt", 0o100644)], 2);
    assert_eq!(
        crate::window::u07_tests::read(&shell, cx, "rowNameText", &["0"]),
        "note.txt"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        [0]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_wire_show_original_late_reply_keeps_the_new_folder(cx: &mut TestAppContext) {
    let (shell, cx, id) = link_menu(cx);
    cx.simulate_keystrokes("backspace");
    assert_eq!(asked(&shell, cx), "/");
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","path":"/files/shortcut","directory":"/target","name":"note.txt","id":{id}}}"#
        ),
    );
    assert_eq!(asked(&shell, cx), "/");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_wire_unmount_moves_to_parent_without_adding_history(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","keys":"default""#, None);
    land(&shell, cx, "/files", vec![entry("note", 0o100644)], 1);
    let history = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.history.back.clone()
    });
    receive(
        &shell,
        cx,
        r#"{"t":"unmounted","path":"/other","parent":"/"}"#,
    );
    assert_eq!(asked(&shell, cx), "/files");
    receive(
        &shell,
        cx,
        r#"{"t":"unmounted","path":"/files","parent":"/"}"#,
    );
    assert_eq!(asked(&shell, cx), "/");
    assert_eq!(
        status(&shell, cx),
        "That drive or share was disconnected, so this folder is no longer available"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .history
            .back
            .clone()),
        history
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_wire_unmount_of_an_ancestor_supersedes_a_pending_listing(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","keys":"default""#, None);
    land(&shell, cx, "/files", vec![entry("sub", 0o040755)], 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(asked(&shell, cx), "/files/sub");
    receive(
        &shell,
        cx,
        r#"{"t":"unmounted","path":"/files","parent":"/"}"#,
    );
    assert_eq!(asked(&shell, cx), "/");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
