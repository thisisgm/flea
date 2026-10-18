use super::*;

fn parked() -> Pane {
    let mut pane = settled();
    assert!(pane.begin(
        "/fixture/b".into(),
        Options {
            restore_name: Some("missing".into()),
            ..Options::default()
        },
        1000,
    ));
    header(&mut pane, "/fixture/b", 1000, 1040);
    assert!(!answer(&mut pane, 0, 320, 2, 1040));
    assert!(pane.anchor_window(680).is_some());
    pane
}

#[test]
fn refused_anchor_must_not_block_future_windows() {
    let mut pane = parked();
    assert!(pane.fail("window", "/fixture/b", "refused", 0, 1042));
    assert_eq!(pane.state.kind, Kind::Ready);
    assert!(!pane.busy());
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(
        pane.wanted_window(),
        Some((450, 320)),
        "refused anchor still owns the only request slot: {:?}",
        pane.requests.outstanding,
    );
}

#[test]
fn expired_anchor_must_not_block_future_windows() {
    let mut pane = parked();
    pane.tick(1066);
    assert!(pane.tick(11040));
    assert!(!pane.busy());
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(
        pane.wanted_window(),
        Some((450, 320)),
        "timed-out anchor still owns the only request slot: {:?}",
        pane.requests.outstanding,
    );
}

#[test]
fn same_path_sort_race_must_converge_to_the_actual_relist() {
    let mut pane = settled();
    let old_action = pane.row_action(vec![0]).unwrap();
    assert!(pane.begin("/fixture/a".into(), Options::default(), 1000));
    for (listing, total, name, now) in [
        (2, 1000, "sorted-before-relist", 1010),
        (3, 1001, "actual-relist", 1040),
    ] {
        header(&mut pane, "/fixture/a", total, now);
        assert!(pane
            .receive_at(
                Reply::Rows {
                    start: 0,
                    rows: vec![
                        RowFacts {
                            name: name.into(),
                            ..RowFacts::default()
                        };
                        320
                    ],
                    listing,
                },
                now,
            )
            .unwrap());
    }
    assert_eq!(
        (
            pane.held.listing,
            pane.held.total,
            pane.held.rows[0].name.as_str()
        ),
        (3, 1001, "actual-relist"),
        "the actual relist was discarded after a same-path sort ended the hold",
    );
    assert_eq!(pane.row_action(vec![0]).unwrap().listing, 3);
    assert!(pane.accepts_work(3));
    assert!(!pane.accepts_work(old_action.listing));
    assert!(!pane.accepts_work(2));
}

#[test]
fn valid_anchor_survives_unrelated_failure_and_the_drawn_minimum() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    pane.tick(1066);
    pane.frame(1066, true);
    header(&mut pane, "/fixture/b", 1000, 1100);
    assert!(!answer(&mut pane, 0, 320, 2, 1100));
    let request = pane.anchor_window(680).unwrap();
    assert!(!pane.fail("window", "/fixture/a", "refused", 0, 1110));
    assert_eq!(pane.requests.outstanding, Some(request));
    assert!(pane.tick(1166));
    assert_eq!(pane.requests.outstanding, Some(request));
    assert!(answer(&mut pane, 680, 320, 2, 1170));
    assert_eq!(pane.held.start, 680);
    assert!(pane.requests.outstanding.is_none());
}

#[test]
fn refused_parked_anchor_retires_before_the_drawn_minimum_ends() {
    let mut pane = settled();
    pane.begin(
        "/fixture/b".into(),
        Options {
            restore_name: Some("missing".into()),
            ..Options::default()
        },
        1000,
    );
    pane.tick(1066);
    pane.frame(1066, true);
    header(&mut pane, "/fixture/b", 1000, 1100);
    assert!(!answer(&mut pane, 0, 320, 2, 1100));
    assert!(pane.anchor_window(680).is_some());
    assert!(!pane.fail("window", "/fixture/b", "refused", 0, 1101));
    assert!(pane.requests.outstanding.is_none());
    assert!(pane.busy());
    assert!(pane.tick(1166));
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((450, 320)));
}

#[test]
fn unheld_pairs_require_complete_newer_rows_and_retire_only_the_old_numbering() {
    let mut pane = settled();
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), Some((450, 320)));
    let old = Rc::clone(&pane.held);
    let request = pane.requests.outstanding;
    header(&mut pane, "/fixture/a", 1001, 1000);
    assert!(Rc::ptr_eq(&old, &pane.held));
    assert_eq!(pane.requests.outstanding, request);
    assert!(pane
        .receive_at(
            Reply::Rows {
                start: 0,
                rows: rows(319),
                listing: 2,
            },
            1010,
        )
        .is_err());
    assert!(Rc::ptr_eq(&old, &pane.held));
    assert_eq!(pane.requests.outstanding, request);
    header(&mut pane, "/fixture/other", 1001, 1020);
    assert!(!answer(&mut pane, 0, 320, 2, 1020));
    assert_eq!(pane.requests.outstanding, request);
    header(&mut pane, "/fixture/a", 1001, 1040);
    assert!(answer(&mut pane, 0, 320, 2, 1040));
    assert!(pane.requests.outstanding.is_none());
    assert_eq!(pane.wanted_window(), Some((450, 320)));
    let current = Rc::clone(&pane.held);
    let request = pane.requests.outstanding;
    for listing in [0, 1, 2] {
        header(&mut pane, "/fixture/a", 999, 1050);
        assert!(!answer(&mut pane, 0, 320, listing, 1050));
        assert!(Rc::ptr_eq(&current, &pane.held));
        assert_eq!(pane.requests.outstanding, request);
    }
    assert!(answer(&mut pane, 450, 320, 2, 1060));
    assert_eq!((pane.held.listing, pane.held.total), (2, 1001));
}
