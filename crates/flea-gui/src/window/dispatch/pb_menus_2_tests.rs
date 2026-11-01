//! Mounted menu and listing keys use simulated input and read back the window's state, not a private call.
use super::{
    menus_tests::{answer, choose, entry, reply, validated},
    test_shell::{asked, land, open_root, sent, status},
    Order,
};
use crate::{window::input::ActionEvent, window::navigate::Step};
use flea::{
    jsondoc::{self, Json},
    keys,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;
const LINK: u32 = 0o120777;

// The footer line Show original explains with: menu_explain writes the footer, not the pane status.
fn footer(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> String {
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.shown().text.to_string())
    })
}

fn drawn(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().path.clone())
}

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
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, Step::Open("/elsewhere".into()), window, cx);
        })
    });
    land(&shell, cx, "/elsewhere", vec![entry("next.txt", FILE)], 2);
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","id":{id},"path":"/files/link","directory":"/wrong","name":"wrong"}}"#
        ),
    );
    assert_eq!(drawn(&shell, cx), "/elsewhere");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_menus_2_show_original_action_asks_for_the_cursor_link_target(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![entry("notes.txt", FILE)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(
                Order::Action(ActionEvent::bare("showOriginal", keys::Context::Listing)),
                window,
                cx,
            )
        })
    });
    assert_eq!(
        footer(&shell, cx),
        "Show original needs the cursor on a link"
    );
    land(&shell, cx, "/files", vec![link()], 2);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(
                Order::Action(ActionEvent::bare("showOriginal", keys::Context::Listing)),
                window,
                cx,
            )
        })
    });
    let request = sent()
        .into_iter()
        .filter_map(|line| jsondoc::parse(&line).ok())
        .find(|line| line.get("c").and_then(Json::as_str) == Some("linktarget"))
        .expect("the listing action requests the cursor link target");
    assert_eq!(
        request.get("path").and_then(Json::as_str),
        Some("/files/link")
    );
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

// EY-363: v0.3.12 Nav.js applyPendingSelect lands through setCursor, so the revealed target keeps three rows of context under it.
#[gpui::test]
fn show_original_keeps_three_rows_of_context_under_the_revealed_target(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![link()], 1);
    let request = show_original(&shell, cx);
    let id = request.get("id").and_then(Json::as_f64).unwrap() as u64;
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"linktarget","id":{id},"path":"/files/link","directory":"/target","name":"item-40"}}"#
        ),
    );
    let rows = (0..80)
        .map(|at| entry(&format!("item-{at:02}"), FILE))
        .collect();
    land(&shell, cx, "/target", rows, 2);
    let (cursor, scroll, row, height) = shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        (
            pane.cursor,
            pane.scroll_y,
            pane.row_height(),
            pane.viewport_height(),
        )
    });
    assert_eq!(cursor, 40);
    assert!(
        height < 40. * row,
        "the fixture overflows the window: {height}"
    );
    assert!(
        (cursor + 4) as f32 * row <= scroll + height + 0.01,
        "rows 41 to 43 stay whole under the target: scroll {scroll}, row {row}, height {height}"
    );
    assert!(
        cursor as f32 * row >= scroll + 3. * row - 0.01,
        "the target keeps its context above as well"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
