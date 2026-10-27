//! Mounted menu and listing keys use simulated input and read back the window's state, not a private call.
use super::{
    menus_tests::{answer, choose, entry, reply, validated},
    test_shell::{asked, land, open_root, sent, status},
    Order,
};
use crate::window::input::ActionEvent;
use flea::{
    jsondoc::{self, Json},
    keys,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;
const LINK: u32 = 0o120777;

fn show_original(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> Json {
    cx.simulate_keystrokes("m");
    answer(shell, cx, &[LINK]);
    choose(shell, cx, "showOriginal");
    sent();
    validated(shell, cx, "showOriginal", &["/files/link"]);
    sent()
        .into_iter()
        .filter_map(|line| jsondoc::parse(&line).ok())
        .find(|line| line.get("c").and_then(Json::as_str) == Some("linktarget"))
        .expect("Show Original requests the validated symlink target")
}

fn link() -> flea::protocol::RowFacts {
    let mut row = entry("link", LINK);
    row.link = Some("/target/report.txt".into());
    row
}

#[gpui::test]
fn pb_menus_2_show_original_reveals_the_target_and_selects_it(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![link()], 1);
    let request = show_original(&shell, cx);
    let id = request.get("id").and_then(Json::as_f64).unwrap() as u64;
    assert_eq!(
        request.get("path").and_then(Json::as_str),
        Some("/files/link")
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","id":{id},"path":"/files/link","directory":"/target","name":"report.txt"}}"#
        ),
    );
    assert_eq!(asked(&shell, cx), "/target");
    land(
        &shell,
        cx,
        "/target",
        vec![entry("alpha.txt", FILE), entry("report.txt", FILE)],
        2,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        1
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_menus_2_show_original_ignores_foreign_and_late_replies_and_reports_missing(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![link()], 1);
    let request = show_original(&shell, cx);
    let id = request.get("id").and_then(Json::as_f64).unwrap() as u64;
    for (token, path) in [(id + 1, "/files/link"), (id, "/files/other")] {
        reply(
            &shell,
            cx,
            &format!(
                r#"{{"t":"linktarget","id":{token},"path":"{path}","directory":"/wrong","name":"wrong"}}"#
            ),
        );
        assert_eq!(asked(&shell, cx), "/files");
    }
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"linktarget","id":{id},"path":"/files/link","directory":"","name":""}}"#),
    );
    assert_eq!(
        status(&shell, cx),
        "That link points to an item that no longer exists."
    );
    assert_eq!(asked(&shell, cx), "/files");
    let request = show_original(&shell, cx);
    let id = request.get("id").and_then(Json::as_f64).unwrap() as u64;
    land(&shell, cx, "/elsewhere", vec![entry("next.txt", FILE)], 2);
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","id":{id},"path":"/files/link","directory":"/wrong","name":"wrong"}}"#
        ),
    );
    assert_eq!(asked(&shell, cx), "/elsewhere");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_menus_2_standalone_code_icons_action_writes_the_shared_setting(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![entry("notes.txt", FILE)], 1);
    for shown in [false, true] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.dispatch(
                    Order::Action(ActionEvent::bare("toggleLangMarks", keys::Context::Listing)),
                    window,
                    cx,
                )
            })
        });
        assert_eq!(
            shell.read_with(cx, |shell, _| shell
                .launch
                .settings
                .data
                .get("langMarks")
                .and_then(Json::as_bool)),
            Some(shown)
        );
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
