use super::*;

fn rows(n: usize) -> Vec<RowFacts> {
    (0..n)
        .map(|i| RowFacts {
            name: format!("{i:06}"),
            ..RowFacts::default()
        })
        .collect()
}

fn listed(pane: &mut Pane, total: usize, count: usize) {
    let path = pane.path.clone();
    assert!(!pane.receive(Reply::Listed { path, total }).unwrap());
    assert!(pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(count),
            listing: 1
        })
        .unwrap());
}

#[test]
fn only_visible_rows_are_counted_including_a_partial_last_row() {
    assert_eq!(visible_rows(0.), 1);
    assert_eq!(visible_rows(31.), 1);
    assert_eq!(visible_rows(62.), 2);
    assert_eq!(visible_rows(63.), 3);
    assert_eq!(visible_rows(640.), 21);
    assert_eq!(window_size(21), 321);
}

#[test]
fn windows_follow_pane_qml_drift_rules_one_request_at_a_time() {
    let mut pane = Pane::new("/files".into(), 620.);
    assert_eq!(
        pane.wanted_window(),
        None,
        "no window while the listing is out"
    );
    listed(&mut pane, 100_000, 320);
    assert_eq!(pane.wanted_window(), None);
    // 320 held rows end at 320; the last visible row reaching within 25 of it asks around the first.
    pane.scroll_to(276. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((126, 320)));
    assert_eq!(
        pane.wanted_window(),
        None,
        "the window in flight counts as landed"
    );
    assert!(pane
        .receive(Reply::Rows {
            start: 126,
            rows: rows(320),
            listing: 1
        })
        .unwrap());
    assert_eq!(pane.wanted_window(), None);
    pane.scroll_to(140. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((0, 320)));
    // A jump past the held window asks around the new first row.
    pane.requests.cancel();
    pane.scroll_to(50_000. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((49_850, 320)));
}

#[test]
fn stale_and_oversized_rows_are_refused_and_the_cursor_stays_in_view() {
    let mut pane = Pane::new("/files".into(), 620.);
    listed(&mut pane, 1000, 320);
    assert!(pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(321),
            listing: 1
        })
        .is_err());
    assert!(pane
        .receive(Reply::Rows {
            start: 990,
            rows: rows(11),
            listing: 1
        })
        .is_err());
    assert!(!pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(3),
            listing: 0
        })
        .unwrap());
    assert!(pane.move_cursor(Move::End));
    assert_eq!(pane.cursor, 999);
    assert_eq!(pane.scroll_y, 1000. * ROW_HEIGHT - 620.);
    pane.move_cursor(Move::PageUp);
    assert_eq!(pane.cursor, 979);
    pane.move_cursor(Move::Home);
    assert_eq!((pane.cursor, pane.scroll_y), (0, 0.));
}

#[test]
fn a_listing_stays_in_flight_until_its_rows_land_and_each_open_request_counts() {
    let mut pane = Pane::new("/files".into(), 620.);
    assert!(pane.listing_in_flight(), "the opening listing is out");
    assert_eq!(pane.pending(), 1);
    assert!(!pane
        .receive(Reply::Listed {
            path: "/files".into(),
            total: 1000
        })
        .unwrap());
    assert!(
        pane.listing_in_flight(),
        "a listed line alone lands no rows"
    );
    assert_eq!(pane.pending(), 1);
    assert!(pane
        .receive(Reply::Rows {
            start: 0,
            rows: rows(320),
            listing: 1
        })
        .unwrap());
    assert!(!pane.listing_in_flight());
    assert_eq!(pane.pending(), 0);
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((450, 320)));
    assert!(
        !pane.listing_in_flight(),
        "a held-window request is not a listing"
    );
    assert_eq!(pane.pending(), 1);
    assert!(pane
        .receive(Reply::Rows {
            start: 450,
            rows: rows(320),
            listing: 1
        })
        .unwrap());
    assert_eq!(pane.pending(), 0);
    assert!(pane.begin("/other".into(), Options::default(), pane.now()));
    assert!(
        pane.listing_in_flight(),
        "a hop elsewhere is out until it lands"
    );
    assert_eq!(pane.pending(), 1);
}

#[test]
fn first_rows_at_stamps_the_first_non_empty_rows_once_in_wall_clock_ms() {
    let mut pane = Pane::new("/srv".into(), 640.);
    assert_eq!(pane.first_rows_at, 0);
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    listed(&mut pane, 3, 3);
    let first = pane.first_rows_at;
    assert!(first >= before, "the stamp is epoch milliseconds");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let _ = pane.receive(Reply::Rows {
        start: 0,
        rows: rows(3),
        listing: 1,
    });
    assert_eq!(pane.first_rows_at, first, "later rows keep the first stamp");
}
