//! Review round uish-u09-r1 regressions: each assertion names the contract the join holds.
use super::host::{advance, call, change, context, frame, open, peek, settle};
use crate::{
    columns::{geometry::Slot, TrackRows, TrackState},
    g2::status_feedback::state::Kind,
    window::startup::View,
};
use gpui::{px, size, VisualTestContext};
use std::time::Duration;

#[test]
fn u09_review_r1_1_denied_neighbour_draws_the_locked_tile() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    change(&mut cx, window, |shell, window, cx| {
        peek(shell, "/srv/a/b", &[], true, window, cx);
        settle(shell, window, cx);
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, cx| {
        let locked = shell.locale.catalog.tr("pane.locked");
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let notice = columns.notices[Slot::Parent.index()]
            .clone()
            .expect("the denial mounts a notice");
        let facts = notice
            .read(cx)
            .facts()
            .cloned()
            .expect("the notice holds facts");
        assert_eq!(
            (facts.kind, facts.sentence.to_string()),
            (Kind::Locked, locked),
            "the denied parent column draws {facts:?}"
        );
    });
}

#[test]
fn u09_review_r1_2_prepaint_allocates_no_track_copy() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    change(&mut cx, window, |shell, window, cx| {
        peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("sibling", true)],
            false,
            window,
            cx,
        );
        settle(shell, window, cx);
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let tracks = columns.area.tracks();
        let painter = columns.painters.get(Slot::Parent).unwrap().borrow();
        let drawn = painter.drawn.as_ref().unwrap();
        assert!(
            std::rc::Rc::ptr_eq(&drawn.tracks, tracks),
            "prepaint kept a copy of the tracks, not the frame's own"
        );
        assert_eq!(
            drawn.view().path.as_ptr(),
            tracks[drawn.at].path.as_ptr(),
            "prepaint heap-copied the track path this frame"
        );
    });
}

#[test]
fn u09_review_r1_3_the_entry_holds_the_list_until_every_column_answers() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    frame(&mut cx, window);
    change(&mut cx, window, |shell, window, cx| {
        assert_eq!(shell.shown_view(), View::List, "replies are still out");
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(columns.painters.built(), 0, "a track drew before its rows");
        assert_eq!(columns.counters.holds, 1);
        assert!(call(shell, "columnParentRowCentre", &["0"], window, cx)
            .stdout
            .trim()
            .is_empty());
        peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        assert_eq!(shell.shown_view(), View::List, "the child is still out");
        peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
        assert_eq!(shell.shown_view(), View::Columns, "every column answered");
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let active = columns
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow()
            .visible()
            .count();
        assert!(active > 0, "the first Columns frame drew no active rows");
        for slot in [Slot::Parent, Slot::Child] {
            let painter = columns.painters.get(slot).unwrap().borrow();
            let drawn = painter.drawn.as_ref().unwrap();
            assert!(
                !matches!(&drawn.view().rows, TrackRows::Peek(rows) if rows.is_empty()),
                "the first Columns frame drew a blank {slot:?} track for {}",
                drawn.view().path
            );
        }
        let counters = &columns.counters;
        assert_eq!(
            (counters.holds, counters.fallbacks, counters.blank_frames),
            (1, 0, 0)
        );
    });
}

#[test]
fn u09_review_r1_3_an_ancestor_past_the_cap_mounts_as_a_loading_tile() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    frame(&mut cx, window);
    advance(
        &mut cx,
        window,
        Duration::from_millis(crate::pane::swap::CAP_MS + 20),
    );
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        assert_eq!(shell.shown_view(), View::Columns, "the cap ended the hold");
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let states: Vec<(Slot, TrackState, bool)> = columns
            .area
            .tracks()
            .iter()
            .filter(|view| view.slot != Slot::Active)
            .map(|view| {
                let blank = matches!(&view.rows, TrackRows::Peek(rows) if rows.is_empty());
                (view.slot, view.state, blank)
            })
            .collect();
        assert!(
            states.contains(&(Slot::Parent, TrackState::Loading, true)),
            "the unanswered parent was not a loading tile: {states:?}"
        );
        assert!(
            states
                .iter()
                .all(|(_, state, blank)| *state == TrackState::Loading || !*blank),
            "a track mounted blank with no tile: {states:?}"
        );
        let counters = &columns.counters;
        assert_eq!(
            (counters.holds, counters.fallbacks, counters.blank_frames),
            (1, 1, 0)
        );
    });
}

#[test]
fn u09_review_r1_4_reentry_reader_answers_only_presented_geometry() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    change(&mut cx, window, |shell, window, cx| {
        peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("sibling", true)],
            false,
            window,
            cx,
        );
        peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    frame(&mut cx, window);
    let mut first = String::new();
    change(&mut cx, window, |shell, window, cx| {
        first = call(shell, "columnParentRowCentre", &["0"], window, cx).stdout;
        shell.set_view(View::List, window, cx);
    });
    frame(&mut cx, window);
    VisualTestContext::from_window(window.into(), &cx).simulate_resize(size(px(2100.), px(700.)));
    frame(&mut cx, window);
    let mut stale = String::new();
    change(&mut cx, window, |shell, window, cx| {
        shell.set_view(View::Columns, window, cx);
        stale = call(shell, "columnParentRowCentre", &["0"], window, cx).stdout;
    });
    frame(&mut cx, window);
    let mut drawn = String::new();
    change(&mut cx, window, |shell, window, cx| {
        drawn = call(shell, "columnParentRowCentre", &["0"], window, cx).stdout;
    });
    assert!(
        stale.trim().is_empty(),
        "before any Columns frame was presented the reader answered {stale:?}, the earlier visit's {first:?}"
    );
    assert!(
        !drawn.trim().is_empty() && drawn != first,
        "the frame drawn next answers {drawn:?} for the resized window, not the earlier {first:?}"
    );
}

#[test]
fn u09_review_r1_5_column_count_in_list_view() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    change(&mut cx, window, settle);
    frame(&mut cx, window);
    change(&mut cx, window, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
    });
    frame(&mut cx, window);
    change(&mut cx, window, |shell, window, cx| {
        let count = call(shell, "columnCount", &[], window, cx).stdout;
        assert_eq!(
            count.trim(),
            "0",
            "List view draws no column, columnCount answers {count:?}"
        );
    });
}

#[test]
fn u09_review_r1_6_columns_fallback_draws_the_loading_mark_in_the_active_track() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    change(&mut cx, window, settle);
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        let now = pane.now();
        assert!(pane.begin(
            "/srv/a/b/c/d01".into(),
            crate::pane::swap::Options::default(),
            now
        ));
        assert!(
            pane.tick(now + 1000),
            "the hold expires to its loading fallback"
        );
        cx.notify();
    });
    frame(&mut cx, window);
    frame(&mut cx, window);
    change(&mut cx, window, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let mark = &host.pane.swap.mark;
        let columns = host.columns.as_ref().unwrap();
        let active = columns
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow()
            .drawn
            .as_ref()
            .unwrap()
            .bounds;
        assert!(mark.shown(), "the fallen-back pane shows its loading mark");
        let slot = mark.slot();
        assert!(
            slot.size.width > px(0.) && active.contains(&slot.center()),
            "the shown loading mark paints at {slot:?}, outside the Active track {active:?}"
        );
    });
}

#[test]
fn u09_review_r1_peek_tiles_draw_inside_their_own_track() {
    let mut cx = context();
    let (window, _requests) = open(&mut cx, "/srv/a/b/c");
    change(&mut cx, window, |shell, window, cx| {
        peek(shell, "/srv/a/b", &[], true, window, cx);
        peek(shell, "/srv/a/b/c/d01", &[], false, window, cx);
    });
    frame(&mut cx, window);
    let (parent, child) = window
        .update(&mut cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let bounds = |slot| {
                columns
                    .painters
                    .get(slot)
                    .unwrap()
                    .borrow()
                    .drawn
                    .as_ref()
                    .unwrap()
                    .bounds
            };
            (bounds(Slot::Parent), bounds(Slot::Child))
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    let lock = visual
        .debug_bounds("fb1-state-mark")
        .expect("the denied parent mounted its Locked tile");
    let hero = visual
        .debug_bounds("fb1-hero-mark")
        .expect("the empty child mounted its hero");
    assert!(
        parent.contains(&lock.center()),
        "the Locked mark at {lock:?} is outside its Parent track {parent:?}"
    );
    assert!(
        child.contains(&hero.center()),
        "the hero mark at {hero:?} is outside its Child track {child:?}"
    );
}
