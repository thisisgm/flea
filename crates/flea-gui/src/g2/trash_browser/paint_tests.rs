//! The Trash list Element drawn in a real window: columns, grounds, shaping once, and the generic icon on image rows.
use super::board::{HOVER, WASH};
use super::elide;
use super::list::columns;
use super::pane::TrashPane;
use super::tests::{ink, opened, row, sent};
use crate::list::bench::real_cx;
use gpui::{prelude::*, TestAppContext, WindowHandle};
use std::time::Duration;

fn draw(cx: &mut TestAppContext, window: WindowHandle<TrashPane>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[test]
fn trash_columns_follow_the_board() {
    assert_eq!(
        columns(&super::board::Board::default(), 908.),
        (42., 489., 540., 759.),
        "14 pad, 19 slot, 9 gaps, Original Location 210, Deleted 135, 14 end"
    );
    assert_eq!(
        columns(&super::board::Board::default(), 100.).1,
        0.,
        "a narrow pane gives the name nothing rather than overlap"
    );
    assert_eq!(elide::right("old-draft.txt", 20), "old-draft.txt");
    assert_eq!(
        elide::right("old-draft.txt", 6),
        "old-d\u{2026}",
        "a name keeps its head"
    );
    assert_eq!(
        elide::left("/fx/v/work/Code", 8),
        "\u{2026}rk/Code",
        "a location keeps its tail"
    );
    assert_eq!(elide::left("/fx", 0), "", "no room draws nothing");
}

#[test]
fn trash_rows_ground_as_the_boards_draw_them() {
    let mut cx = real_cx();
    let (window, _) = opened(
        &mut cx,
        &[row("a.txt", None), row("b.txt", None), row("c.txt", None)],
    );
    let ink = ink();
    window
        .update(&mut cx, |pane, _, cx| {
            let paint = pane.paint.borrow();
            assert_eq!(
                paint.ground(0),
                Some(ink.accent.opacity(WASH)),
                "trash: the cursor on a marked row draws the rung alone"
            );
            assert_eq!(paint.ground(1), None, "an unmarked row draws no ground");
            drop(paint);
            pane.action("extendDown", false, cx);
            cx.set_global(crate::list::hover_wash::HoverWash(true));
            pane.hover(Some(2), cx);
            let paint = pane.paint.borrow();
            assert_eq!(
                paint.ground(0),
                Some(ink.foreground.opacity(0.35)),
                "a marked row off the cursor takes the list pane's translucent marked plate"
            );
            assert_eq!(
                paint.ground(1),
                Some(ink.accent.opacity(WASH)),
                "the cursor row's rung"
            );
            assert_eq!(
                paint.ground(2),
                Some(ink.foreground.opacity(HOVER)),
                "the pointer's 8% wash under Highlight on hover"
            );
            drop(paint);
            pane.click_row(2, false, cx);
            let paint = pane.paint.borrow();
            assert_eq!(
                (paint.ground(0), paint.ground(1), paint.cursor),
                (None, None, 2),
                "a plain click marks its row alone"
            );
            assert_eq!(
                paint.ground(2),
                Some(ink.accent.opacity(WASH)),
                "and the pointer wash gives way to the rung"
            );
        })
        .unwrap();
    draw(&mut cx, window);
    draw(&mut cx, window);
    window
        .update(&mut cx, |pane, _, _| {
            let paint = pane.paint.borrow();
            assert_eq!(paint.painted, [0, 1, 2], "every held visible row paints");
            assert_eq!(paint.shapes, 3, "each row is shaped once across frames");
        })
        .unwrap();
}

#[test]
fn a_trash_image_row_keeps_the_generic_icon() {
    let mut cx = real_cx();
    let (window, calls) = opened(
        &mut cx,
        &[
            row("a.png", Some("/fixture/a.png")),
            row("b.png", Some("/fixture/b.png")),
        ],
    );
    cx.executor().advance_clock(Duration::from_millis(70));
    cx.run_until_parked();
    let asks = sent(&calls)
        .into_iter()
        .filter(|line| line.contains(r#""c":"trashthumb""#))
        .count();
    assert_eq!(asks, 0, "no Trash row asks for a thumbnail");
    draw(&mut cx, window);
    window
        .update(&mut cx, |pane, _, _| {
            assert!(
                pane.paint.borrow().drawn_images.is_empty(),
                "every image row draws the generic icon"
            );
        })
        .unwrap();
}

#[test]
fn ey388_v_toggling_the_cursor_row_off_drops_its_plate() {
    let mut cx = real_cx();
    let (window, _) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    let ink = ink();
    window
        .update(&mut cx, |pane, _, cx| {
            assert_eq!(pane.paint.borrow().ground(0), Some(ink.accent.opacity(WASH)));
            pane.action("toggleSelect", false, cx);
            assert_eq!(pane.view().selected_count(), 0, "v unmarks the only marked row");
            let paint = pane.paint.borrow();
            assert_eq!(
                (paint.ground(0), paint.edge_at(0)),
                (None, false),
                "TrashView.qml grounds only marked rows, so an unmarked cursor row draws no plate and no edge"
            );
        })
        .unwrap();
}

#[test]
fn ey405_a_marked_row_under_its_menu_keeps_the_readable_marked_plate() {
    let mut cx = real_cx();
    let (window, _) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    let ink = ink();
    window
        .update(&mut cx, |pane, _, cx| {
            pane.right_click(Some(0), 40., 80., cx);
            assert!(pane.menu_open);
            assert_eq!(
                pane.paint.borrow().ground(0),
                Some(ink.foreground.opacity(0.35)),
                "trash-menu: the row keeps the list's translucent marked plate while its menu is open"
            );
        })
        .unwrap();
}
