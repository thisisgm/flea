//! Created neighbour results select their own folder only while the originating pane state remains current.
use crate::{
    backend::Backend,
    state::Shell,
    window::{
        dispatch::{
            fileops_menu_tests::opened,
            fileops_tests::reply,
            test_shell::{land, row, set_cursor},
            Order,
        },
        input::ActionEvent,
        navigate::Step,
    },
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn asked(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> impl FnMut() -> Vec<String> {
    let (backend, mut read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    move || std::iter::from_fn(&mut read).collect()
}

#[gpui::test]
fn ey379_failed_creation_keeps_another_pending_origin(cx: &mut TestAppContext) {
    for kind in ["made", "duplicated"] {
        for fail_first in [true, false] {
            let (shell, mut view) = opened(cx);
            let cx = &mut view;
            let mut sent = asked(&shell, cx);
            let (first, second) = shell.update(cx, |shell, cx| {
                (
                    shell
                        .fileops_creation_requested(0, kind, "/parent", cx)
                        .unwrap(),
                    shell
                        .fileops_creation_requested(0, kind, "/parent", cx)
                        .unwrap(),
                )
            });
            let (failed, succeeded) = if fail_first {
                (first, second)
            } else {
                (second, first)
            };
            reply(&shell, cx, &creation(kind, failed, false, ""));
            reply(
                &shell,
                cx,
                &creation(kind, succeeded, true, "/parent/created"),
            );
            assert!(
                sent()
                    .iter()
                    .any(
                        |line| flea::json::field_str(line, "c").as_deref() == Some("list")
                            && flea::json::field_str(line, "path").as_deref() == Some("/parent")
                    ),
                "a failed request cannot erase another creation's origin"
            );
            shell.update(cx, |shell, _| shell.stop_for_app_quit());
        }
    }
}

fn creation(kind: &str, request: u64, ok: bool, path: &str) -> String {
    format!(
        r#"{{"t":"menuaction","op":"creation","kind":"{kind}","id":{request},"ok":{ok},"path":"{path}"}}"#
    )
}

#[gpui::test]
fn ey379_failed_sibling_cannot_bypass_stale_creation_guard(cx: &mut TestAppContext) {
    let (shell, mut view) = opened(cx);
    let cx = &mut view;
    let mut sent = asked(&shell, cx);
    let (first, second) = shell.update(cx, |shell, cx| {
        (
            shell
                .fileops_creation_requested(0, "made", "/parent", cx)
                .unwrap(),
            shell
                .fileops_creation_requested(0, "made", "/parent", cx)
                .unwrap(),
        )
    });
    reply(&shell, cx, &creation("made", first, false, ""));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::OpenPlace("/parent".into()), window, cx),
                Ok(true)
            );
        })
    });
    land(
        &shell,
        cx,
        "/parent",
        vec![row("keep", false), row("later", true)],
        2,
    );
    let _ = sent();
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"made","request":{second},"ok":true,"path":"/parent/later"}}"#),
    );
    reply(&shell, cx, &creation("made", second, true, "/parent/later"));
    assert!(!sent()
        .iter()
        .any(|line| flea::json::field_str(line, "c").as_deref() == Some("list")));
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
        assert!(shell.operations.fileops.arrival[0].is_none());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey379_creation_token_preserves_independent_mkdir_fields(cx: &mut TestAppContext) {
    let (shell, mut view) = opened(cx);
    let line = shell.update(&mut view, |shell, cx| shell.fileops_creation_request(0,
        r#"{"c":"mkdir","path":"/parent","defaultBase":"Nouveau dossier","name":"exact","independent":7}"#.into(), cx).unwrap());
    assert_eq!(
        flea::json::field_str(&line, "defaultBase").as_deref(),
        Some("Nouveau dossier")
    );
    assert_eq!(
        flea::json::field_str(&line, "name").as_deref(),
        Some("exact")
    );
    assert_eq!(flea::json::field_usize(&line, "independent"), Some(7));
    assert!(flea::json::field_usize(&line, "request").unwrap() > 0);
    shell.update(&mut view, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey379_created_neighbour_item_adopts_its_parent_and_selects_the_result(cx: &mut TestAppContext) {
    for kind in ["newFile", "made", "duplicated"] {
        let (shell, mut view) = opened(cx);
        let cx = &mut view;
        let mut sent = asked(&shell, cx);
        let target = "/parent/created.txt";
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let request = shell
                    .fileops_creation_requested(0, kind, "/parent", cx)
                    .unwrap();
                if kind == "newFile" {
                    shell.new_file_created(0, target.into(), window, cx);
                } else {
                    assert!(shell.fileops_line(
                        0,
                        &creation(kind, request, true, target),
                        window,
                        cx,
                    ));
                }
            })
        });
        let requests = sent();
        assert!(
            requests.iter().any(|line| {
                flea::json::field_str(line, "c").as_deref() == Some("list")
                    && flea::json::field_str(line, "path").as_deref() == Some("/parent")
            }),
            "{kind} must read the result's parent: {requests:?}"
        );
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0]
                .as_ref()
                .unwrap()
                .nav
                .pending_select
                .clone()),
            Some(target.into()),
        );
        land(
            &shell,
            cx,
            "/parent",
            vec![row("alpha", false), row("created.txt", kind == "made")],
            2,
        );
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.path(), "/parent");
            assert_eq!(
                host.pane.cursor, 1,
                "{kind} selects the created neighbour item"
            );
        });
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn ey379_delayed_creation_result_preserves_later_navigation(cx: &mut TestAppContext) {
    for (action, command, reply_kind, target) in [
        (
            "duplicate",
            "duplicate",
            "duplicated",
            "/files/notes copy.txt",
        ),
        ("newFolder", "mkdir", "made", "/files/New Folder"),
    ] {
        let (shell, mut view) = opened(cx);
        let cx = &mut view;
        let mut sent = asked(&shell, cx);
        set_cursor(&shell, cx, 2);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.dispatch(
                    Order::Action(ActionEvent::bare(action, flea::keys::Context::Listing)),
                    window,
                    cx,
                );
            })
        });
        let request = sent()
            .iter()
            .find(|line| flea::json::field_str(line, "c").as_deref() == Some(command))
            .and_then(|line| flea::json::field_usize(line, "request"))
            .expect("actual outgoing creation token") as u64;
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                assert_eq!(
                    shell.step(0, Step::OpenPlace("/elsewhere".into()), window, cx),
                    Ok(true)
                );
            })
        });
        land(&shell, cx, "/elsewhere", vec![row("keep.txt", false)], 2);
        let _ = sent();
        reply(&shell, cx, &creation(reply_kind, request, true, target));
        let requests = sent();
        assert!(
            !requests
                .iter()
                .any(|line| flea::json::field_str(line, "c").as_deref() == Some("list")),
            "a late result cannot navigate or refresh the new listing: {requests:?}"
        );
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.path, "/elsewhere");
            assert_eq!(host.pane.cursor, 0);
            assert!(
                shell.operations.fileops.arrival[0].is_none(),
                "a stale result cannot arm Rename"
            );
        });
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}
