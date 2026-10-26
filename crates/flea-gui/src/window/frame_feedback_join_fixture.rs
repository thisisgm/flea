//! Mounted producers and actual TestWindow receipts, with no assigned presentation facts.
use super::*;
use crate::window::{pane::PaneHost, root::Root, startup::View};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::tests::shell_with,
};
use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use gpui::{
    AppContext, Entity, FrameTerminal, PresentationTime, RequestFrameOptions, TestAppContext,
    VisualTestContext,
};
use std::time::Duration;

pub(super) fn mount(
    cx: &mut TestAppContext,
    dual: bool,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut(usize) -> Option<String>,
) {
    let (backend, mut requests) = Backend::test_lines();
    let (right_backend, mut right_requests) = Backend::test_lines();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            if dual {
                let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
                right.pane.use_executor(cx.background_executor().clone());
                right.backend = Some(right_backend);
                shell.panes[1] = Some(right);
                shell.lifecycle.attach(1);
            }
            shell
        }),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.enable_frame_feedback();
    edit(&shell, cx, |shell, window, cx| {
        cx.set_reduce_motion(true);
        shell.follow_frame_feedback(window, cx);
        window.focus(&shell.focus, cx);
        assert!(shell.presentation.observer.is_some());
        if dual {
            for answer in [
                Reply::Listed {
                    path: "/right".into(),
                    total: 2,
                },
                rows(2),
            ] {
                shell.receive(1, BackendEvent::Reply(answer), window, cx);
            }
            shell.set_view(View::Dual, window, cx);
        }
        cx.notify();
    });
    paint(cx);
    let receive = move |side| {
        if side == 0 {
            requests()
        } else {
            right_requests()
        }
    };
    (shell, cx, receive)
}

pub(super) fn edit<R>(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    cx.update(|window, cx| shell.update(cx, |shell, cx| change(shell, window, cx)))
}

pub(super) fn paint(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

pub(super) fn step(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

pub(super) fn begin(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    edit(shell, cx, |shell, window, cx| {
        assert!(shell.navigate(0, "/next".into(), None, Default::default(), window, cx));
    });
    cx.run_until_parked();
}

pub(super) fn submit(cx: &mut VisualTestContext) -> u64 {
    let before = cx.update(|window, _| window.presented_frames().unwrap().committed);
    cx.simulate_frame_request(RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    let id = cx.update(|window, _| {
        assert_eq!(window.current_draw_id(), None);
        window.presented_frames().unwrap().committed
    });
    assert!(id > before, "real Window must issue a successful receipt");
    id
}

pub(super) fn terminal(cx: &mut VisualTestContext, id: u64) {
    cx.simulate_frame_terminal(FrameTerminal {
        frame_id: id,
        outcome: FrameOutcome::Presented(PresentationTime {
            timestamp: None,
            clock_id: None,
        }),
    });
    cx.run_until_parked();
}

pub(super) fn rows(listing: usize) -> Reply {
    Reply::Rows {
        start: 0,
        listing,
        rows: ["first", "second"]
            .map(|name| RowFacts {
                name: name.into(),
                ..Default::default()
            })
            .into(),
    }
}

pub(super) fn ready(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    edit(shell, cx, |shell, window, cx| {
        for answer in [
            Reply::Listed {
                path: "/next".into(),
                total: 2,
            },
            rows(3),
        ] {
            shell.receive(0, BackendEvent::Reply(answer), window, cx);
        }
    });
    cx.run_until_parked();
}

pub(super) fn whole(shell: &Entity<Shell>, cx: &mut VisualTestContext, landed: bool) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.busy(), !landed);
        assert_eq!(
            host.pane.state.kind,
            if landed { Kind::Ready } else { Kind::Loading }
        );
        assert_eq!(host.path, if landed { "/next" } else { "/files" });
        assert_eq!(host.pane.path(), host.path);
        assert_eq!(shell.path_labels[0].as_ref(), host.path);
        assert_eq!(host.pane.cursor, 0);
        assert_eq!(
            host.pane.swap.mark.shown(),
            !landed,
            "actual Root mark visibility"
        );
        assert!(host.selected.is_empty());
        assert_eq!(host.pane.held.total, if landed { 2 } else { 0 });
        if landed {
            assert_eq!(
                host.pane
                    .held
                    .rows
                    .iter()
                    .map(|row| row.name.as_str())
                    .collect::<Vec<_>>(),
                ["first", "second"]
            );
            assert_eq!(host.status.as_ref(), "2 items");
        } else {
            assert!(host.pane.held.rows.is_empty());
            assert_eq!(
                host.status.as_ref(),
                shell.locale.catalog.tr("pane.reading")
            );
        }
    });
}

pub(super) fn readback(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> Json {
    use crate::ipc::{pane::PaneAdapter, Adapter};
    shell.read_with(cx, |shell, _| {
        let reply = PaneAdapter {
            pane: shell.pane(side).unwrap(),
            catalog: &shell.locale.catalog,
        }
        .call("swapState", &[])
        .unwrap();
        assert_eq!(reply.code, 0);
        flea::jsondoc::parse(&reply.stdout).unwrap()
    })
}

pub(super) fn open_menu(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    requests: &mut impl FnMut(usize) -> Option<String>,
) {
    let at = shell.read_with(cx, |shell, _| {
        let frame = shell.panes[1].as_ref().unwrap().input.drawn.unwrap().frame;
        gpui::point(
            frame.bounds.left() + gpui::px(40.),
            frame.bounds.top() + gpui::px(frame.row / 2.),
        )
    });
    cx.simulate_mouse_down(at, gpui::MouseButton::Right, gpui::Modifiers::default());
    let requested = loop {
        let line = requests(1).expect("real menu must request a snapshot on side 1");
        let data = flea::jsondoc::parse(&line).unwrap();
        if data.get("c").and_then(Json::as_str) == Some("menuaction")
            && data.get("op").and_then(Json::as_str) == Some("snapshot")
        {
            break flea::json::field_usize(&line, "id").unwrap() as u64;
        }
    };
    shell.read_with(cx, |shell, _| {
        let captured = shell.menu.captured.as_ref().expect("real menu capture");
        assert_eq!(captured.side, 1);
        assert_eq!(captured.id, requested);
    });
}

pub(super) fn cover_right(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    edit(shell, cx, |shell, window, cx| {
        shell.focus_pane(1, cx);
        shell.mount_trash(window, cx);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
        assert!(shell.trash_action("openTrash", window, cx));
        for (op, fields) in [
            (
                "list",
                r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha","size":6},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta","size":5}]"#,
            ),
            ("summary", r#", "bytes":11,"partial":false"#),
        ] {
            let host = shell.trash.as_ref().unwrap();
            let side = host.side;
            let id = host.pane.read(cx).view().request_id;
            let line = format!(r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true{fields}}}"#);
            shell.receive(side, BackendEvent::TrashLine(line), window, cx);
        }
    });
    cx.run_until_parked();
    paint(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(shell.slots.covers[0].is_none());
        assert!(shell.slots.covers[1].is_some());
        assert!(shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .paint
            .borrow()
            .drawn());
    });
}

pub(super) fn close(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    edit(shell, cx, |shell, window, cx| {
        shell.close(window, cx);
    });
    shell.read_with(cx, |shell, _| {
        assert!(shell.presentation.observer.is_none())
    });
}

pub(super) fn presented_ready_list(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let id = submit(cx);
    shell.read_with(cx, |shell, _| {
        let submitted = shell
            .presentation
            .pending
            .iter()
            .find(|frame| frame.id == id)
            .unwrap();
        let content = &submitted.draw.panes[0].as_ref().unwrap().content;
        assert_eq!(content.path.as_ref(), "/next");
        assert_eq!(content.held.total, 2);
        assert_eq!(content.drawn_cursor, Some(0));
        assert_eq!(content.painted_rows, [0, 1]);
        assert_eq!(
            content
                .held
                .rows
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(content.json().get("selection"), Some(&Json::Null));
    });
    terminal(cx, id);
}
