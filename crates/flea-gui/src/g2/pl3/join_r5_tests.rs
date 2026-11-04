//! Round 5 probes for the remaining focus and composition lifetime boundaries.
use super::join_tests::{answer, call, draw, opened, root, shell, state};
use crate::{ipc::server::Event, state::Shell, window::startup::View};
use flea::{
    ipc::wire::Request,
    jsondoc::{self, Json},
};
use gpui::{Context, Window};
use std::rc::Rc;

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

fn right_pane(shell: &mut Shell) {
    use crate::window::pane::PaneHost;
    use flea::protocol::{Reply, RowFacts};
    let mut host = PaneHost::new("/right".into(), None, 500., &shell.locale);
    host.pane
        .receive(Reply::Listed {
            path: "/right".into(),
            total: 100,
        })
        .unwrap();
    host.pane
        .receive(Reply::Rows {
            start: 0,
            rows: vec![RowFacts::default(); 100],
            listing: 3,
        })
        .unwrap();
    let (backend, receiver) = crate::backend::Backend::test_channel();
    host.backend = Some(backend);
    std::mem::forget(receiver);
    shell.panes[1] = Some(host);
    shell.view = View::Dual;
    shell.lifecycle.attach(1);
}

#[test]
fn review_u32_r5_focus_only_and_unchanged_pumps_keep_the_drawn_surface() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| right_pane(shell));
    draw(&mut cx, window);
    let original = call(&mut cx, window, "trashRowCentre", &["0"]);
    assert!(!original.is_empty());
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        let paint = pane.read(cx).paint.clone();
        let committed = paint.borrow().committed;
        for n in 0..20 {
            shell.focus_pane(n % 2, cx);
            let focus = if n % 2 == 0 {
                pane.read(cx).focus.clone()
            } else {
                shell.focus.clone()
            };
            window.focus(&focus, cx);
            pane.update(cx, |pane, cx| pane.pump(cx));
            shell.flush_trash(window, cx);
            assert!(Rc::ptr_eq(&paint, &pane.read(cx).paint));
            assert_eq!(paint.borrow().committed, committed);
            assert!(paint.borrow().drawn());
            assert_eq!(
                jsondoc::parse(&read(shell, window, cx, "trashState"))
                    .unwrap()
                    .get("busy"),
                Some(&Json::Bool(false))
            );
            assert_eq!(read(shell, window, cx, "trashRowCentre"), original);
        }
    });
}

#[test]
fn review_u32_r5_single_side_view_changes_retain_one_real_list() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let paint = shell(&mut cx, window, |shell, _, cx| {
        shell.trash.as_ref().unwrap().pane.read(cx).paint.clone()
    });
    for view in [View::Grid, View::Columns, View::List] {
        shell(&mut cx, window, |shell, window, cx| {
            shell.set_view(view, window, cx)
        });
        draw(&mut cx, window);
        assert_eq!(state(&mut cx, window).get("busy"), Some(&Json::Bool(false)));
        assert!(!call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
        shell(&mut cx, window, |shell, _, cx| {
            assert_eq!(shell.slots.covers.iter().flatten().count(), 1);
            assert!(Rc::ptr_eq(
                &paint,
                &shell.trash.as_ref().unwrap().pane.read(cx).paint
            ));
        });
    }
}

#[test]
fn review_u32_r5_right_trash_hidden_by_single_view_has_no_drawn_centre() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, cx| {
        right_pane(shell);
        shell.focus_pane(1, cx);
    });
    opened(&mut cx, window);
    let original = call(&mut cx, window, "trashRowCentre", &["0"]);
    assert!(!original.is_empty());
    shell(&mut cx, window, |shell, window, cx| {
        shell.focus_pane(0, cx);
        window.focus(&shell.focus, cx);
    });
    cx.simulate_keystrokes(window.into(), "ctrl-1");
    shell(&mut cx, window, |shell, _, _| {
        assert_eq!(shell.view, View::List)
    });
    draw(&mut cx, window);
    let raw = call(&mut cx, window, "trashState", &[]);
    let centre = call(&mut cx, window, "trashRowCentre", &["0"]);
    shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        let paint = host.pane.read(cx).paint.borrow();
        eprintln!("R5 right side hidden: view={:?}, host_side={}, viewport={:?}, slot={:?}, bounds={:?}, drawn={}, centre={centre:?}, state={raw}",shell.view,host.side,window.viewport_size(),shell.geometry(window).panes[host.side],paint.bounds,paint.drawn());
    });
    let (revealed, revealed_centre) = shell(&mut cx, window, |shell, window, cx| {
        shell.set_view(View::Dual, window, cx);
        let raw = read(shell, window, cx, "trashState");
        let centre = read(shell, window, cx, "trashRowCentre");
        eprintln!(
            "R5 reveal before its frame: centre={centre:?}, busy={:?}",
            jsondoc::parse(&raw).unwrap().get("busy")
        );
        (raw, centre)
    });
    draw(&mut cx, window);
    assert_eq!(
        call(&mut cx, window, "trashRowCentre", &["0"]),
        original,
        "visible-frame control restores the correct centre"
    );
    assert!(
        centre.is_empty(),
        "a right Trash collapsed to zero width is not a clickable surface"
    );
    assert!(
        revealed_centre.is_empty(),
        "revealing the right pane has not drawn its new frame"
    );
    assert_eq!(
        jsondoc::parse(&revealed).unwrap().get("busy"),
        Some(&Json::Bool(true))
    );
    assert_eq!(
        jsondoc::parse(&raw).unwrap().get("busy"),
        Some(&Json::Bool(true)),
        "an opened hidden Trash remains undrawn"
    );
}

#[test]
fn review_u32_r5_reopen_on_another_pane_keeps_the_same_list_and_one_cover() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let paint = shell(&mut cx, window, |shell, _, cx| {
        shell.trash.as_ref().unwrap().pane.read(cx).paint.clone()
    });
    shell(&mut cx, window, |shell, window, cx| {
        right_pane(shell);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.close(cx));
        shell.flush_trash(window, cx);
        shell.focus_pane(1, cx);
        shell.trash_action("openTrash", window, cx);
        assert_eq!(shell.slots.covers.iter().flatten().count(), 0);
        assert!(Rc::ptr_eq(
            &paint,
            &shell.trash.as_ref().unwrap().pane.read(cx).paint
        ));
        assert!(!paint.borrow().drawn());
        assert!(read(shell, window, cx, "trashRowCentre").is_empty());
    });
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","size":6},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity","size":5}]"#,
    );
    answer(
        &mut cx,
        window,
        "summary",
        r#", "bytes":11,"partial":false"#,
    );
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        assert!(shell.slots.covers[0].is_none());
        assert!(shell.slots.covers[1].is_some());
        assert_eq!(shell.slots.covers.iter().flatten().count(), 1);
        assert!(Rc::ptr_eq(
            &paint,
            &shell.trash.as_ref().unwrap().pane.read(cx).paint
        ));
        assert!(paint.borrow().drawn());
    });
    assert_eq!(state(&mut cx, window).get("busy"), Some(&Json::Bool(false)));
    assert!(!call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
}

#[test]
fn review_u32_r5_hide_and_reveal_in_one_update_waits_for_a_visible_frame() {
    let mut cx = crate::list::bench::real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, cx| {
        right_pane(shell);
        shell.focus_pane(1, cx);
    });
    opened(&mut cx, window);
    let original = call(&mut cx, window, "trashRowCentre", &["0"]);
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        let paint = pane.read(cx).paint.clone();
        let committed = paint.borrow().committed;
        for view in [View::List, View::Dual] {
            shell.set_view(view, window, cx);
            assert!(Rc::ptr_eq(&paint, &pane.read(cx).paint));
            assert_eq!(paint.borrow().committed, committed);
            assert!(!paint.borrow().drawn());
            assert!(read(shell, window, cx, "trashRowCentre").is_empty());
            assert_eq!(
                jsondoc::parse(&read(shell, window, cx, "trashState"))
                    .unwrap()
                    .get("busy"),
                Some(&Json::Bool(true))
            );
        }
    });
    draw(&mut cx, window);
    assert_eq!(call(&mut cx, window, "trashRowCentre", &["0"]), original);
    assert_eq!(state(&mut cx, window).get("busy"), Some(&Json::Bool(false)));
}
