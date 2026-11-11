//! A relist of the open Trash keeps its drawn rows: F5 or an outside change never gives them back to Reading Trash… (#422).
use super::{
    join_tests::{answer, draw, root, shell},
    monitor::Heard,
    pane::TrashPane,
    presentation::Stage,
    window_tests::{fields, listed, names, TOTAL},
};
use crate::{list::bench::real_cx, window::root::Root};
use flea::jsondoc::Json;
use gpui::{Context, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

const EMPTY: &str = r#", "total":0,"start":0,"present":[],"rows":[]"#;
// Longer than the hold and cap together, so a held relist would show Reading Trash… by then.
const PAST_THE_HOLD_MS: u64 = 400;

fn on_pane(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    act: impl FnOnce(&mut TrashPane, &mut Context<TrashPane>),
) {
    shell(cx, window, |shell, window, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, act);
        shell.flush_trash(window, cx);
    });
}

fn past_the_hold(cx: &mut TestAppContext, window: WindowHandle<Root>) {
    const { assert!(PAST_THE_HOLD_MS > crate::pane::swap::HOLD_MS + crate::pane::swap::CAP_MS) };
    cx.executor()
        .advance_clock(Duration::from_millis(PAST_THE_HOLD_MS));
    cx.run_until_parked();
    draw(cx, window);
}

// Starts a relist with `start`, outlasts the open's hold and cap, then answers it with renamed rows.
fn relist_keeps_the_drawn_rows(start: impl FnOnce(&mut TestAppContext, WindowHandle<Root>)) {
    let mut cx = real_cx();
    let window = root(&mut cx);
    let (count, _) = listed(&mut cx, window);
    let drawn = names(&mut cx, window);
    assert!(!drawn.is_empty());
    start(&mut cx, window);
    on_pane(&mut cx, window, |pane, _| {
        assert_eq!(pane.view().pending_op, "list")
    });
    past_the_hold(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let host = shell.trash.as_ref().unwrap();
        assert_eq!(
            host.presentation.stage,
            Stage::Ready,
            "a relist opens no hold"
        );
        let pane = host.pane.read(cx);
        assert!(
            !pane.presentation_loading,
            "Reading Trash… never covers the rows"
        );
        assert_eq!(pane.empty_line(), None);
        assert_eq!(pane.paint.borrow().total, TOTAL);
    });
    assert_eq!(
        names(&mut cx, window),
        drawn,
        "the last complete rows stay drawn"
    );
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    assert!(visual.debug_bounds("trash-empty-line").is_none());
    assert!(visual.debug_bounds("trash-list-area").is_some());
    // The relist reads from the held window's start, which is the top of the list here.
    answer(
        &mut cx,
        window,
        "list",
        &fields(0, count).replace("item-", "relisted-"),
    );
    answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        let host = shell.trash.as_ref().unwrap();
        assert_eq!(host.presentation.stage, Stage::Ready);
        assert_eq!(host.presentation.swap.counters.fallbacks, 0);
    });
    let relisted = names(&mut cx, window);
    assert_eq!(relisted.len(), drawn.len());
    assert!(
        relisted.iter().all(|name| name.contains("relisted-")),
        "the relist's first screenful replaces the rows at once"
    );
}

#[test]
fn an_f5_relist_keeps_the_drawn_rows_until_its_first_screenful_lands() {
    relist_keeps_the_drawn_rows(|cx, window| cx.simulate_keystrokes(window.into(), "f5"));
}

#[test]
fn an_outside_change_relists_without_reading_trash() {
    relist_keeps_the_drawn_rows(|cx, window| {
        on_pane(cx, window, |pane, cx| pane.heard(Heard::Changed, cx))
    });
}

#[test]
fn a_cancel_reply_relists_without_reading_trash() {
    relist_keeps_the_drawn_rows(|cx, window| {
        // A delete card dismissed on its default Cancel button sends cancel, whose reply re-reads the Trash; down selects the row d d reviews.
        cx.simulate_keystrokes(window.into(), "down d d");
        answer(
            cx,
            window,
            "prepare",
            r#", "token":41,"count":1,"bytes":6,"all":false"#,
        );
        draw(cx, window);
        let card = |cx: &mut TestAppContext| {
            let state = super::join_tests::state(cx, window);
            state.get("confirmation").unwrap().get("opened").cloned()
        };
        assert_eq!(card(cx), Some(Json::Bool(true)), "the delete card is up");
        cx.simulate_keystrokes(window.into(), "enter");
        cx.run_until_parked();
        assert_eq!(card(cx), Some(Json::Bool(false)), "Cancel dismissed it");
        on_pane(cx, window, |pane, _| {
            assert_eq!(pane.view().pending_op, "cancel")
        });
        answer(cx, window, "cancel", r#", "cancelled":true"#);
    });
}

#[test]
fn a_relist_of_an_empty_trash_still_falls_back_to_reading_trash() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    let relist = |pane: &mut TrashPane, cx: &mut Context<TrashPane>| pane.heard(Heard::Changed, cx);
    on_pane(&mut cx, window, relist);
    answer(&mut cx, window, "list", EMPTY);
    answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
    draw(&mut cx, window);
    on_pane(&mut cx, window, relist);
    past_the_hold(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let host = shell.trash.as_ref().unwrap();
        assert_eq!(
            host.presentation.stage,
            Stage::Loading,
            "no rows were drawn to keep"
        );
        assert!(host.pane.read(cx).presentation_loading);
    });
}
