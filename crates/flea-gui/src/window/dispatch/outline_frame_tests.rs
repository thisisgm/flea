//! Mounted frames retain a settled listing throughout in-place changes.
use super::test_shell::{land, open_root, row, sent};
use crate::{backend::Event, pane::Kind, state::Shell};
use flea::protocol::Reply;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{rc::Rc, time::Duration};

fn frame(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Vec<String>, f32, usize, Kind) {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.update(|window, cx| {
        shell.update(cx, |_, cx| cx.notify());
        window.simulate_next_frame(cx);
    });
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        (
            list.visible()
                .iter()
                .filter_map(|row| row.prepared.as_ref().map(|row| row.label.clone()))
                .collect(),
            host.pane.scroll_y,
            list.drawn_rows.listing,
            host.pane.state.kind,
        )
    })
}

fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, reply: Reply) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Event::Reply(reply), window, cx)
        });
    });
}

#[gpui::test]
fn outline_frames_keep_rows_until_the_complete_answer(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        1,
    );
    let before = frame(&shell, cx);
    assert!(!before.0.is_empty());
    sent();
    cx.simulate_keystrokes("ctrl-right");
    for _ in 0..20 {
        assert_eq!(
            frame(&shell, cx),
            before,
            "waiting frame must retain settled rows"
        );
    }
    cx.executor().advance_clock(Duration::from_millis(
        crate::window::startup::LISTING_WAIT_MS,
    ));
    assert_eq!(
        frame(&shell, cx),
        before,
        "a slow child retains the same settled frame"
    );
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/files".into(),
            total: 3,
        },
    );
    for _ in 0..3 {
        assert_eq!(
            frame(&shell, cx),
            before,
            "count alone must never alter a frame"
        );
    }
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    reply(
        &shell,
        cx,
        Reply::Rows {
            start: 0,
            rows: vec![folder, child, row("end.txt", false)],
            listing: 2,
        },
    );
    let after = frame(&shell, cx);
    assert_eq!(after.0, ["A", "child.txt", "end.txt"]);
    assert_eq!((after.1, after.3), (before.1, Kind::Ready));
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .swap
                .counters
                .fallbacks,
            0
        );
    });
    cx.simulate_keystrokes("ctrl-left");
    for _ in 0..10 {
        assert_eq!(
            frame(&shell, cx),
            after,
            "collapse retains the expanded frame"
        );
    }
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        3,
    );
    assert_eq!(frame(&shell, cx).0, before.0);
}

#[gpui::test]
fn outline_frames_reuse_unchanged_rows_and_keep_fractional_scroll(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let rows: Vec<_> = (0..80).map(|at| row(&format!("F{at:02}"), true)).collect();
    land(&shell, cx, "/files", rows.clone(), 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 20;
        host.pane.scroll_to(17. * host.pane.row_height() + 0.5);
        host.selected = vec![19, 24];
    });
    let before = frame(&shell, cx);
    let retained = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        host.list
            .borrow()
            .visible()
            .iter()
            .find(|row| row.index == 19)
            .unwrap()
            .prepared
            .clone()
            .unwrap()
    });
    cx.simulate_keystrokes("ctrl-right");
    for _ in 0..10 {
        assert_eq!(frame(&shell, cx), before);
    }
    let mut expanded = rows;
    expanded[20].open = true;
    let mut child = row("F20/child.txt", false);
    child.depth = 1;
    expanded.insert(21, child);
    land(&shell, cx, "/files", expanded, 2);
    let after = frame(&shell, cx);
    assert_eq!(after.1, before.1);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 20);
        assert_eq!(host.selected, [19, 25]);
        let list = host.list.borrow();
        let same = list
            .visible()
            .iter()
            .find(|row| row.index == 19)
            .unwrap()
            .prepared
            .as_ref()
            .unwrap();
        assert!(
            Rc::ptr_eq(&retained, same),
            "unchanged row must retain its prepared cells"
        );
    });
}

#[gpui::test]
fn watched_reread_frames_keep_the_settled_listing(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        1,
    );
    let before = frame(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::Changed {
                    path: "/files".into(),
                },
                window,
                cx,
            )
        });
    });
    for _ in 0..50 {
        assert_eq!(
            frame(&shell, cx),
            before,
            "watch must not introduce an empty frame"
        );
    }
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false), row("new.txt", false)],
        2,
    );
    assert_eq!(frame(&shell, cx).0, ["A", "end.txt", "new.txt"]);
}

#[gpui::test]
fn outline_frames_land_a_bounded_window_at_a_deep_cursor(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let mut rows: Vec<_> = (0..1000)
        // The two marked rows are files, so Ctrl+Right opens the cursor's folder.
        .map(|at| row(&format!("F{at:04}"), at != 499 && at != 510))
        .collect();
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    super::test_shell::land_on(&shell, cx, 0, "/files", rows[..count].to_vec(), 1000, 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::pane::Held {
            listing: 1,
            total: 1000,
            start: 450,
            rows: rows[450..450 + count].to_vec(),
        });
        host.pane.cursor = 500;
        host.pane.scroll_to(497. * host.pane.row_height() + 0.5);
        host.selected = vec![499, 510];
    });
    let before = frame(&shell, cx);
    sent();
    cx.simulate_keystrokes("ctrl-right");
    let commands = sent();
    assert!(commands
        .iter()
        .any(|line| line.contains("\"c\":\"outline\",\"rows\":[500]")));
    for _ in 0..10 {
        assert_eq!(frame(&shell, cx), before);
    }
    rows[500].open = true;
    let mut child = row("F0500/child.txt", false);
    child.depth = 1;
    rows.insert(501, child);
    let start = 0;
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/files".into(),
            total: rows.len(),
        },
    );
    assert_eq!(frame(&shell, cx), before);
    reply(
        &shell,
        cx,
        Reply::Rows {
            start,
            rows: rows[start..start + count].to_vec(),
            listing: 2,
        },
    );
    assert_eq!(frame(&shell, cx), before);
    let commands = sent();
    let requested = commands
        .iter()
        .find(|line| flea::json::field_str(line, "c").as_deref() == Some("window"))
        .expect("the retained bounded window was requested");
    let start = flea::json::field_usize(requested, "start").unwrap();
    let count = flea::json::field_usize(requested, "count").unwrap();
    for _ in 0..10 {
        assert_eq!(frame(&shell, cx), before);
    }
    reply(
        &shell,
        cx,
        Reply::Rows {
            start,
            rows: rows[start..start + count].to_vec(),
            listing: 2,
        },
    );
    let after = frame(&shell, cx);
    assert_eq!((after.1, after.3), (before.1, Kind::Ready));
    assert!(after.0.iter().any(|name| name == "child.txt"));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 500);
        assert_eq!(host.selected, [499, 511]);
        assert_eq!(host.pane.held.rows.len(), count);
        assert!(host
            .list
            .borrow()
            .visible()
            .iter()
            .all(|row| row.prepared.is_some()));
    });
}
