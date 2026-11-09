use super::host;
use crate::{backend::Event, window::startup::View};

#[test]
fn u09_live_columns_watch_refresh_retains_rows_and_refuses_undrawn_changes() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        let requests = shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .peek_requests;
        shell.receive(
            0,
            Event::Changed {
                path: "/unasked".into(),
            },
            window,
            cx,
        );
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area
                .peek_requests,
            requests
        );
        shell.receive(
            0,
            Event::Changed {
                path: "/srv/a/b/c/d01".into(),
            },
            window,
            cx,
        );
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area
                .peek_requests,
            requests + 1
        );
        assert_eq!(
            host::call(shell, "columnNames", &["child"], window, cx)
                .stdout
                .trim(),
            "seed.txt"
        );
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "columnNames", &["child"], window, cx)
                .stdout
                .trim(),
            "seed.txt",
            "a frame drawn while the refresh waits lost its held rows"
        );
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false), ("new.txt", false)],
            false,
            window,
            cx,
        );
        host::peek(
            shell,
            "/unasked",
            &[("ghost.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "columnNames", &["child"], window, cx)
                .stdout
                .trim(),
            "seed.txt|new.txt"
        );
        assert!(
            host::call(shell, "columnPeekNames", &["/unasked"], window, cx)
                .stdout
                .trim()
                .is_empty()
        );
        shell.set_view(View::List, window, cx);
        let before = shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .peek_requests;
        shell.receive(
            0,
            Event::Changed {
                path: "/srv/a/b/c/d01".into(),
            },
            window,
            cx,
        );
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area
                .peek_requests,
            before
        );
        shell.set_view(View::Columns, window, cx);
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area
                .peek_requests
                > before,
            "a return must rearm the evicted watches"
        );
    });
}

#[test]
fn u09_live_columns_empty_denied_and_stale_order_replies_use_real_producers() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        let mut stale = host::document(shell, "/srv/a/b", &[("ghost", true)], false);
        if let flea::jsondoc::Json::Obj(fields) = &mut stale {
            fields.push(("by".into(), flea::jsondoc::Json::Str("mtime".into())));
        }
        shell.receive(0, Event::Peeked(stale), window, cx);
        host::peek(shell, "/srv/a/b/c/d01", &[], false, window, cx);
        host::peek(shell, "/srv/a/b", &[], true, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "columnChildEmpty", &[], window, cx)
                .stdout
                .trim(),
            "true 1.00 1.00"
        );
        let mark: Vec<f32> = host::call(shell, "columnChildMarkRect", &[], window, cx)
            .stdout
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let child = columns
            .painters
            .get(crate::columns::geometry::Slot::Child)
            .and_then(|paint| paint.borrow().drawn.as_ref().map(|drawn| drawn.bounds))
            .expect("the child track drew");
        assert_eq!(mark.len(), 4, "the shown empty tile has a painted mark");
        assert!(mark[2] > 0. && mark[3] > 0.);
        let (left, right) = (
            f32::from(child.origin.x).floor(),
            f32::from(child.origin.x + child.size.width).ceil(),
        );
        assert!(
            mark[0] >= left && mark[0] + mark[2] <= right,
            "the mark sits inside the child track"
        );
        assert!(
            host::call(shell, "columnChildRowCentre", &["0"], window, cx)
                .stdout
                .trim()
                .is_empty()
        );
        assert!(
            host::call(shell, "columnParentRowCentre", &["0"], window, cx)
                .stdout
                .trim()
                .is_empty()
        );
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let notice = columns.notices[crate::columns::geometry::Slot::Parent.index()]
            .as_ref()
            .expect("denial mounts the shared refusal producer");
        let facts = notice
            .read(cx)
            .facts()
            .cloned()
            .expect("the notice holds facts");
        assert_eq!(
            (facts.kind, facts.sentence.to_string()),
            (
                crate::g2::status_feedback::state::Kind::Locked,
                shell.locale.catalog.tr("pane.locked")
            ),
            "a denied neighbour is the Locked tile, never the stopped-responding error"
        );
        assert!(!columns
            .area
            .peeks
            .names(&crate::columns::peeks::peek_key("/srv/a/b", false, false))
            .contains("ghost"));
    });
    host::change(&mut cx, window, |shell, window, cx| {
        shell.receive(
            0,
            Event::Changed {
                path: "/srv/a/b/c/d01".into(),
            },
            window,
            cx,
        );
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("new.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert!(
            host::call(shell, "columnChildEmpty", &[], window, cx)
                .stdout
                .trim()
                .is_empty(),
            "an unmounted empty hero kept its old visible state"
        );
        assert_eq!(
            host::call(shell, "columnNames", &["child"], window, cx)
                .stdout
                .trim(),
            "new.txt"
        );
    });
}

#[test]
fn u09_live_columns_refilled_window_under_the_same_cursor_updates_its_child() {
    let mut cx = host::context();
    let (window, mut requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        let held = std::rc::Rc::new(crate::list::pane::Held {
            listing: pane.held.listing,
            total: 1000,
            start: 0,
            rows: vec![host::row("d01", true), host::row("d02", true)],
        });
        pane.held = held;
        // Replacing the fixture's snapshot invalidates the old two-row painted scroll extent.
        pane.scroll_extent = None;
        assert!(pane.move_cursor(crate::list::pane::Move::End));
        let (start, count) = pane.wanted_window().unwrap();
        let request = pane
            .requests
            .outstanding
            .expect("the viewport request is pending");
        assert_eq!((request.start, request.count), (start, count));
        shell.panes[0]
            .as_ref()
            .unwrap()
            .backend
            .as_ref()
            .unwrap()
            .window(start, count)
            .expect("the fixture backend accepts the viewport request");
        assert_eq!(requests(), Some((start, count)));
        shell.sync_columns(window, cx);
        let rows = (start..start.saturating_add(count).min(1000))
            .map(|index| {
                if index == 999 {
                    host::row("tail", true)
                } else {
                    host::row(&format!("file-{index}"), false)
                }
            })
            .collect();
        let listing = shell.panes[0].as_ref().unwrap().pane.held.listing;
        shell.receive(
            0,
            Event::Reply(flea::protocol::Reply::Rows {
                start,
                listing,
                rows,
            }),
            window,
            cx,
        );
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert!(
            columns
                .area
                .drawn
                .iter()
                .any(|path| path == "/srv/a/b/c/tail"),
            "a same-listing window reply did not resync the cursor's folder"
        );
        host::peek(
            shell,
            "/srv/a/b/c/tail",
            &[("tail.txt", false)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "columnNames", &["child"], window, cx)
                .stdout
                .trim(),
            "tail.txt"
        );
    });
}
