//! Real Shell/Root paint facts only. Typed terminal causal tests require the fork's public test adapter.
use super::*;
use crate::{
    pane::swap::{Options, CAP_MS},
    window::rate_atomic_tests::{draw, reply, root, rows},
};
use flea::{jsondoc::Json, protocol::Reply};
use gpui::TestAppContext;
use std::time::Duration;

#[path = "typed_presentation_tests.rs"]
mod typed;

#[gpui::test]
fn painted_content_keeps_the_composed_rows_cursor_path_and_status_after_model_changes(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _requests) = root(cx);
    draw(cx);
    let content = shell.read_with(cx, |shell, _| {
        let frame = shell.presentation.rendered[0].as_ref().unwrap();
        Rc::new(frame.content.painted(&frame.list.borrow(), frame.ready))
    });
    assert!(
        !content.painted_rows.is_empty(),
        "the real List prepaint must provide rows"
    );
    assert_eq!(content.drawn_cursor, Some(content.cursor));
    assert_eq!(content.path.as_ref(), "/files");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.navigate(0, "/next".into(), None, Options::default(), window, cx));
        });
    });
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/next".into(),
            total: 2,
        },
    );
    reply(&shell, cx, rows(3));
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let current = shell.presentation.rendered[0].as_ref().unwrap();
        assert_eq!(current.content.path.as_ref(), "/next");
        assert_eq!(current.content.status.as_deref(), Some("2 items"));
        assert_eq!(content.path.as_ref(), "/files");
        assert_ne!(content.held.listing, current.content.held.listing);
        assert_ne!(content.status, current.content.status);
        assert!(!Rc::ptr_eq(&content.held, &current.content.held));
        assert_eq!(content.held.total, 1000);
    });
    let captured = content.json();
    assert_eq!(captured.get("selection"), Some(&Json::Null));
    assert_eq!(
        captured.get("selectionCoverage").and_then(Json::as_str),
        Some("uncomposed")
    );
}

#[gpui::test]
fn real_root_paints_cannot_create_submission_or_loading_ack_without_feedback(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _requests) = root(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.navigate(0, "/next".into(), None, Options::default(), window, cx));
        });
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(CAP_MS));
    cx.run_until_parked();
    for _ in 0..3 {
        draw(cx);
    }
    shell.read_with(cx, |shell, _| {
        let pane = shell.pane(0).unwrap();
        assert!(
            pane.swap.frames.loading.get() > 0,
            "real Root painted Loading"
        );
        assert_eq!(
            pane.swap.mark_since, None,
            "paint cannot start the visible minimum"
        );
        assert!(shell.presentation.staged.borrow().is_none());
        assert!(shell.presentation.pending.is_empty());
        assert_eq!(
            pane.swap.observed.borrow().frames,
            0,
            "no terminal callbacks were observed"
        );
        assert_eq!(
            pane.swap.observed.borrow().json().get("frames"),
            Some(&Json::Null)
        );
    });
    cx.executor().advance_clock(Duration::from_millis(2500));
    cx.run_until_parked();
    reply(
        &shell,
        cx,
        Reply::Listed {
            path: "/next".into(),
            total: 2,
        },
    );
    reply(&shell, cx, rows(3));
    draw(cx);
    assert!(!shell.read_with(cx, |shell, _| shell.pane(0).unwrap().busy()));
}
