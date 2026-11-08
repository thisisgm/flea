//! A window fetch keeps the drawn rows: scrolling Trash never gives its listing back to Reading Trash… (#422).
use super::{
    join::Effect,
    join_tests::{answer, draw, root, shell},
    presentation::Stage,
};
use crate::{list::bench::real_cx, window::root::Root};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

pub(super) const TOTAL: usize = 200;

pub(super) fn fields(start: usize, count: usize) -> String {
    let rows = (start..(start + count).min(TOTAL))
        .map(|index| {
            format!(
                r#"{{"uri":"trash:///item-{index}","original":"/private/item-{index}","identity":"identity-{index}"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(r#", "total":{TOTAL},"start":{start},"present":[],"rows":[{rows}]"#)
}

// Opens Trash, re-lists it with TOTAL items and returns the held window's row count and the view height.
pub(super) fn listed(cx: &mut TestAppContext, window: WindowHandle<Root>) -> (usize, f32) {
    super::join_tests::opened(cx, window);
    let (count, height) = shell(cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx)
        });
        shell.flush_trash(window, cx);
        let view = pane.read(cx).view();
        (view.window_rows(), view.height)
    });
    answer(cx, window, "list", &fields(0, count));
    answer(cx, window, "summary", r#", "bytes":0,"partial":false"#);
    draw(cx, window);
    (count, height)
}

pub(super) fn names(cx: &mut TestAppContext, window: WindowHandle<Root>) -> Vec<String> {
    shell(cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        let paint = pane.paint.borrow();
        paint.lines.iter().map(|line| line.name.clone()).collect()
    })
}

#[test]
fn a_window_round_trip_keeps_the_total_and_the_drawn_lines() {
    let row_h = super::board::Board::default().row_h;
    let mut cx = real_cx();
    let window = root(&mut cx);
    let (count, height) = listed(&mut cx, window);
    let drawn = names(&mut cx, window);
    assert!(!drawn.is_empty());
    // A jump past every held row needs a window, and the backend takes longer than the open's hold and cap.
    let scroll = count as f32 * row_h;
    let (start, sent) = shell(&mut cx, window, |shell, window, cx| {
        let host = shell.trash.as_ref().unwrap();
        host.pane.update(cx, |pane, cx| {
            pane.wheel(scroll, cx);
            assert_eq!(pane.view().pending_op, "window");
        });
        // The backend answers the window the view sent, from the start it asked for.
        let sent = host
            .effects
            .borrow()
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::Send(line) if line.contains(r#""op":"window""#) => {
                    let request = flea::jsondoc::parse(line).unwrap();
                    let field = |name: &str| request.get(name).unwrap().as_f64().unwrap() as usize;
                    Some((field("start"), field("count")))
                }
                _ => None,
            });
        shell.flush_trash(window, cx);
        sent.unwrap()
    });
    let (top, end) = (count, ((scroll + height) / row_h).ceil() as usize);
    assert!(
        start < top && start + sent > end,
        "the window holds a margin past each edge of the new viewport"
    );
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let host = shell.trash.as_ref().unwrap();
        assert_eq!(
            host.presentation.stage,
            Stage::Ready,
            "a window fetch opens no hold"
        );
        let pane = host.pane.read(cx);
        assert!(
            !pane.presentation_loading,
            "Reading Trash… never covers the rows"
        );
        assert_eq!(pane.empty_line(), None);
        let paint = pane.paint.borrow();
        assert_eq!(paint.total, TOTAL, "the total stays while the window reads");
        assert_eq!(paint.scroll_y, 0., "the last complete viewport stays drawn");
    });
    assert_eq!(names(&mut cx, window), drawn, "the drawn lines stay");
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    assert!(visual.debug_bounds("trash-empty-line").is_none());
    assert!(visual.debug_bounds("trash-list-area").is_some());
    answer(&mut cx, window, "window", &fields(start, count));
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let host = shell.trash.as_ref().unwrap();
        let view = host.pane.read(cx).view();
        assert_eq!((view.first, view.pending_op.as_str()), (start, ""));
        assert_eq!(host.presentation.stage, Stage::Ready);
        assert_eq!(host.presentation.swap.counters.fallbacks, 0);
        let paint = host.pane.read(cx).paint.borrow();
        assert_eq!((paint.total, paint.scroll_y), (TOTAL, scroll));
        let top = (scroll / row_h).floor() as usize;
        let end = ((scroll + height) / row_h).ceil() as usize;
        assert!(
            (top..end).all(|index| paint.line(index).is_some()),
            "the new viewport draws whole"
        );
    });
}

#[test]
fn a_notch_near_the_held_edge_reads_the_next_window_while_the_rows_scroll() {
    let row_h = super::board::Board::default().row_h;
    let mut cx = real_cx();
    let window = root(&mut cx);
    let (count, height) = listed(&mut cx, window);
    // Notch down three rows at a time until a window goes out; every notch until then scrolls without one.
    let mut scroll = 0.;
    let sent = shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        let sent = pane.update(cx, |pane, cx| {
            while pane.view().pending_op.is_empty() && scroll < (TOTAL as f32 * row_h - height) {
                scroll += 3. * row_h;
                pane.wheel(3. * row_h, cx);
            }
            pane.view().pending_op == "window"
        });
        shell.flush_trash(window, cx);
        sent
    });
    assert!(sent, "the next window is read before the held rows run out");
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        let paint = pane.paint.borrow();
        assert_eq!(
            paint.scroll_y, scroll,
            "the rows follow the notch that asked"
        );
        let top = (scroll / row_h).floor() as usize;
        let end = ((scroll + height) / row_h).ceil() as usize;
        assert!(end <= count && (top..end).all(|index| paint.line(index).is_some()));
        assert!(
            !pane.interaction_held(),
            "the held rows still take the pointer"
        );
    });
    // A notch while that window reads still scrolls the held rows.
    shell(&mut cx, window, |shell, window, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.wheel(3. * row_h, cx));
        shell.flush_trash(window, cx);
    });
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert_eq!(pane.paint.borrow().scroll_y, scroll + 3. * row_h);
    });
}

#[test]
fn a_relist_whose_rows_cover_the_view_publishes_while_the_next_window_reads() {
    let row_h = super::board::Board::default().row_h;
    let mut cx = real_cx();
    let window = root(&mut cx);
    let (_, height) = listed(&mut cx, window);
    let shown = (height / row_h).ceil() as usize;
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.heard(super::monitor::Heard::Changed, cx)
        });
        shell.flush_trash(window, cx);
    });
    // The re-list holds just the shown rows, so the summary's answer sends the next window at once.
    answer(&mut cx, window, "list", &fields(0, shown + 1));
    answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let host = shell.trash.as_ref().unwrap();
        let pane = host.pane.read(cx);
        assert_eq!(pane.view().pending_op, "window");
        assert_eq!(host.presentation.stage, Stage::Ready);
        assert!(
            !pane.presentation_loading,
            "Reading Trash… never covers the rows"
        );
        let paint = pane.paint.borrow();
        assert_eq!(paint.total, TOTAL);
        assert!((0..shown).all(|index| paint.line(index).is_some()));
    });
}
