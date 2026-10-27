//! Tag completion and stale replies preserve the held listing and the next operation's identity.
use super::menus_tests::{answer, home, reply, settle, validated};
use super::tags_tests::{request, tagged_rows};
use super::test_shell::{land, open_root, sent, set_cursor, status};
use crate::{backend::Backend, state::Shell, window::u07_tests::read};
use flea::jsondoc::Json;
use gpui::{Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;

fn choose(shell: &Entity<Shell>, cx: &mut VisualTestContext, digit: &str) {
    cx.simulate_keystrokes("m");
    answer(shell, cx, &[FILE]);
    cx.simulate_keystrokes(digit);
    validated(
        shell,
        cx,
        &format!("tags:tag{digit}"),
        &["/files/notes.txt"],
    );
    request("settags");
}

#[gpui::test]
fn pb_tag_completion_reread_is_accepted_and_updates_the_next_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    choose(&shell, cx, "1");
    let (backend, mut windows) = Backend::test_channel();
    let original = shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend.replace(backend)
    });
    reply(&shell, cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    let (start, count) = windows().expect("completion asks for the held window");
    assert_eq!(start, 0);
    assert!(count >= 4);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = original
    });
    let mut rows = home();
    rows[2] = tagged_rows().remove(0);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                    start,
                    listing: 1,
                    rows,
                }),
                window,
                cx,
            );
        })
    });
    settle(cx);
    cx.simulate_keystrokes("T");
    answer(&shell, cx, &[FILE]);
    assert!(read(&shell, cx, "contextMenuSubmenuEntries", &[]).ends_with("|-|Remove All Tags"));
    assert!(read(&shell, cx, "contextMenuModel", &[]).contains(r#""state":"applied""#));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_late_completion_does_not_refresh_another_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    choose(&shell, cx, "1");
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/", home(), 2);
    let (backend, mut windows) = Backend::test_channel();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    reply(&shell, cx, r#"{"t":"settagged","ok":1,"op":"add"}"#);
    assert!(
        windows().is_none(),
        "an old completion must not read the new folder"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_refusal_retires_only_its_own_pending_change(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    choose(&shell, cx, "1");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Backend::test_reply_line(
                    r#"{"t":"error","where":"stale","path":"settags","msg":"folder changed"}"#,
                ),
                window,
                cx,
            )
        })
    });
    choose(&shell, cx, "5");
    reply(&shell, cx, r#"{"t":"settagged","ok":1,"op":"remove"}"#);
    assert_eq!(
        status(&shell, cx),
        "Removed Blue from 1 item \u{b7} z undoes"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_unsupported_filesystem_keeps_the_next_tag_identity(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    choose(&shell, cx, "1");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0,
        Backend::test_reply_line(r#"{"t":"error","where":"settags","path":"/files/notes.txt","msg":"This filesystem does not support file tags."}"#), window, cx)));
    choose(&shell, cx, "2");
    reply(
        &shell,
        cx,
        r#"{"t":"settagged","ok":0,"op":"add","skipped":["link"]}"#,
    );
    assert_eq!(status(&shell, cx), "1 item skipped");
    sent();
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_tag_old_selection_reads_cannot_reopen_or_rewrite_a_new_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let first = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .pending
            .as_ref()
            .unwrap()
            .first
    });
    let mut rows = home();
    rows.extend(
        (rows.len()..first).map(|at| super::menus_tests::entry(&format!("file-{at}.txt"), FILE)),
    );
    super::test_shell::land_on(&shell, cx, 0, "/files", rows, 1000, 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 2;
        host.selected = vec![2, 900];
    });
    cx.simulate_keystrokes("T");
    answer(&shell, cx, &[FILE, FILE]);
    let ask = request("tagstates");
    let Json::Num(id) = ask.get("id").unwrap() else {
        panic!("read id")
    };
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"tagstates","id":{id},"listing":2,"applied":["Red"],"mixed":[]}}"#),
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    cx.simulate_keystrokes("escape");
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"tagstates","id":{id},"listing":1,"applied":["Red"],"mixed":[]}}"#),
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE, FILE]);
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"tagstates","id":{id},"listing":1,"applied":["Red"],"mixed":[]}}"#),
    );
    assert!(!read(&shell, cx, "contextMenuModel", &[]).contains(r#""state":"applied""#));
    super::tags_tests::states(&shell, cx, "\"Blue\"", "");
    assert!(read(&shell, cx, "contextMenuModel", &[]).contains(r#""state":"applied""#));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
