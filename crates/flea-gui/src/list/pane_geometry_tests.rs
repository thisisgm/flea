use super::Pane;
use crate::list::{pane::Held, Metrics};
use gpui::{point, px, size, Bounds};
use std::rc::Rc;

fn pane(row: f32, height: f32, total: usize, header: Option<f32>) -> Pane {
    let mut pane = Pane::at_row_height("/files".into(), height, row);
    pane.held = Rc::new(Held {
        listing: 1,
        total,
        ..Held::default()
    });
    pane.scroll_extent = header.map(|header| total as f32 * row + header);
    pane
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn g70_row_height_preserves_fractional_anchor_before_the_next_extent_frame() {
    let mut pane = pane(31., 400., 100, Some(27.));
    pane.scroll_to(2727.);
    pane.set_row_height(42.);
    close(pane.scroll_y / pane.row_height(), 2727. / 31.);
    assert_eq!(pane.scroll_extent, Some(4227.));
    pane.set_height(390.);
    close(pane.scroll_y / pane.row_height(), 2727. / 31.);

    // The next painted frame refreshes the header as well, without recovering a lost anchor.
    let frame = crate::input::Frame::list(
        Bounds::new(point(px(0.), px(0.)), size(px(900.), px(390.))),
        &Metrics::at(px(20.)),
        &pane,
        Default::default(),
    );
    assert_eq!(frame.content, 4237.);
    pane.scroll_extent = Some(frame.content);
    pane.scroll_by(0.);
    close(pane.scroll_y / pane.row_height(), 2727. / 31.);
}

#[test]
fn g70_row_height_changes_only_the_row_contribution_to_the_extent() {
    let mut pane = pane(31., 400., 100, Some(27.));
    pane.scroll_to(20.5 * 31.);
    pane.set_row_height(42.);
    assert_eq!(pane.scroll_extent, Some(4227.));
    close(pane.scroll_y, 20.5 * 42.);
    pane.scroll_by(f32::MAX);
    assert_eq!(pane.scroll_y, 4227. - 400.);
    pane.set_row_height(23.);
    assert_eq!(pane.scroll_extent, Some(2327.));
    assert_eq!(pane.scroll_y, 2327. - 400.);
    pane.scroll_by(f32::MAX);
    assert_eq!(pane.scroll_y, 2327. - 400.);
}

#[test]
fn g70_row_height_keeps_the_uncached_extent_fallback_and_short_list_clamp() {
    let mut uncached = pane(31., 400., 100, None);
    uncached.scroll_to(20.5 * 31.);
    uncached.set_row_height(42.);
    assert_eq!(uncached.scroll_extent, None);
    close(uncached.scroll_y, 20.5 * 42.);
    uncached.scroll_by(f32::MAX);
    assert_eq!(uncached.scroll_y, 4200. - 400.);
    for total in [0, 3] {
        let mut short = pane(31., 400., total, Some(27.));
        short.set_row_height(42.);
        assert_eq!(short.scroll_y, 0.);
        assert_eq!(short.scroll_extent, Some(total as f32 * 42. + 27.));
    }
}

#[test]
fn g70_invalid_or_unchanged_row_height_keeps_scroll_and_extent() {
    let mut pane = pane(31., 400., 100, Some(27.));
    pane.scroll_to(20.5 * 31.);
    for row in [f32::NAN, f32::INFINITY, 0., -1., 31.] {
        pane.set_row_height(row);
        assert_eq!(pane.row_height(), 31.);
        assert_eq!(pane.scroll_y, 20.5 * 31.);
        assert_eq!(pane.scroll_extent, Some(3127.));
    }
}

#[test]
fn sw18_viewport_handoff_keeps_a_revealed_launch_cursor_visible() {
    let mut pane = pane(31., 1359., 2000, Some(27.));
    pane.cursor = 1500;
    pane.reveal_cursor();
    close(pane.scroll_y, 45172.);
    // Native List's header and footer settle after the initial --select reply.
    pane.set_height(1305.);
    let top = pane.cursor as f32 * pane.row_height() - pane.scroll_y;
    assert!(
        top >= 0. && top + pane.row_height() <= 1305.,
        "launch row falls below the real viewport: {top}"
    );
    pane.set_height(619.);
    let top = pane.cursor as f32 * pane.row_height() - pane.scroll_y;
    assert!(
        top >= 0. && top + pane.row_height() <= 619.,
        "the revealed row remains visible when the native window shrinks"
    );
}

#[test]
fn sw18_viewport_handoff_preserves_a_cursor_already_scrolled_offscreen() {
    let mut pane = pane(31., 1359., 2000, Some(27.));
    pane.cursor = 1500;
    pane.scroll_to(1000.5);
    pane.set_height(619.);
    close(pane.scroll_y, 1000.5);
}
