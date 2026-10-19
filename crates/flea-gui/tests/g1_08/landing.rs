//! Review g1-08-r1-1: a far landing never overwrites the pane's outstanding window; it waits behind it and asks once the slot is free.
use super::{all, answer, board_view, focus, listed, named};
use flea::protocol::Reply;

#[test]
fn a_far_landing_waits_behind_an_outstanding_drift_window_and_asks_once_it_lands() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    grid.scroll_by(shown, &focus(0), 50. * 134.);
    let drift = grid
        .wanted_window(&mut pane, shown)
        .expect("a scrolled view asks its drift window");
    assert_eq!(
        grid.follow_cursor(&mut pane, shown, &focus(1999)),
        None,
        "End holds its ask while the drift window is out"
    );
    assert_eq!(
        pane.requests
            .outstanding
            .map(|request| (request.start, request.count)),
        Some(drift),
        "the drift window stays the outstanding one"
    );
    assert!(grid.landing_wait, "End waits for its rows");
    let end = (drift.0 + drift.1).min(pane.held.total);
    let listing = pane.held.listing;
    let landed = pane.receive_at(
        Reply::Rows {
            start: drift.0,
            rows: (drift.0..end).map(named).collect(),
            listing,
        },
        2,
    );
    assert!(
        landed.is_ok(),
        "the drift window's own reply lands: {landed:?}"
    );
    assert_eq!(
        grid.rows_landed(&mut pane, shown, &focus(1999)),
        Some((1937, 63)),
        "the held landing asks once the slot is free"
    );
    assert!(answer(&mut pane), "the landing's reply lands");
    assert_eq!(grid.rows_landed(&mut pane, shown, &focus(1999)), None);
    assert!(
        !grid.landing_wait && grid.scroll == grid.layout(shown).reveal(50. * 134., 1999),
        "End lands on its reply, lifted in one pass"
    );
}

#[test]
fn end_then_home_before_ends_reply_keeps_ends_reply_valid_and_lands_home() {
    let mut pane = listed(2000, 640.);
    let mut grid = board_view();
    let shown = all(&pane);
    grid.scroll_by(shown, &focus(0), grid.layout(shown).max_scroll() / 2.);
    grid.wanted_window(&mut pane, shown)
        .expect("the middle asks its drift window");
    assert!(answer(&mut pane), "the middle window lands");
    let end_ask = grid
        .follow_cursor(&mut pane, shown, &focus(1999))
        .expect("End asks");
    assert_eq!(
        grid.follow_cursor(&mut pane, shown, &focus(0)),
        None,
        "Home holds its ask behind End's"
    );
    let stop = (end_ask.0 + end_ask.1).min(pane.held.total);
    let listing = pane.held.listing;
    let landed = pane.receive_at(
        Reply::Rows {
            start: end_ask.0,
            rows: (end_ask.0..stop).map(named).collect(),
            listing,
        },
        2,
    );
    assert!(
        landed.is_ok(),
        "End's reply after Home still lands: {landed:?}"
    );
    let home = grid
        .rows_landed(&mut pane, shown, &focus(0))
        .expect("Home asks once End's reply frees the slot");
    assert_eq!(home.0, 0);
    assert!(answer(&mut pane), "Home's reply lands");
    assert_eq!(grid.rows_landed(&mut pane, shown, &focus(0)), None);
    assert!(
        !grid.landing_wait && grid.scroll == grid.layout(shown).row_top(0),
        "Home lands on the first row"
    );
}
