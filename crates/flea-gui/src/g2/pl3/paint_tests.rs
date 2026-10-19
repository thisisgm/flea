//! The Trash list Element drawn in a real window: columns, grounds, shaping once, thumbnails and their eviction.
use super::board::{HOVER, WASH};
use super::elide;
use super::list::columns;
use super::pane::TrashPane;
use super::tests::{answer, ink, listed, opened, row, sent};
use crate::list::bench::real_cx;
use gpui::{prelude::*, ImageAssetLoader, Resource, TestAppContext, WindowHandle};
use std::{path::Path, sync::Arc, time::Duration};

fn draw(cx: &mut TestAppContext, window: WindowHandle<TrashPane>) {
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[test]
fn trash_columns_follow_the_board() {
    assert_eq!(columns(908.), (42., 489., 540., 759.), "14 pad, 19 slot, 9 gaps, Original Location 210, Deleted 135, 14 end");
    assert_eq!(columns(100.).1, 0., "a narrow pane gives the name nothing rather than overlap");
    assert_eq!(elide::right("old-draft.txt", 20), "old-draft.txt");
    assert_eq!(elide::right("old-draft.txt", 6), "old-d\u{2026}", "a name keeps its head");
    assert_eq!(elide::left("/fx/v/work/Code", 8), "\u{2026}rk/Code", "a location keeps its tail");
    assert_eq!(elide::left("/fx", 0), "", "no room draws nothing");
}

#[test]
fn trash_rows_ground_as_the_boards_draw_them() {
    let mut cx = real_cx();
    let (window, _) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None), row("c.txt", None)]);
    let ink = ink();
    window.update(&mut cx, |pane, _, cx| {
        let paint = pane.paint.borrow();
        assert_eq!(paint.ground(0), Some(ink.accent.opacity(WASH)), "trash: the cursor on a marked row draws the rung alone");
        assert_eq!(paint.ground(1), None, "an unmarked row draws no ground");
        drop(paint);
        pane.action("extendDown", false, cx);
        pane.hover(Some(2), cx);
        let paint = pane.paint.borrow();
        assert_eq!(paint.ground(0), Some(ink.mark_fill), "a marked row off the cursor takes the marked plate");
        assert_eq!(paint.ground(1), Some(ink.accent.opacity(WASH)), "the cursor row's rung");
        assert_eq!(paint.ground(2), Some(ink.foreground.opacity(HOVER)), "the pointer's 8% wash");
        drop(paint);
        pane.click_row(2, false, cx);
        let paint = pane.paint.borrow();
        assert_eq!((paint.ground(0), paint.ground(1), paint.cursor), (None, None, 2), "a plain click marks its row alone");
        assert_eq!(paint.ground(2), Some(ink.accent.opacity(WASH)), "and the pointer wash gives way to the rung");
    }).unwrap();
    draw(&mut cx, window);
    draw(&mut cx, window);
    window.update(&mut cx, |pane, _, _| {
        let paint = pane.paint.borrow();
        assert_eq!(paint.painted, [0, 1, 2], "every held visible row paints");
        assert_eq!(paint.shapes, 3, "each row is shaped once across frames");
    }).unwrap();
}

#[test]
fn a_thumbnail_draws_in_the_mark_slot_and_leaves_with_its_row() {
    let mut cx = real_cx();
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/images/icon.svg");
    let (window, calls) = opened(&mut cx, &[row("a.png", Some("/fixture/a.png")), row("b.png", Some("/fixture/b.png"))]);
    cx.executor().advance_clock(Duration::from_millis(70));
    cx.run_until_parked();
    let asks: Vec<String> = sent(&calls).into_iter().filter(|line| line.contains(r#""c":"trashthumb""#)).collect();
    assert_eq!(asks.len(), 2, "both visible image rows ask");
    let id = |line: &str| line[line.find(r#""id":"#).unwrap() + 5..].split(',').next().unwrap().to_string();
    let (first, second) = (id(&asks[0]), id(&asks[1]));
    draw(&mut cx, window);
    window.update(&mut cx, |pane, _, cx| {
        assert!(pane.paint.borrow().drawn_images.is_empty(), "the glyph stays drawn before a reply");
        pane.line(&format!(r#"{{"t":"trashthumbed","id":{first},"file":"{fixture}"}}"#), cx);
        pane.line(&format!(r#"{{"t":"trashthumbed","id":{second},"file":"/nonexistent/flea-pl3/missing.png"}}"#), cx);
    }).unwrap();
    draw(&mut cx, window);
    cx.run_until_parked();
    draw(&mut cx, window);
    let source = Resource::Path(Arc::from(Path::new(fixture)));
    window.update(&mut cx, |pane, _, cx| {
        assert_eq!(pane.paint.borrow().drawn_images, [0], "the decoded image replaces the glyph; a failed decode keeps it");
        assert!(cx.has_asset::<ImageAssetLoader>(&source), "the image is cached while its row shows it");
        pane.action("reload", false, cx);
    }).unwrap();
    answer(&mut cx, window, &calls, "list", &listed(&[row("a.png", Some("/fixture/a.png"))]));
    draw(&mut cx, window);
    window.update(&mut cx, |pane, _, cx| {
        assert!(pane.paint.borrow().drawn_images.is_empty(), "a refresh clears every image at once");
        assert!(!cx.has_asset::<ImageAssetLoader>(&source), "and its decoded image leaves the cache");
    }).unwrap();
}
