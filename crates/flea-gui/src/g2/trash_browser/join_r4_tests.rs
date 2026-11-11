//! Independent FIX3 checks served within one production-shaped update, before redraw.
use super::join_tests::{answer, call, draw, opened, root, shell};
use crate::{ipc::server::Event, state::Shell};
use flea::{
    ipc::wire::Request,
    jsondoc::{self, Json},
};
use gpui::{Context, Window};

fn read(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>, name: &str) -> String {
    let (answer, reply) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: if name == "trashRowCentre" {
                    vec!["0".into()]
                } else {
                    vec![]
                },
            },
            answer,
        },
        window,
        cx,
    );
    let reply = reply.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0);
    reply.stdout.trim_end().to_string()
}

#[test]
fn review_u32_r4_identical_reopen_must_not_reuse_a_hidden_frame() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let original = call(&mut cx, window, "trashRowCentre", &["0"]);
    assert!(!original.is_empty());
    shell(&mut cx, window, |shell, window, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.back_pressed(cx));
        shell.flush_trash(window, cx);
        assert!(shell.slots.covers[0].is_none());
    });
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        assert!(shell.trash_action("openTrash", window, cx));
        assert!(shell.slots.covers[0].is_none());
    });
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","size":6},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity","size":5}]"#,
    );
    let (raw, centre) = shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        let id = host.pane.read(cx).view().request_id;
        let side = host.side;
        let before = {
            let paint = host.pane.read(cx).paint.borrow();
            (paint.committed, paint.laid_out)
        };
        shell.receive(side, crate::backend::Event::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"summary","ok":true,"bytes":11,"partial":false}}"#)), window, cx);
        shell.flush_trash(window, cx);
        let raw = read(shell, window, cx, "trashState");
        let centre = read(shell, window, cx, "trashRowCentre");
        let paint = shell.trash.as_ref().unwrap().pane.read(cx).paint.borrow();
        eprintln!("R4 identical reopen before frame: stamp {:?} -> ({}, {:?}), centre={centre:?}, state={raw}", before, paint.committed, paint.laid_out);
        (raw, centre)
    });
    let state = jsondoc::parse(&raw).unwrap();
    assert_eq!(
        state.get("busy"),
        Some(&Json::Bool(true)),
        "an identical reopen has not drawn its new Trash frame"
    );
    assert!(
        centre.is_empty(),
        "a hidden previous opening supplies no currently drawn centre"
    );
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "trashRowCentre", &["0"]), original);
}

#[test]
fn review_u32_r4_scroll_changes_geometry_and_waits_for_the_next_frame() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
        shell.trash_action("openTrash", window, cx);
    });
    let rows: Vec<_> = (0..100).map(|i| format!(r#"{{"uri":"trash:///row-{i}","original":"/private/row-{i}.txt","identity":"identity-{i}"}}"#)).collect();
    let present: Vec<_> = (0..100).map(|i| format!(r#""trash:///row-{i}""#)).collect();
    answer(
        &mut cx,
        window,
        "list",
        &format!(
            r#", "total":100,"start":0,"present":[{}],"rows":[{}]"#,
            present.join(","),
            rows.join(",")
        ),
    );
    answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
    draw(&mut cx, window);
    let original = call(&mut cx, window, "trashRowCentre", &["0"]);
    assert!(!original.is_empty());
    let (raw, centre) = shell(&mut cx, window, |shell, window, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.wheel(9., cx));
        let paint = shell.trash.as_ref().unwrap().pane.read(cx).paint.borrow();
        assert_eq!(paint.scroll_y, 9.);
        assert!(paint
            .laid_out
            .is_some_and(|laid_out| paint.committed > laid_out));
        drop(paint);
        (
            read(shell, window, cx, "trashState"),
            read(shell, window, cx, "trashRowCentre"),
        )
    });
    assert_eq!(
        jsondoc::parse(&raw).unwrap().get("busy"),
        Some(&Json::Bool(true))
    );
    assert!(centre.is_empty());
    draw(&mut cx, window);
    let after = call(&mut cx, window, "trashRowCentre", &["0"]);
    let y = |s: &str| s.split_whitespace().nth(1).unwrap().parse::<f32>().unwrap();
    assert_eq!(y(&original) - y(&after), 9.);
    assert_eq!(
        super::join_tests::state(&mut cx, window).get("busy"),
        Some(&Json::Bool(false))
    );
}

#[test]
fn review_u32_r4_all_entry_actions_and_close_paths_drop_the_drawn_surface() {
    for flush_close in [false, true] {
        for (close, action) in [
            ("back_pressed", "openTrash"),
            ("close", "emptyTrash"),
            ("parent", "restoreAll"),
            ("back", "restoreTrashSelection"),
            ("escape", "deleteTrashSelection"),
        ] {
            let mut cx = crate::list::bench::real_cx();
            let window = root(&mut cx);
            opened(&mut cx, window);
            shell(&mut cx, window, |shell, window, cx| {
                let pane = shell.trash.as_ref().unwrap().pane.clone();
                let committed = pane.read(cx).paint.borrow().committed;
                pane.update(cx, |pane, cx| match close {
                    "back_pressed" => pane.back_pressed(cx),
                    "close" => pane.close(cx),
                    key => pane.action(key, false, cx),
                });
                if flush_close {
                    shell.flush_trash(window, cx);
                    assert!(shell.slots.covers[0].is_none(), "{close}");
                    assert!(!pane.read(cx).paint.borrow().drawn(), "{close}");
                } else {
                    assert!(pane.read(cx).paint.borrow().drawn());
                }
                assert_eq!(pane.read(cx).paint.borrow().committed, committed);
                assert!(shell.trash_action(action, window, cx));
                assert!(!pane.read(cx).paint.borrow().drawn(), "{action}");
                assert_eq!(pane.read(cx).paint.borrow().committed, committed);
                assert_eq!(
                    jsondoc::parse(&read(shell, window, cx, "trashState"))
                        .unwrap()
                        .get("busy"),
                    Some(&Json::Bool(true)),
                    "{action}"
                );
                assert!(read(shell, window, cx, "trashRowCentre").is_empty());
            });
        }
    }
}

#[test]
fn review_u32_r4_a_refresh_keeps_its_rows_and_changed_rows_need_a_new_prepaint() {
    use std::time::Duration;

    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let original = call(&mut cx, window, "trashRowCentre", &["0"]);
    shell(&mut cx, window, |shell, window, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            pane.action("reload", false, cx);
        });
        shell.flush_trash(window, cx);
        let host = shell.trash.as_ref().unwrap();
        assert!(
            !host.presentation.held_trash,
            "a refresh over drawn rows opens no hold"
        );
        assert!(host.pane.read(cx).paint.borrow().drawn());
    });
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::CAP_MS + 100));
    cx.run_until_parked();
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        assert_eq!(host.presentation.stage, super::presentation::Stage::Ready);
        assert!(
            host.pane.read(cx).paint.borrow().drawn(),
            "the drawn rows stay through a slow refresh"
        );
        assert_eq!(
            jsondoc::parse(&read(shell, window, cx, "trashState"))
                .unwrap()
                .get("busy"),
            Some(&Json::Bool(true))
        );
    });
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":3,"start":0,"present":["trash:///gamma","trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///gamma","original":"/private/gamma.txt","identity":"gamma-identity","size":4},{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","size":6},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity","size":5}]"#,
    );
    shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        let id = host.pane.read(cx).view().request_id;
        let side = host.side;
        shell.receive(side, crate::backend::Event::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"summary","ok":true,"bytes":11,"partial":false}}"#)), window, cx);
        shell.flush_trash(window, cx);
        assert_eq!(
            shell.trash.as_ref().unwrap().presentation.stage,
            super::presentation::Stage::Ready
        );
        assert_eq!(
            jsondoc::parse(&read(shell, window, cx, "trashState"))
                .unwrap()
                .get("busy"),
            Some(&Json::Bool(true))
        );
        assert!(read(shell, window, cx, "trashRowCentre").is_empty());
    });
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "trashRowCentre", &["0"]), original);
    assert_eq!(
        super::join_tests::state(&mut cx, window).get("busy"),
        Some(&Json::Bool(false))
    );
}
