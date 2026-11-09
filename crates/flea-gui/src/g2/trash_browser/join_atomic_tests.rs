//! The binding surface timing note through the same native window tests, with a deterministic producer delay.
use super::{
    join_tests::{answer, call, draw, root, shell},
    presentation::Stage,
};
use crate::list::bench::real_cx;
use std::time::Duration;

#[test]
fn u32_a_slow_open_holds_then_keeps_its_drawn_loading_state_until_one_complete_swap() {
    let mut cx = real_cx();
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
        assert!(
            shell.slots.covers[0].is_none(),
            "the previous complete listing stays drawn"
        );
    });
    cx.executor().advance_clock(Duration::from_millis(65));
    cx.run_until_parked();
    shell(&mut cx, window, |shell, _, _| {
        assert!(shell.slots.covers[0].is_none())
    });
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        assert_eq!(
            shell.trash.as_ref().unwrap().presentation.stage,
            Stage::Loading
        )
    });
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":1,"start":0,"present":["trash:///alpha"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity"}]"#,
    );
    answer(&mut cx, window, "summary", r#", "bytes":6,"partial":false"#);
    assert!(
        call(&mut cx, window, "trashRowCentre", &["0"]).is_empty(),
        "ready rows stay behind the minimum loading interval"
    );
    cx.executor().advance_clock(Duration::from_millis(99));
    cx.run_until_parked();
    shell(&mut cx, window, |shell, _, _| {
        assert_eq!(
            shell.trash.as_ref().unwrap().presentation.stage,
            Stage::Loading
        )
    });
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    draw(&mut cx, window);
    assert!(!call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
    shell(&mut cx, window, |shell, _, _| {
        let presentation = &shell.trash.as_ref().unwrap().presentation;
        assert_eq!(presentation.stage, Stage::Ready);
        assert_eq!(presentation.swap.counters.holds, 1);
        assert_eq!(presentation.swap.counters.fallbacks, 1);
    });
}

#[test]
fn u32_a_fast_open_publishes_after_the_row_and_summary_pair_without_a_loading_frame() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        let host = shell.trash.as_ref().unwrap();
        assert_eq!(host.presentation.stage, Stage::Ready);
        assert_eq!(host.presentation.swap.counters.fallbacks, 0);
    });
    assert!(!call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
}

#[test]
fn u32_a_changed_review_keeps_its_disabled_card_until_replaced_and_still_cancels() {
    use flea::jsondoc::Json;
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "d d");
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":41,"count":1,"bytes":6,"all":false"#,
    );
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx)
        });
        shell.flush_trash(window, cx);
    });
    draw(&mut cx, window);
    let held = super::join_tests::state(&mut cx, window);
    assert_eq!(
        held.get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(true))
    );
    assert_eq!(
        held.get("confirmation").unwrap().get("busy"),
        Some(&Json::Bool(true))
    );
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":42,"count":2,"bytes":11,"all":false"#,
    );
    draw(&mut cx, window);
    let replaced = super::join_tests::state(&mut cx, window);
    assert_eq!(
        replaced
            .get("confirmation")
            .unwrap()
            .get("count")
            .and_then(Json::as_f64),
        Some(2.)
    );
    assert_eq!(
        replaced.get("confirmation").unwrap().get("busy"),
        Some(&Json::Bool(false))
    );
    shell(&mut cx, window, |shell, window, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx)
        });
        shell.flush_trash(window, cx);
    });
    cx.simulate_keystrokes(window.into(), "enter");
    cx.run_until_parked();
    let cancelled = super::join_tests::state(&mut cx, window);
    assert_eq!(
        cancelled.get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(false))
    );
    shell(&mut cx, window, |shell, _, cx| {
        assert_eq!(
            shell
                .trash
                .as_ref()
                .unwrap()
                .pane
                .read(cx)
                .view()
                .pending_op,
            "cancel"
        )
    });
}

#[test]
fn u32_refresh_freezes_row_actions_until_rows_and_summary_commit_together() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx);
        });
        shell.flush_trash(window, cx);
    });
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":3,"start":0,"present":["trash:///added","trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///added","original":"/private/added.txt","identity":"added-identity"},{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity"},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity"}]"#,
    );
    shell(&mut cx, window, |shell, _, cx| {
        assert_eq!(
            shell.trash_count(),
            2,
            "the rail count retains the painted snapshot"
        );
        assert!(
            shell.slots.covers[0].is_some(),
            "the held Trash stays in place"
        );
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            assert_eq!(pane.paint.borrow().total, 2);
            assert_eq!(pane.paint.borrow().lines[0].name, "alpha.txt");
            pane.click_row(0, false, cx);
            pane.action("cursorDown", false, cx);
            pane.right_click(Some(0), 10., 80., cx);
            assert!(!pane.view().is_selected("trash:///added"));
            assert!(pane.view().is_selected("trash:///alpha"));
            assert!(!pane.menu_open);
        });
    });
    answer(&mut cx, window, "summary", r#", "bytes":7,"partial":false"#);
    shell(&mut cx, window, |shell, _, cx| {
        assert_eq!(shell.trash_count(), 3);
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            assert_eq!(pane.paint.borrow().total, 3);
            assert_eq!(pane.paint.borrow().lines[0].name, "added.txt");
            pane.click_row(0, false, cx);
            assert!(pane.view().is_selected("trash:///added"));
        });
    });
}

#[test]
fn u32_failed_reprepare_closes_the_held_review_and_reports_the_actual_failure() {
    use crate::backend::Event;
    use flea::jsondoc::Json;
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "d d");
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":41,"count":1,"bytes":6,"all":false"#,
    );
    shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        host.pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx)
        });
        let id = host.pane.read(cx).view().request_id;
        let side = host.backend_side;
        shell.flush_trash(window, cx);
        shell.receive(side, Event::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"prepare","ok":false,"error":"provider refused review"}}"#)), window, cx);
        shell.flush_trash(window, cx);
    });
    draw(&mut cx, window);
    let state = super::join_tests::state(&mut cx, window);
    assert_eq!(
        state.get("confirmation").unwrap().get("opened"),
        Some(&Json::Bool(false))
    );
    assert_eq!(
        call(&mut cx, window, "lastMessage", &[]),
        "provider refused review"
    );
}

#[test]
fn u32_presented_counters_detect_a_stale_source_snapshot_even_with_newer_shape_revisions() {
    use super::presentation::Presentation;
    let mut presentation = Presentation::new(std::time::Instant::now());
    presentation.begin(true, 0);
    presentation.frame(1, false, 10);
    presentation.presented(1);
    assert_eq!(presentation.holds, 1);
    presentation.ready(2);
    presentation.frame(2, false, 10);
    presentation.presented(2);
    presentation.frame(3, false, 12);
    presentation.presented(3);
    presentation.frame(4, false, 10);
    assert_eq!(
        presentation.blink_backs, 0,
        "unpresented stale content counts nothing"
    );
    presentation.presented(4);
    assert_eq!(
        presentation.blink_backs, 1,
        "the source request, not shape-cache revision, identifies stale content"
    );
    presentation.presented(4);
    assert_eq!(
        presentation.blink_backs, 1,
        "duplicate feedback counts nothing"
    );
}

#[test]
fn u32_fractional_scroll_fetches_the_partly_visible_row_before_publishing_its_viewport() {
    let row_h = super::board::Board::default().row_h;
    let mut cx = real_cx();
    let window = root(&mut cx);
    super::join_tests::opened(&mut cx, window);
    let (count, height) = shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        let count = pane.read(cx).view().window_rows();
        let height = pane.read(cx).view().height;
        pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx)
        });
        shell.flush_trash(window, cx);
        (count, height)
    });
    let fields = |start: usize| {
        let rows = (start..start + count).map(|index| format!(
            r#"{{"uri":"trash:///item-{index}","original":"/private/item-{index}","identity":"identity-{index}"}}"#
        )).collect::<Vec<_>>().join(",");
        format!(
            r#", "total":{},"start":{start},"present":[],"rows":[{rows}]"#,
            count * 4
        )
    };
    answer(&mut cx, window, "list", &fields(0));
    answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
    draw(&mut cx, window);
    let last_complete = count as f32 * row_h - height;
    let next_row = ((last_complete / row_h).floor() + 1.) * row_h;
    let scroll = (last_complete + next_row) / 2.;
    let start = (scroll / row_h).floor() as usize;
    shell(&mut cx, window, |shell, window, cx| {
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, cx| {
            pane.wheel(scroll, cx);
            assert_eq!(
                pane.view().pending_op,
                "window",
                "the bottom partial row needs a new held window"
            );
            assert_eq!(
                pane.paint.borrow().scroll_y,
                0.,
                "the complete previous viewport stays drawn during the fetch"
            );
        });
        shell.flush_trash(window, cx);
    });
    answer(&mut cx, window, "window", &fields(start));
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        let paint = pane.paint.borrow();
        assert_eq!(paint.scroll_y, scroll);
        assert!(
            paint.painted.contains(&count),
            "the newly exposed partial row paints in the same frame as the new viewport"
        );
        let last = ((scroll + height) / row_h).ceil() as usize;
        assert!((start..last).all(|index| paint.line(index).is_some()));
    });
}

#[test]
fn u32_an_idle_state_always_means_a_clickable_row() {
    let mut cx = real_cx();
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
    cx.executor().advance_clock(Duration::from_millis(66));
    cx.run_until_parked();
    draw(&mut cx, window);
    answer(
        &mut cx,
        window,
        "list",
        r#", "total":2,"start":0,"present":["trash:///alpha","trash:///beta"],"rows":[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity"},{"uri":"trash:///beta","original":"/private/beta.txt","identity":"beta-identity"}]"#,
    );
    answer(
        &mut cx,
        window,
        "summary",
        r#", "bytes":11,"partial":false"#,
    );
    draw(&mut cx, window);
    let waiting = super::join_tests::state(&mut cx, window);
    assert_eq!(
        waiting.get("busy").and_then(flea::jsondoc::Json::as_bool),
        Some(true),
        "a drawn loading state held for its minimum is busy"
    );
    assert!(call(&mut cx, window, "trashRowCentre", &["0"]).is_empty());
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    draw(&mut cx, window);
    let idle = super::join_tests::state(&mut cx, window);
    assert_eq!(
        idle.get("busy").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        idle.get("total").and_then(flea::jsondoc::Json::as_f64),
        Some(2.)
    );
    assert!(
        !call(&mut cx, window, "trashRowCentre", &["0"]).is_empty(),
        "idle with two rows answers a centre for the first"
    );
}
