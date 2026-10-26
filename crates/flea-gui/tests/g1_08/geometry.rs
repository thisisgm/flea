//! tests/js/gridgeometry.js ported, plus the placement GridArea.qml gets from Qt: partial rows, Contain and the last-row gap.
use flea_gui::grid::{
    geometry::{
        cell_width_for, columns_for, drift_ask, landing_ask, window_for, Density, Metrics,
        ThumbSize, WindowPlan,
    },
    layout::Layout,
};

const MIN_CELL: f32 = 146.;
const THUMB: f32 = 96.;
const PAD_X: f32 = 7.;
const GAP: f32 = 8.;

#[test]
fn columns_and_cells_leave_the_gap_and_the_lane_out() {
    let fits = |width: f32, lane: f32| {
        let columns = columns_for(width, MIN_CELL, THUMB, PAD_X, GAP, lane);
        let cell = cell_width_for(width, columns, GAP, lane);
        let usable = width - GAP - lane;
        assert_eq!(
            columns,
            ((usable / MIN_CELL.max(THUMB + 2. * PAD_X)).floor() as usize).max(1),
            "columns at {width} lane {lane}"
        );
        assert_eq!(
            cell,
            (usable / columns as f32).floor().max(1.),
            "cell width at {width} lane {lane}"
        );
        assert!(
            columns as f32 * cell <= usable,
            "cells at {width} fit the width they were divided from"
        );
        assert!(
            (columns + 1) as f32 * MIN_CELL.max(THUMB + 2. * PAD_X) > usable,
            "one more column at {width} would not fit"
        );
    };
    for width in [500., 800., 1200.] {
        fits(width, 0.);
    }
    for width in [500., 738., 1200.] {
        fits(width, PAD_X);
    }
    assert_eq!(
        columns_for(GAP - 1., MIN_CELL, THUMB, PAD_X, GAP, 0.),
        1,
        "a width the gap already spent still draws one column"
    );
    assert_eq!(
        cell_width_for(GAP - 1., 1, GAP, 0.),
        1.,
        "that column is never narrower than a pixel"
    );
    assert_eq!(
        cell_width_for(800., 0, GAP, 0.),
        800. - GAP,
        "zero columns never divide"
    );
    assert_eq!(
        (
            columns_for(738., MIN_CELL, THUMB, PAD_X, GAP, PAD_X),
            columns_for(738., MIN_CELL, THUMB, PAD_X, GAP, 0.)
        ),
        (4, 5),
        "at 738 the lane costs a column"
    );
    assert_eq!(
        (
            columns_for(800., MIN_CELL, THUMB, PAD_X, GAP, 0.),
            cell_width_for(800., 5, GAP, 0.)
        ),
        (5, 158.)
    );
}

fn plan() -> WindowPlan {
    window_for(18, 11, 150, 26., 124., 25)
}

#[test]
fn the_window_is_the_lists_context_in_whole_tile_rows() {
    let plan = plan();
    assert_eq!(
        plan,
        WindowPlan {
            columns: 18,
            visible: 11,
            buffer: 576,
            size: 1350,
            margin: 36
        }
    );
    assert_eq!(
        (plan.size % 18, plan.buffer % 18),
        (0, 0),
        "a window is whole tile rows"
    );
    assert_eq!(
        window_for(2, 3, 150, 26., 124., 25).margin,
        25,
        "a narrow grid keeps the list's own refetch margin"
    );
    assert_eq!(
        window_for(0, 0, 0, 26., 0., 25).size,
        3,
        "a grid with no columns yet still holds a row each side"
    );
}

#[test]
fn drift_asks_only_near_an_edge_the_held_rows_have_more_beyond() {
    let drift = |first: i64, last: i64, held: usize, length: usize| {
        drift_ask(&plan(), first, last, held, length, 2000)
    };
    assert_eq!(
        drift(1000, 1200, 0, 1350),
        None,
        "rows well inside the held window ask nothing"
    );
    assert_eq!(
        drift(1000, 1330, 0, 1350),
        Some((424, 1350)),
        "rows near the held end ask a window around the view"
    );
    assert_eq!(
        drift(620, 700, 600, 1350),
        Some((44, 1350)),
        "rows near the held start ask a window around the view"
    );
    assert_eq!(
        drift(5, 100, 0, 1350),
        None,
        "the top of the listing is not an edge to refetch"
    );
    assert_eq!(
        drift(1700, 1990, 650, 1350),
        None,
        "the end of the listing is not an edge to refetch"
    );
    assert_eq!(
        drift(0, 50, 0, 0),
        Some((0, 1350)),
        "no rows held asks the window around the view"
    );
    assert_eq!(
        drift(-18, 50, 0, 0),
        Some((0, 1350)),
        "a view showing its top margin asks from the top"
    );
}

#[test]
fn a_far_landing_asks_the_tiles_its_move_draws_and_a_step_asks_nothing() {
    let landing = |cursor: usize, first: i64, held: usize, length: usize| {
        landing_ask(&plan(), cursor, first, 2000, held, length)
    };
    assert_eq!(
        landing(1999, 0, 0, 352),
        Some((1746, 254)),
        "a jump to the last item asks the tail of the listing"
    );
    assert_eq!(
        landing(1999, 0, 700, 1500),
        None,
        "a jump the held rows already reach asks nothing"
    );
    assert_eq!(
        landing(0, 1980, 1000, 1000),
        Some((0, 252)),
        "a jump to the first item asks the head once the held rows left it"
    );
    assert_eq!(
        landing(90, 0, 0, 0),
        None,
        "a step of a few tile rows asks nothing"
    );
    assert_eq!(
        landing(216, 0, 0, 352),
        None,
        "a page of twelve tile rows the held rows reach asks nothing"
    );
}

fn board(total: usize, height: f32) -> Layout {
    Layout::new(
        Metrics::board(),
        (908., height),
        ThumbSize::MEDIUM,
        Density::Compact,
        total,
        0.,
    )
}

#[test]
fn a_partial_last_row_counts_whole_and_the_extent_keeps_the_footer() {
    let layout = board(20, 640.);
    assert_eq!(
        (layout.columns, layout.cell_width, layout.cell_height),
        (6, 147., 134.)
    );
    assert_eq!(
        (layout.rows(), layout.tile_rows(), layout.visible_rows()),
        (4, 4, 5)
    );
    assert_eq!(layout.extent(), 9. + 4. * 134. + 27. + 9.);
    assert_eq!(layout.max_scroll(), 0., "four rows fit 640 with the footer");
    let empty = board(0, 640.);
    assert_eq!(
        (empty.rows(), empty.tile_rows(), empty.drawn(0.)),
        (0, 1, 0..0)
    );
    let long = board(100, 640.);
    assert_eq!(long.rows(), 17);
    assert_eq!(long.max_scroll(), 9. + 17. * 134. + 27. + 9. - 640.);
    assert_eq!(
        long.drawn(0.),
        0..30,
        "rows 0 to 4 meet the view, the fifth partly"
    );
    assert_eq!(
        long.drawn(long.max_scroll()),
        72..100,
        "the partial last row is drawn whole"
    );
}

#[test]
fn a_cursor_landing_on_the_last_row_is_lifted_in_the_same_pass() {
    let layout = board(100, 640.);
    let last_row_top = layout.row_top(16);
    // Contain alone parks the last row flush on the view bottom; the reveal must already sit one gap higher.
    let contain_only = last_row_top + layout.cell_height - layout.height;
    let lifted = layout.reveal(0., 99);
    assert_eq!(
        lifted,
        last_row_top + layout.cell_height + layout.metrics.gap - layout.height
    );
    assert!(
        lifted > contain_only,
        "the lift is in the reveal itself, not a later turn"
    );
    assert!(lifted <= layout.max_scroll());
    let frame = layout.frame(layout.cell(lifted, 99));
    let bottom = frame.bottom() + layout.metrics.hairline;
    assert!(
        bottom <= layout.height - layout.metrics.gap,
        "last frame bottom {bottom} clears the view by a gap"
    );
    assert_eq!(
        layout.reveal(lifted, 99),
        lifted,
        "a row already clear stays put"
    );
    assert_eq!(
        layout.reveal(lifted, 98),
        lifted,
        "another last-row tile keeps the same view"
    );
}

#[test]
fn a_middle_row_reveal_is_contain_alone_and_a_short_grid_never_scrolls() {
    let layout = board(100, 640.);
    let row = 8;
    let revealed = layout.reveal(0., row * 6);
    assert_eq!(
        revealed,
        layout.row_top(row) + layout.cell_height - layout.height,
        "a middle row parks flush as Contain does"
    );
    assert_eq!(
        layout.reveal(revealed, row * 6),
        revealed,
        "a visible tile moves nothing"
    );
    assert_eq!(
        layout.reveal(layout.max_scroll(), 0),
        layout.row_top(0),
        "a reveal up aligns the row top"
    );
    let short = board(20, 640.);
    assert_eq!(
        (short.reveal(0., 19), short.keep_last_row_clear(0., 19)),
        (0., 0.),
        "a grid that fits never lifts"
    );
}

#[test]
fn the_view_index_map_follows_the_cell_grid_and_misses_the_margins() {
    let layout = board(20, 640.);
    assert_eq!(layout.index_at(0., 9., 9.), Some(0));
    assert_eq!(
        layout.index_at(0., 8., 20.),
        None,
        "the left margin is bare"
    );
    assert_eq!(layout.index_at(0., 20., 8.), None, "the top margin is bare");
    assert_eq!(layout.index_at(0., 9. + 147. * 5. + 1., 9.), Some(5));
    assert_eq!(
        layout.index_at(0., 9. + 147. * 6. + 1., 9.),
        None,
        "past the last column is the lane"
    );
    assert_eq!(
        layout.index_at(0., 9. + 147. * 2. + 1., 9. + 134. * 3. + 1.),
        None,
        "past the last tile of the partial row"
    );
    assert_eq!(layout.index_at(0., 9., 9. + 134. * 3. + 1.), Some(18));
    let scrolled = board(100, 640.);
    assert_eq!(
        scrolled.index_at(134., 9., 9.),
        Some(6),
        "a row of scroll moves the map a row"
    );
    let cell = scrolled.cell(0., 7);
    assert_eq!(
        (cell.x, cell.y, cell.width, cell.height),
        (9. + 147., 9. + 134., 147., 134.)
    );
    let (slot, frame) = (scrolled.slot(cell), scrolled.frame(cell));
    // Qt's anchors centre a 64 px slot on the whole pixel, 42 px into a 147 px cell.
    assert_eq!(
        (slot.x, slot.y, slot.width),
        (cell.x + 42., cell.y + 14., 64.)
    );
    assert_eq!(
        (frame.x, frame.y, frame.width, frame.height),
        (cell.x + 1., cell.y + 1., 145., 132.)
    );
}

#[test]
fn the_viewport_reads_tile_rows_as_qt_does_with_the_top_margin_showing() {
    let layout = board(100, 640.);
    assert_eq!(
        layout.viewport(0.),
        (-6, 29),
        "at rest the top margin puts the view a row above 0"
    );
    assert_eq!(layout.viewport(9.), (0, 29));
    assert_eq!(layout.viewport(layout.max_scroll()), (72, 99));
}

#[test]
fn zoom_and_density_size_the_cell_and_a_rename_grows_every_cell() {
    let layout = Layout::new(
        Metrics::board(),
        (908., 640.),
        ThumbSize::named("xlarge"),
        Density::Compact,
        10,
        0.,
    );
    assert_eq!(
        (layout.columns, layout.cell_height),
        (5, 128. + 9. + 33. + 28.)
    );
    let tight = Layout::new(
        Metrics::board(),
        (908., 640.),
        ThumbSize::MEDIUM,
        Density::Tight,
        10,
        0.,
    );
    assert_eq!(tight.cell_height, 64. + 9. + 33. + 14.);
    let renaming = Layout::new(
        Metrics::board(),
        (908., 640.),
        ThumbSize::MEDIUM,
        Density::Compact,
        10,
        12.,
    );
    assert_eq!(renaming.cell_height, 146.);
    let huge = Layout::new(
        Metrics::board(),
        (908., 640.),
        ThumbSize::named("largest"),
        Density::Compact,
        10,
        0.,
    );
    let mark = huge.mark(huge.slot(huge.cell(0., 0)));
    assert_eq!(
        (huge.columns, mark.width),
        (3, 128.),
        "a mark never grows past the glyph cap in the largest slot"
    );
}
