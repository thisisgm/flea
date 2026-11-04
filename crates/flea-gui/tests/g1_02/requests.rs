use super::*;
use flea_gui::pane::{requests, visible_rows, window_size};

#[test]
fn every_geometry_asks_only_visible_plus_300_and_first_window_rides_the_list() {
    for height in [0., 31., 63., 619., 899., 1386.] {
        let pane = Pane::new("/fixture/a".into(), height);
        assert_eq!(pane.first_count(), visible_rows(height) + 300);
        assert_eq!(pane.first_count(), window_size(pane.visible_rows()));
        assert_eq!(pane.list_requests, 1);
        assert!(pane.requests.outstanding.is_none());
    }
}

#[test]
fn one_outstanding_window_survives_far_jumps_and_unrelated_replies() {
    let mut pane = settled();
    assert_eq!(pane.wanted_window(), None);
    pane.scroll_to(275. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), None);
    pane.scroll_to(276. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((126, 320)));
    let original = Rc::clone(&pane.held);
    pane.scroll_to(800. * ROW_HEIGHT);
    for (start, listing) in [(0, 1), (126, 0), (126, 2)] {
        assert_eq!(pane.wanted_window(), None);
        assert!(!answer(&mut pane, start, 10, listing, 1000));
        assert!(Rc::ptr_eq(&original, &pane.held));
    }
    assert!(answer(&mut pane, 126, 320, 1, 1010));
    assert_eq!(pane.wanted_window(), Some((650, 320)));
    assert_eq!(pane.requests.sent, 2);
}

#[test]
fn deep_first_window_places_the_cursor_without_an_extra_round_trip() {
    let mut pane = Pane::new("/fixture/a".into(), 620.);
    header(&mut pane, "/fixture/a", 1000, 1);
    assert!(answer(&mut pane, 680, 320, 1, 2));
    assert_eq!(pane.cursor, 680);
    assert_eq!(pane.wanted_window(), None);
    assert_eq!(pane.requests.sent, 0);
}

#[test]
fn oversized_out_of_bounds_and_incomplete_nonempty_windows_are_refused() {
    for (start, count, total) in [
        (0, 321, 1000),
        (990, 11, 1000),
        (1, 0, 0),
        (0, 0, 3),
        (0, 1, 3),
    ] {
        let mut pane = Pane::new("/fixture/a".into(), 620.);
        header(&mut pane, "/fixture/a", total, 1);
        assert!(pane
            .receive_at(
                Reply::Rows {
                    start,
                    rows: rows(count),
                    listing: 1
                },
                1
            )
            .is_err());
        assert!(pane.busy());
        assert_eq!(pane.held.total, 0);
    }
}

#[test]
fn row_capture_preserves_identity_and_late_actions_are_refused() {
    let mut pane = settled();
    let action = pane.row_action(vec![0, 999]).unwrap();
    assert_eq!(action.listing, 1);
    assert!(requests::current(&action, &pane.held, pane.busy()));
    assert!(pane.row_action(vec![]).is_none());
    assert!(pane.row_action(vec![1000]).is_none());
    pane.begin("/fixture/a".into(), Options::default(), 1000);
    assert!(!requests::current(&action, &pane.held, pane.busy()));
    assert!(!pane.accepts_work(1));
    header(&mut pane, "/fixture/a", 3, 1040);
    assert!(answer(&mut pane, 0, 3, 2, 1040));
    assert!(!requests::current(&action, &pane.held, false));
    assert!(!pane.accepts_work(1));
    assert!(pane.accepts_work(2));
}

#[test]
fn action_gate_allows_navigation_but_swallows_every_row_action() {
    for action in [
        "cursorDown",
        "cursorFirst",
        "pageDown",
        "toggleSelect",
        "selectAll",
        "trashArm",
        "trash",
        "copy",
        "cut",
        "paste",
        "menu",
        "preview",
        "rename",
        "duplicate",
        "properties",
        "deletePermanently",
        "filter",
        "search",
        "sortNext",
        "toggleHidden",
        "newFolder",
        "undo",
    ] {
        assert!(requests::swallows(true, action), "{action}");
        assert!(!requests::swallows(false, action), "{action}");
    }
    for action in [
        "",
        "open",
        "parent",
        "historyBack",
        "historyForward",
        "escape",
        "viewList",
        "viewColumns",
        "viewGrid",
        "viewDual",
        "sidebar",
        "windowNew",
        "togglePreview",
        "reload",
    ] {
        assert!(!requests::swallows(true, action), "{action}");
    }
}

#[test]
fn a_window_failure_ends_only_that_window_and_allows_a_retry() {
    let mut pane = settled();
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((450, 320)));
    assert!(!pane.fail("window", "/fixture/other", "timed out", 0, 1000));
    assert_eq!(pane.wanted_window(), None);
    assert!(!pane.fail("window", "/fixture/a", "timed out", 0, 1000));
    assert_eq!(pane.wanted_window(), Some((450, 320)));
    assert_eq!(pane.state.kind, Kind::Ready);
}
