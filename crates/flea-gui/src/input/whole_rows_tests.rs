//! S2-5: after a wheel scroll the cursor lands on a wholly drawn row, never one the list's top or bottom edge cuts.
use super::{Frame, PaneInput};
use crate::list::pane::Grow;
use gpui::{point, px, size, Bounds};

fn input(columns: usize, height: f32, grow: Grow) -> PaneInput {
    PaneInput {
        frame: Some(Frame {
            bounds: Bounds::new(point(px(0.), px(0.)), size(px(800.), px(height))),
            row: 20.,
            lane: 14.,
            content: 2_000_000.,
            scroll: 0.,
            listing: 1,
            total: 100_000,
            columns,
            cell_width: 100.,
            grow,
            cell_origin: (0., 0.),
        }),
        ..Default::default()
    }
}

#[test]
fn a_scroll_that_cuts_the_top_row_pulls_the_cursor_to_the_first_whole_row() {
    let mut input = input(1, 190., Grow::default());
    let mut cursor = 0;
    // Row 1114 spans 22,280..22,300: a scroll of 22,290 shows only its lower half under the header.
    input.scrolled(22_290., &mut cursor);
    assert_eq!(cursor, 1_115);
    // From below, the last whole row is 1123 (22,460..22,480 ends at 22,480 = 22,290 + 190).
    let mut cursor = 5_000;
    input.scrolled(22_290., &mut cursor);
    assert_eq!(cursor, 1_123);
    // At 22,310 the view ends on 1124's bottom edge (22,500), so 1124 is whole and the cursor lands there.
    let mut cursor = 5_000;
    input.scrolled(22_310., &mut cursor);
    assert_eq!(cursor, 1_124);
    let mut cursor = 5_000;
    input.scrolled(22_299., &mut cursor);
    assert_eq!(cursor, 1_123);
}

#[test]
fn an_aligned_scroll_keeps_the_top_row_and_a_cursor_inside_stays_put() {
    let mut input = input(1, 190., Grow::default());
    let mut cursor = 0;
    input.scrolled(22_280., &mut cursor);
    assert_eq!(cursor, 1_114);
    let mut cursor = 1_118;
    input.scrolled(22_290., &mut cursor);
    assert_eq!(cursor, 1_118);
}

#[test]
fn a_grid_line_cut_by_the_top_moves_the_cursor_to_the_first_whole_line() {
    let mut input = input(4, 190., Grow::default());
    let mut cursor = 1;
    // Line 10 spans 200..220; a scroll of 210 cuts it, so the first whole cell is line 11's first.
    input.scrolled(210., &mut cursor);
    assert_eq!(cursor, 44);
    // The grid's 8 px top gap puts line 10 at 208..228, so a scroll of 205 still shows it whole.
    if let Some(frame) = input.frame.as_mut() {
        frame.cell_origin = (8., 8.);
    }
    let mut cursor = 1;
    input.scrolled(205., &mut cursor);
    assert_eq!(cursor, 40);
}

#[test]
fn a_viewport_shorter_than_a_row_keeps_the_floor_row() {
    let mut input = input(1, 15., Grow::default());
    let mut cursor = 0;
    input.scrolled(22_290., &mut cursor);
    assert_eq!(cursor, 1_114);
}

#[test]
fn the_rename_band_row_counts_its_band_when_it_is_cut() {
    // Row 1114 grows by a 30 px band: 22,280..22,330.
    let grow = Grow {
        at: 1_114,
        band: 30.,
    };
    let mut input = input(1, 190., grow);
    let mut cursor = 0;
    input.scrolled(22_290., &mut cursor);
    assert_eq!(cursor, 1_115);
    let mut cursor = 5_000;
    input.scrolled(22_290., &mut cursor);
    // Rows below the band sit 30 px lower: 1121 ends at 22,470 inside the view, 1122 at 22,490 past its 22,480 end.
    assert_eq!(cursor, 1_121);
}
