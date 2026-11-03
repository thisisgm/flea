//! Trash is a covering place: navigation dismisses it before the destination is shown.
use super::{
    menus_tests::settle,
    test_shell::{asked, land, lists, open_root_with, row, sent},
};
use crate::{
    backend::Event as BackendEvent,
    state::Shell,
    window::{
        navigate::Step,
        u07_tests::{press, read, reply},
    },
};
use flea::jsondoc::{self, Json};
use gpui::{Entity, TestAppContext, VisualTestContext};

type Scene<'a> = (Entity<Shell>, &'a mut VisualTestContext);

fn open<'a>(cx: &'a mut TestAppContext, view: &str) -> Scene<'a> {
    let (shell, cx) = open_root_with(cx, &format!(r#""keys":"default","view":"{view}""#), None);
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    shell.update(cx, |shell, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
    });
    rail(&shell, cx, "Trash");
    answer(
        &shell,
        cx,
        "list",
        r#", "total":1,"start":0,"rows":[{"uri":"trash:///notes.txt","original":"/files/notes.txt","identity":"notes","size":13}]"#,
    );
    answer(&shell, cx, "summary", r#", "bytes":13,"partial":false"#);
    settle(cx);
    assert_eq!(state(&shell, cx).get("opened"), Some(&Json::Bool(true)));
    (shell, cx)
}

fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, reader: &str, arg: &str) {
    let at = read(shell, cx, reader, &[arg]);
    assert_eq!(at.split_whitespace().count(), 2, "{reader} {arg}: {at}");
    press(cx, &at, 1);
    settle(cx);
}

fn rail(shell: &Entity<Shell>, cx: &mut VisualTestContext, label: &str) {
    let labels = read(shell, cx, "railLabels", &[]);
    // Sample input: "Home|Downloads|Trash".
    let index = labels
        .split('|')
        .position(|name| name == label)
        .unwrap_or_else(|| panic!("missing rail row {label}: {labels}"));
    click(shell, cx, "railRowCentre", &index.to_string());
}

fn home() -> String {
    std::env::var("HOME").unwrap()
}

fn answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, op: &str, fields: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.trash.as_ref().unwrap();
            let id = host.pane.read(cx).view().request_id;
            shell.receive(
                host.backend_side,
                BackendEvent::TrashLine(format!(
                    r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true{fields}}}"#
                )),
                window,
                cx,
            );
        })
    });
    settle(cx);
}

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "trashState", &[])).unwrap()
}

fn closed(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str) {
    assert_eq!(
        state(shell, cx).get("opened"),
        Some(&Json::Bool(false)),
        "navigation must leave Trash"
    );
    assert_eq!(asked(shell, cx), path);
}

#[gpui::test]
fn trash_navigation_sidebar_place_replaces_trash_in_every_view(cx: &mut TestAppContext) {
    for view in ["list", "columns", "grid"] {
        let (shell, cx) = open(cx, view);
        rail(&shell, cx, "Home");
        assert_eq!(asked(&shell, cx), home(), "the place really navigated");
        land(&shell, cx, &home(), vec![row("destination.txt", false)], 2);
        closed(&shell, cx, &home());
        assert_eq!(read(&shell, cx, "path", &[]), home());
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn trash_navigation_empty_then_sidebar_place_replaces_the_empty_view(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "columns");
    click(&shell, cx, "trashControlCentre", "empty");
    answer(
        &shell,
        cx,
        "prepare",
        r#", "token":5,"count":1,"bytes":13,"all":true"#,
    );
    assert_eq!(
        state(&shell, cx).get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(true))
    );
    sent();
    click(&shell, cx, "trashControlCentre", "danger");
    assert!(
        sent()
            .iter()
            .any(|line| line.contains(r#""op":"delete""#) && line.contains(r#""token":5"#)),
        "the confirmation really deletes"
    );
    answer(
        &shell,
        cx,
        "delete",
        r#", "done":1,"failed":0,"failures":[]"#,
    );
    answer(&shell, cx, "list", r#", "total":0,"start":0,"rows":[]"#);
    answer(&shell, cx, "summary", r#", "bytes":0,"partial":false"#);
    assert_eq!(
        state(&shell, cx).get("total").and_then(Json::as_f64),
        Some(0.)
    );
    assert_eq!(
        state(&shell, cx).get("operationActive"),
        Some(&Json::Bool(false))
    );
    assert_eq!(state(&shell, cx).get("busy"), Some(&Json::Bool(false)));
    rail(&shell, cx, "Home");
    assert_eq!(asked(&shell, cx), home());
    land(&shell, cx, &home(), vec![row("destination.txt", false)], 2);
    closed(&shell, cx, &home());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn trash_navigation_history_back_and_parent_only_close_the_cover(cx: &mut TestAppContext) {
    for action in ["historyBack", "parent"] {
        let (shell, cx) = open(cx, "list");
        let before = lists(&shell, cx);
        read(&shell, cx, "runAction", &[action]);
        settle(cx);
        closed(&shell, cx, "/files");
        assert_eq!(
            lists(&shell, cx),
            before,
            "{action} must leave the underlying history alone"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn trash_navigation_escape_key_and_ipc_close_the_cover(cx: &mut TestAppContext) {
    for ipc in [false, true] {
        let (shell, cx) = open(cx, "list");
        let before = lists(&shell, cx);
        if ipc {
            read(&shell, cx, "runAction", &["escape"]);
        } else {
            cx.simulate_keystrokes("escape");
        }
        settle(cx);
        closed(&shell, cx, "/files");
        assert_eq!(lists(&shell, cx), before);
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn trash_navigation_typed_path_leaves_the_cover(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "columns");
    cx.simulate_keystrokes("ctrl-l ctrl-a");
    cx.simulate_input("/target");
    cx.simulate_keystrokes("enter");
    settle(cx);
    land(
        &shell,
        cx,
        "/target",
        vec![row("destination.txt", false)],
        2,
    );
    closed(&shell, cx, "/target");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn trash_navigation_confirmation_keeps_trash_until_cancel(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "list");
    click(&shell, cx, "trashControlCentre", "empty");
    answer(
        &shell,
        cx,
        "prepare",
        r#", "token":5,"count":1,"bytes":13,"all":true"#,
    );
    assert_eq!(
        reply(&shell, cx, "runAction", &["historyBack"]).code,
        flea::keys::actions::UNAVAILABLE
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::OpenPlace("/target".into()), window, cx),
                Ok(false)
            );
        })
    });
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(state(&shell, cx).get("opened"), Some(&Json::Bool(true)));
    assert_eq!(
        state(&shell, cx).get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(true))
    );
    click(&shell, cx, "trashControlCentre", "cancel");
    answer(&shell, cx, "cancel", "");
    answer(
        &shell,
        cx,
        "list",
        r#", "total":1,"start":0,"rows":[{"uri":"trash:///notes.txt","original":"/files/notes.txt","identity":"notes","size":13}]"#,
    );
    answer(&shell, cx, "summary", r#", "bytes":13,"partial":false"#);
    rail(&shell, cx, "Home");
    land(&shell, cx, &home(), vec![row("destination.txt", false)], 2);
    closed(&shell, cx, &home());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
