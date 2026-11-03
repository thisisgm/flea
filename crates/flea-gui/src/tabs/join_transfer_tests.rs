use super::*;
use crate::{backend::Event as BackendEvent, tabs::transfer};

fn ack(
    shell: &mut crate::state::Shell,
    token: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: Request::Call {
                target: "fleatab".into(),
                function: "taken".into(),
                arguments: vec![token.into()],
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().unwrap().reply
}

// The wire line a backend delivers, as every owner receives it.
fn peeked_event(reply: &transfer::Peeked) -> BackendEvent {
    BackendEvent::Peeked(
        flea::jsondoc::parse(&format!(
            "{{\"t\":\"peeked\",\"path\":\"{}\",\"n\":0,\"rows\":[],\"hidden\":{},\"hiddenLast\":{},\"failed\":{},\"first\":{}}}",
            flea::json::escape(&reply.path),
            reply.hidden,
            reply.hidden_last,
            reply.read_failed,
            reply.first
        ))
        .unwrap(),
    )
}

fn peek_reply(path: &str) -> transfer::Peeked {
    transfer::Peeked {
        path: path.into(),
        hidden: false,
        hidden_last: false,
        first: transfer::PEEK_FIRST,
        read_failed: false,
    }
}

#[gpui::test]
fn u25_addressed_ack_closes_only_the_captured_identity(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        let (other_backend, _other_requests) = Backend::test_channel();
        let other = cx.new(|cx| shell_with(other_backend, cx));
        let token = entity.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 1, now));
            let token = shell.strip.bar.out_token.clone();
            assert!(!token.is_empty());
            assert_eq!(ack(shell, "unrelated", window, cx), Reply::value("false"));
            shell.with_tabs(|_, panes| {
                panes.visit(0, &mut |tabs, pane| tabs.move_tab(pane, 1, 0));
            });
            shell.strip.bar.out_finished(now);
            token
        });
        let identities = |shell: &crate::state::Shell| -> Vec<u64> {
            shell.strip.tabs[0]
                .tabs
                .as_ref()
                .unwrap()
                .items
                .iter()
                .map(|tab| tab.identity)
                .collect()
        };
        let before = entity.read(cx).strip.tabs[0]
            .tabs
            .as_ref()
            .unwrap()
            .items
            .clone();
        let lifted = before[0].identity;
        other.update(cx, |shell, cx| {
            assert_eq!(ack(shell, &token, window, cx), Reply::value("false"));
            assert_eq!(shell.strip.tabs[0].count(), 1);
        });
        entity.update(cx, |shell, cx| {
            assert_eq!(ack(shell, &token, window, cx), Reply::value("true"));
            assert_eq!(shell.strip.tabs[0].count(), 2);
            let kept: Vec<u64> = before
                .iter()
                .map(|tab| tab.identity)
                .filter(|identity| *identity != lifted)
                .collect();
            assert_eq!(
                identities(shell),
                kept,
                "the captured identity closed, wherever it moved"
            );
            assert_eq!(ack(shell, &token, window, cx), Reply::value("false"));
        });
    });
}

#[gpui::test]
fn u25_foreign_drop_requires_its_folder_check_and_failure_keeps_tabs(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let payload = transfer::payload(
                "987654321",
                "receive-token",
                "/received",
                "list",
                "cursor.txt",
            );
            let now = shell.tab_now();
            assert!(!shell.with_tabs(|bar, panes| bar.strip_drop(panes, "invalid", 0., now)));
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &payload, 0., now)));
            let request = requests().unwrap();
            assert!(request.contains("\"c\":\"peek\"") && request.contains("\"first\":2"));
            let mut reply = transfer::Peeked {
                path: "/received".into(),
                hidden: false,
                hidden_last: false,
                first: 3,
                read_failed: false,
            };
            shell.receive(0, peeked_event(&reply), window, cx);
            assert!(shell.strip.bar.pending.is_some());
            assert_eq!(shell.strip.tabs[0].count(), 1);
            reply.first = transfer::PEEK_FIRST;
            reply.read_failed = true;
            shell.receive(0, peeked_event(&reply), window, cx);
            assert!(shell.strip.bar.pending.is_none());
            assert_eq!(shell.strip.tabs[0].count(), 1);
            assert_eq!(
                shell.strip.notice[0].text,
                shell.locale.catalog.tr("tabs.folder_gone")
            );
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &payload, 0., now)));
            reply.read_failed = false;
            shell.receive(0, peeked_event(&reply), window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), "/received");
            assert!(shell.strip.bar.pending.is_none());
            assert!(
                shell.strip.bar.ack_running,
                "the source is acknowledged as soon as the tab is received, as v0.3.12 does"
            );
            assert!(shell.strip.bar.taken_queue.is_empty());
        });
    });
}

#[gpui::test]
fn u25_refused_native_start_releases_the_retained_internal_gesture(cx: &mut TestAppContext) {
    use gpui::{point, px, Modifiers, MouseButton};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| shell.update(cx, |shell, cx| key(shell, "t", window, cx)));
    cx.run_until_parked();
    let (start, outside) = cx.update(|window, cx| {
        let shell = shell.read(cx);
        let bounds = shell.strip.bounds.get().unwrap();
        let start = point(
            px(crate::tabs::bar::PAD_X + shell.strip.bar.tab_width(2) / 2.),
            bounds.origin.y + px(13.),
        );
        (
            start,
            point(window.viewport_size().width + px(40.), start.y),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        start + point(px(6.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::default());
    cx.update(|_, cx| {
        assert!(
            shell.read(cx).strip.bar.out_active,
            "the resolver was called before TestPlatform refused startup"
        )
    });
    cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        assert!(!shell.strip.bar.out_active);
        assert!(shell.strip.bar.drag_from.is_none());
        assert!(shell.strip.bar.lifts.is_empty());
        assert_eq!(shell.strip.tabs[0].count(), 2);
    });
    cx.simulate_mouse_down(start, MouseButton::Middle, Modifiers::default());
    cx.simulate_mouse_up(start, MouseButton::Middle, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(shell.read(cx).strip.tabs[0].count(), 1));
}

#[gpui::test]
fn u25_pointer_drag_reorders_without_selecting_or_listing(cx: &mut TestAppContext) {
    use gpui::{point, px, Modifiers, MouseButton};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
        })
    });
    cx.run_until_parked();
    let (start, end, original, lists) = cx.update(|_, cx| {
        let shell = shell.read(cx);
        let bounds = shell.strip.bounds.get().unwrap();
        let width = shell.strip.bar.tab_width(3);
        let y = bounds.origin.y + px(13.);
        (
            point(px(crate::tabs::bar::PAD_X + width / 2.), y),
            point(px(crate::tabs::bar::PAD_X + 3. * width + 3.), y),
            shell.strip.tabs[0]
                .tabs
                .as_ref()
                .unwrap()
                .items
                .iter()
                .map(|tab| tab.identity)
                .collect::<Vec<_>>(),
            shell.panes[0].as_ref().unwrap().pane.list_requests,
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        start + point(px(6.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let tabs = shell.strip.tabs[0].tabs.as_ref().unwrap();
        assert_eq!(
            tabs.items
                .iter()
                .map(|tab| tab.identity)
                .collect::<Vec<_>>(),
            [original[1], original[2], original[0]]
        );
        assert_eq!(tabs.index, 1);
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.list_requests, lists);
        assert!(!shell.strip.bar.out_active);
    });
}

#[gpui::test]
fn rev_u25_failed_destination_listing_leaves_the_tab_in_both_windows(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let payload = transfer::payload("987654321", "failed-listing", "/refused", "list", "");
            let now = shell.tab_now();
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &payload, 0., now)));
            shell.receive(0, peeked_event(&peek_reply("/refused")), window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert!(
                shell.strip.bar.ack_running,
                "the receiver acknowledged at once"
            );
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "list".into(),
                    path: "/refused".into(),
                    message: "permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
            assert_eq!(
                shell.strip.tabs[0].count(),
                2,
                "the receiver keeps the tab the source closed"
            );
            assert!(shell.strip.bar.taken_queue.is_empty());
        });
    });
}

#[gpui::test]
fn rev_u25_second_receive_before_landing_keeps_the_first_tab_unacknowledged(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let now = shell.tab_now();
            let first = transfer::payload("987654321", "first-token", "/first", "list", "");
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &first, 0., now)));
            shell.receive(0, peeked_event(&peek_reply("/first")), window, cx);
            assert!(shell.strip.bar.ack_running);
            let second = transfer::payload("987654322", "second-token", "/second", "list", "");
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &second, 0., now)));
            shell.receive(0, peeked_event(&peek_reply("/second")), window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 3);
            let queued: Vec<_> = shell.strip.bar.taken_queue.iter().cloned().collect();
            assert_eq!(
                queued,
                [("987654322".to_string(), "second-token".to_string())],
                "the first ack runs and the second waits behind it, none is overwritten"
            );
        });
    });
}

#[gpui::test]
fn u25_a_peeked_line_for_another_owner_does_not_disturb_the_strip(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            // A receive waits on its folder check; this peek differs only in path, so the path filter alone keeps it out.
            let payload = transfer::payload(
                "987654321",
                "receive-token",
                "/received",
                "list",
                "cursor.txt",
            );
            let now = shell.tab_now();
            assert!(shell.with_tabs(|bar, panes| bar.strip_drop(panes, &payload, 0., now)));
            assert!(shell.strip.bar.pending.is_some());
            let columns = flea::jsondoc::parse(&format!(
                r#"{{"t":"peeked","path":"/files/child","first":{},"n":1,"rows":[{{"n":"seed.txt","d":false}}],"hidden":false,"hiddenLast":false}}"#,
                transfer::PEEK_FIRST
            ))
            .unwrap();
            shell.receive(0, BackendEvent::Peeked(columns), window, cx);
            assert!(shell.strip.bar.pending.is_some(), "the folder check still waits");
            assert_eq!(shell.strip.tabs[0].count(), 1);
            assert!(shell.strip.bar.taken_queue.is_empty());
        });
    });
}
