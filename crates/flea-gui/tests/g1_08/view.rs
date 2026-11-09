//! GridArea.qml's policy over the real pane: the window asks through the pane's one slot, far landings, hidden holds, resize, zoom and arrows.
use super::{all, answer, board_view, focus, listed};
use flea_gui::grid::{arrow, geometry::ThumbSize, zoom_travel, Arrow, Shown, ZoomInput};

#[test]
fn a_step_reveals_in_one_pass_and_a_far_jump_asks_once_then_lands_on_its_reply() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    assert_eq!(
        grid.follow_cursor(&mut pane, shown, &focus(6)),
        None,
        "a step asks nothing"
    );
    assert_eq!(grid.scroll, 0., "a visible tile moves nothing");
    let sent = pane.requests.sent;
    let asked = grid
        .follow_cursor(&mut pane, shown, &focus(1999))
        .expect("a far jump asks its window");
    assert_eq!(pane.requests.sent, sent + 1, "a far jump asks one window");
    assert!(
        asked.0 + asked.1 >= 2000 && asked.0 <= 1999,
        "the window {asked:?} covers the cursor"
    );
    assert!(grid.landing_wait, "the wait holds while the reply is out");
    assert_eq!(grid.scroll, 0., "no tile moves before its window lands");
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "a landing wait asks no drift window"
    );
    assert!(answer(&mut pane));
    grid.rows_landed(&mut pane, shown, &focus(1999));
    assert!(!grid.landing_wait, "the wait ends on the ask's own reply");
    let layout = grid.layout(shown);
    assert_eq!(
        grid.scroll,
        layout.reveal(0., 1999),
        "the view lands lifted in the same pass"
    );
    assert!(layout.covers(grid.scroll, 1999));
}

#[test]
fn an_operator_scroll_ends_a_wait_and_a_late_window_does_not_reveal_back() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    grid.follow_cursor(&mut pane, shown, &focus(1999))
        .expect("a far jump asks");
    grid.scroll = 268.;
    assert!(grid.landing_wait, "a position write alone keeps the wait");
    grid.scroll_by(shown, &focus(1999), 134.);
    assert!(!grid.landing_wait, "the operator's scroll ends the wait");
    let parked = grid.scroll;
    assert!(answer(&mut pane));
    grid.rows_landed(&mut pane, shown, &focus(1999));
    assert_eq!(
        grid.scroll, parked,
        "the late window does not reveal the cursor back"
    );
    let mut failed = board_view();
    failed.scroll = failed.layout(shown).max_scroll();
    failed
        .follow_cursor(&mut pane, shown, &focus(0))
        .expect("a far jump back asks");
    failed.end_landing(shown, &focus(0), true);
    assert!(
        !failed.landing_wait && failed.layout(shown).covers(failed.scroll, 0),
        "a failed window reveals the cursor where the rows are"
    );
}

#[test]
fn drift_asks_go_through_the_pane_slot_and_a_refused_window_is_not_asked_again() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "the first window covers the top"
    );
    grid.scroll_by(shown, &focus(0), 50. * 134.);
    let asked = grid
        .wanted_window(&mut pane, shown)
        .expect("a view past the held rows asks");
    assert_eq!(
        pane.requests
            .outstanding
            .map(|request| (request.start, request.count)),
        Some(asked)
    );
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "one window is out at a time"
    );
    let (first, last) = grid.layout(shown).viewport(grid.scroll);
    assert!(
        asked.0 as i64 <= first && (asked.0 + asked.1) as i64 > last,
        "the drift window covers the view"
    );
    assert!(answer(&mut pane));
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "held rows around the view ask nothing more"
    );
    assert_eq!(grid.counters.drift_asks, 1);
    let refused = flea_gui::pane::requests::WindowRequest {
        start: 0,
        count: 1,
        listing: pane.held.listing,
    };
    grid.scroll = 0.;
    let plan = grid.plan(&grid.layout(shown));
    pane.requests.refused = Some(flea_gui::pane::requests::WindowRequest {
        count: plan.size,
        ..refused
    });
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "a refused window is not asked again"
    );
}

#[test]
fn hidden_holds_nothing_and_asks_nothing_and_returns_on_the_cursor() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    grid.set_visible(false, shown, &focus(0));
    assert!(grid.hidden_held && !grid.visible);
    let sent = pane.requests.sent;
    assert_eq!(
        grid.follow_cursor(&mut pane, shown, &focus(1500)),
        None,
        "a hidden grid asks no landing"
    );
    grid.scroll = 40. * 134.;
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "a hidden grid asks no drift"
    );
    assert_eq!(
        grid.wanted_thumbnails(&pane, shown),
        None,
        "a hidden grid plans no thumbnails"
    );
    assert_eq!(pane.requests.sent, sent);
    grid.set_visible(true, shown, &focus(1500));
    let layout = grid.layout(shown);
    assert!(
        !grid.hidden_held && layout.covers(grid.scroll, 1500),
        "showing parks the view on the cursor before the first frame"
    );
    let mut end = board_view();
    end.set_visible(false, shown, &focus(1999));
    end.set_visible(true, shown, &focus(1999));
    let frame = layout.frame(layout.cell(end.scroll, 1999));
    assert!(
        frame.bottom() + 1. <= layout.height - layout.metrics.gap,
        "a restored last tile keeps its gap"
    );
}

#[test]
fn a_filter_maps_views_to_rows_and_asks_no_window() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let kept: Vec<usize> = (0..2000).filter(|row| row % 40 == 3).collect();
    let shown = Shown::Only(&kept);
    assert_eq!(
        (shown.total(), shown.at(0), shown.at(49), shown.at(50)),
        (50, Some(3), Some(1963), None)
    );
    assert_eq!(
        (shown.view_of(43), shown.view_of(44)),
        (Some(1), None),
        "a row the filter hides has no view"
    );
    assert_eq!(
        shown.span(0, 11),
        Some(3..=443),
        "the visible views span their listing rows"
    );
    assert_eq!(
        grid.layout(shown).rows(),
        9,
        "fifty kept rows lay out nine tile rows"
    );
    assert_eq!(
        grid.follow_cursor(&mut pane, shown, &focus(1963)),
        None,
        "a narrowed grid asks no landing"
    );
    assert!(!grid.landing_wait);
    assert!(
        grid.layout(shown).covers(grid.scroll, 49),
        "the cursor's view index is revealed"
    );
    assert_eq!(
        grid.wanted_window(&mut pane, shown),
        None,
        "a narrowed grid asks no drift"
    );
    assert_eq!(
        grid.follow_cursor(&mut pane, shown, &focus(44)),
        None,
        "a filtered-away cursor moves nothing"
    );
    assert_eq!(
        arrow(6, shown.view_of(44), shown.total(), Arrow::Down, false),
        None
    );
    assert_eq!(Shown::Only(&[]).span(0, 5), None);
}

#[test]
fn arrows_step_by_tiles_and_hold_at_row_edges_and_listing_ends() {
    let columns = 6;
    let at = |view: usize, step: Arrow| arrow(columns, Some(view), 20, step, false);
    assert_eq!(
        (
            at(7, Arrow::Down),
            at(7, Arrow::Up),
            at(7, Arrow::Left),
            at(7, Arrow::Right)
        ),
        (Some(13), Some(1), Some(6), Some(8))
    );
    assert_eq!(at(6, Arrow::Left), None, "left holds at a row start");
    assert_eq!(at(11, Arrow::Right), None, "right holds at a row end");
    assert_eq!(at(19, Arrow::Right), None, "right holds at the listing end");
    assert_eq!(
        at(15, Arrow::Down),
        None,
        "down holds past the partial last row"
    );
    assert_eq!(at(14, Arrow::Down), None);
    assert_eq!(at(13, Arrow::Down), Some(19));
    assert_eq!(at(3, Arrow::Up), None, "up holds on the first row");
    assert_eq!(
        (at(7, Arrow::ExtendDown), at(7, Arrow::ExtendUp)),
        (Some(13), Some(1)),
        "extends step as the arrows do"
    );
}

#[test]
fn zoom_travel_counts_qt_612_notches_and_touchpad_pixels() {
    assert_eq!(
        zoom_travel(ZoomInput {
            touchpad: false,
            pixels: -15.,
            angle: -120.
        }),
        -1.,
        "a phaseless Qt 6.12 notch is one step"
    );
    assert_eq!(
        zoom_travel(ZoomInput {
            touchpad: false,
            pixels: -10.,
            angle: 0.
        }),
        -10. / 48.,
        "pixels with no angle count"
    );
    assert_eq!(
        zoom_travel(ZoomInput {
            touchpad: true,
            pixels: 24.,
            angle: 120.
        }),
        0.5,
        "a 24 px touchpad stroke is half a step"
    );
    let pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    assert_eq!(
        grid.zoom(
            shown,
            &focus(0),
            ZoomInput {
                touchpad: false,
                pixels: 15.,
                angle: 120.
            }
        ),
        Some(ThumbSize::named("large")),
        "a notch takes medium to large"
    );
    assert_eq!(
        grid.zoom(
            shown,
            &focus(0),
            ZoomInput {
                touchpad: true,
                pixels: 24.,
                angle: 0.
            }
        ),
        None
    );
    assert_eq!(grid.zoom_travel, 0.5, "a half stroke stays as travel");
    for _ in 0..8 {
        grid.zoom(
            shown,
            &focus(0),
            ZoomInput {
                touchpad: false,
                pixels: 0.,
                angle: 120.,
            },
        );
    }
    assert_eq!(
        grid.thumb,
        ThumbSize::named("largest"),
        "zoom clamps at the largest stop"
    );
}

#[test]
fn zoom_and_resize_at_the_end_keep_the_lifted_last_row_and_the_first_tile_anchored() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    grid.follow_cursor(&mut pane, shown, &focus(1999));
    assert!(answer(&mut pane));
    grid.rows_landed(&mut pane, shown, &focus(1999));
    for input in [
        ZoomInput {
            touchpad: false,
            pixels: 0.,
            angle: 120.,
        },
        ZoomInput {
            touchpad: false,
            pixels: 0.,
            angle: -240.,
        },
    ] {
        grid.zoom(shown, &focus(1999), input)
            .expect("each notch moves a stop");
        let layout = grid.layout(shown);
        assert_eq!(
            grid.scroll,
            layout.reveal(grid.scroll, 1999),
            "{:?}: the end stays revealed and lifted",
            grid.thumb
        );
        assert!(grid.scroll <= layout.max_scroll());
    }
    grid.reflow(shown, &focus(1999), |grid| grid.size = (600., 640.));
    let narrow = grid.layout(shown);
    assert_eq!(narrow.columns, 3);
    assert_eq!(
        grid.scroll,
        narrow.reveal(grid.scroll, 1999),
        "narrowing at the end keeps the last row lifted"
    );
    let mut middle = board_view();
    middle.scroll = 40. * 134. + 9.;
    let anchor = middle.layout(shown).drawn(middle.scroll).start;
    middle.reflow(shown, &focus(0), |grid| grid.size = (600., 640.));
    let after = middle.layout(shown);
    assert_eq!(
        after.drawn(middle.scroll).start / after.columns,
        anchor / after.columns,
        "the first visible tile keeps its row on re-flow"
    );
    let mut taller = board_view();
    taller.scroll = 20. * 134. + 9.;
    taller.reflow(shown, &focus(0), |grid| grid.size = (908., 900.));
    assert_eq!(
        taller.scroll,
        20. * 134. + 9.,
        "a height-only change keeps the view"
    );
    let (first, last) = taller.layout(shown).viewport(taller.scroll);
    assert!(
        last - first + 1 > 30,
        "the taller view covers more tiles than before"
    );
    let asked = taller.wanted_window(&mut pane, shown);
    let held = pane
        .requests
        .outstanding
        .map_or((pane.held.start, pane.held.rows.len()), |request| {
            (request.start, request.count)
        });
    assert!(
        asked.is_some() || (held.0 as i64 <= first && (held.0 + held.1) as i64 > last),
        "the held or asked window covers the grown view"
    );
}

#[test]
fn a_rename_grows_every_cell_and_a_bottom_rename_stays_in_view() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    grid.follow_cursor(&mut pane, shown, &focus(1999));
    assert!(answer(&mut pane));
    grid.rows_landed(&mut pane, shown, &focus(1999));
    grid.reflow(shown, &focus(1999), |grid| grid.rename_extra = 14.);
    let layout = grid.layout(shown);
    assert_eq!(layout.cell_height, 148.);
    let frame = layout.frame(layout.cell(grid.scroll, 1999));
    assert!(
        frame.y >= 0. && frame.bottom() + 1. <= layout.height - layout.metrics.gap,
        "the renaming bottom tile and its taller cell stay whole in view"
    );
}
