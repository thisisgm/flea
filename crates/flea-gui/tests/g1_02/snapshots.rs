use super::*;

#[test]
fn old_path_rows_count_cursor_and_query_remain_one_snapshot_until_the_pair() {
    let mut pane = settled();
    pane.cursor = 100;
    pane.scroll_to(80. * ROW_HEIGHT);
    pane.state.query = "kept".into();
    let old = Rc::clone(&pane.held);
    assert!(pane.begin("/fixture/b".into(), Options::default(), 1000));
    for time in [1000, 1020, 1040] {
        pane.frame(time, false);
        assert!(Rc::ptr_eq(&old, &pane.held));
        assert_eq!(pane.path(), "/fixture/a");
        assert_eq!(
            (pane.held.total, pane.cursor, pane.scroll_y),
            (1000, 100, 2480.)
        );
        assert_eq!(pane.state.query, "kept");
        assert!(!pane.accepts_work(1));
        assert!(pane.row_action(vec![0]).is_none());
    }
    header(&mut pane, "/fixture/b", 2, 1040);
    assert_eq!(pane.held.total, 1000);
    assert!(answer(&mut pane, 0, 2, 2, 1040));
    assert_eq!(
        (pane.path(), pane.held.total, pane.held.rows.len()),
        ("/fixture/b", 2, 2)
    );
    assert_eq!((pane.cursor, pane.scroll_y), (0, 0.));
    assert_eq!(pane.swap.counters.commits, 2);
    assert_eq!(pane.swap.counters.blank_frames, 0);
}

#[test]
fn wrong_path_sort_and_its_rows_cannot_land_under_the_new_path() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    header(&mut pane, "/fixture/a", 900, 1010);
    assert!(!answer(&mut pane, 0, 3, 2, 1010));
    assert_eq!(pane.path(), "/fixture/a");
    header(&mut pane, "/fixture/b", 2, 1040);
    assert!(answer(&mut pane, 0, 2, 3, 1040));
    assert_eq!(pane.path(), "/fixture/b");
}

#[test]
fn same_path_relist_drops_old_numbering_and_duplicate_pair_commits_once() {
    let mut pane = settled();
    pane.begin("/fixture/a".into(), Options::default(), 1000);
    header(&mut pane, "/fixture/a", 4, 1010);
    assert!(!answer(&mut pane, 0, 4, 1, 1010));
    assert!(pane.busy());
    assert!(answer(&mut pane, 0, 4, 2, 1040));
    let original = Rc::clone(&pane.held);
    header(&mut pane, "/fixture/a", 4, 1041);
    assert!(!answer(&mut pane, 0, 4, 2, 1041));
    assert!(Rc::ptr_eq(&original, &pane.held));
    assert_eq!(pane.swap.counters.commits, 2);
}

#[test]
fn rows_before_header_and_superseded_headers_are_dropped() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    assert!(!answer(&mut pane, 0, 2, 2, 1001));
    assert!(!pane.begin(
        "/fixture/b".into(),
        Options {
            supersede: true,
            ..Options::default()
        },
        1001
    ));
    assert!(!pane.begin("/fixture/c".into(), Options::default(), 1002));
    assert!(pane.begin(
        "/fixture/c".into(),
        Options {
            supersede: true,
            ..Options::default()
        },
        1003
    ));
    header(&mut pane, "/fixture/b", 2, 1010);
    assert!(!answer(&mut pane, 0, 2, 2, 1010));
    header(&mut pane, "/fixture/c", 3, 1040);
    assert!(answer(&mut pane, 0, 3, 3, 1040));
    assert_eq!((pane.path(), pane.held.total), ("/fixture/c", 3));
}

#[test]
fn an_empty_pair_commits_together_and_remains_holdable() {
    let mut pane = settled();
    pane.begin("/fixture/empty".into(), Options::default(), 1000);
    header(&mut pane, "/fixture/empty", 0, 1020);
    assert_eq!(pane.state.kind, Kind::Ready);
    assert!(answer(&mut pane, 0, 0, 2, 1040));
    assert_eq!(pane.state.kind, Kind::Empty);
    assert_eq!((pane.path(), pane.held.total), ("/fixture/empty", 0));
    pane.begin("/fixture/b".into(), Options::default(), 2000);
    assert!(pane.swap.holding);
    assert_eq!(pane.state.kind, Kind::Empty);
}

#[test]
fn stale_window_and_abandoned_scan_failures_leave_the_live_hold_intact() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    let old = Rc::clone(&pane.held);
    for (operation, path) in [
        ("stale", ""),
        ("sort", "size"),
        ("window", "/fixture/a"),
        ("scan", "/fixture/abandoned"),
    ] {
        assert!(!pane.fail(operation, path, "refused", 0, 1010));
        assert!(Rc::ptr_eq(&old, &pane.held));
        assert!(pane.busy());
    }
    assert!(pane.fail("scan", "/fixture/b", "not found", 0, 1040));
    assert!(!pane.busy());
    assert_eq!(pane.state.kind, Kind::Error);
    assert_eq!(pane.state.target, "/fixture/b");
    assert_eq!(pane.swap.counters.last_end, "dropped");
}

#[test]
fn held_rows_reject_cursor_and_window_work() {
    let mut pane = settled();
    pane.cursor = 20;
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    for step in [
        Move::Up,
        Move::Down,
        Move::PageUp,
        Move::PageDown,
        Move::Home,
        Move::End,
    ] {
        assert!(!pane.move_cursor(step));
        assert_eq!(pane.cursor, 20);
    }
    pane.scroll_to(600. * ROW_HEIGHT);
    assert_eq!(pane.wanted_window(), None);
}

#[test]
fn kept_query_and_walk_exit_land_with_the_rows() {
    let mut pane = settled();
    pane.state.search = true;
    pane.state.query = "old search".into();
    pane.begin(
        "/fixture/b".into(),
        Options {
            hold_walk: true,
            kept_query: "filter".into(),
            ..Options::default()
        },
        1000,
    );
    assert!(pane.swap.holding);
    assert_eq!(pane.state.query, "old search");
    header(&mut pane, "/fixture/b", 2, 1040);
    assert!(answer(&mut pane, 0, 2, 2, 1040));
    assert!(!pane.state.search);
    assert_eq!(pane.state.query, "filter");
}

#[test]
fn changed_view_and_active_search_do_not_hold_incompatible_rows() {
    for search in [false, true] {
        let mut pane = settled();
        pane.state.search = search;
        pane.begin(
            "/fixture/b".into(),
            Options {
                clear_at_once: !search,
                ..Options::default()
            },
            1000,
        );
        assert!(!pane.swap.holding);
        assert_eq!(pane.state.kind, Kind::Loading);
        assert_eq!(pane.held.total, 0);
    }
}

#[test]
fn search_generations_and_live_pairs_are_filtered_without_a_directory_hold() {
    let mut pane = settled();
    pane.swap.session = flea_gui::pane::swap::Session {
        open: true,
        generation: 3,
        opened_generation: 1,
        query: "needle".into(),
        leaving: false,
    };
    for generation in [1, 2, 4] {
        assert!(pane.swap.session.drops(generation));
        assert!(!pane
            .receive_session(
                Reply::Listed {
                    path: "/fixture/search".into(),
                    total: 2
                },
                generation,
                1000
            )
            .unwrap());
        assert!(pane.swap.pending.is_none());
    }
    assert!(!pane.swap.session.drops(3));
    assert!(!pane.swap.session.drops(0));
    pane.swap.session.leaving = true;
    assert!(pane.swap.session.drops(3));
    pane.swap.session.leaving = false;
    pane.swap.session.open = false;
    assert!(pane.swap.session.drops(3));
    pane.swap.session.open = true;
    assert!(!pane
        .receive_session(
            Reply::Listed {
                path: "/fixture/search".into(),
                total: 2
            },
            3,
            1000
        )
        .unwrap());
    assert!(!pane.busy());
    assert_eq!(pane.path(), "/fixture/a");
    assert!(pane
        .receive_session(
            Reply::Rows {
                start: 0,
                rows: rows(2),
                listing: 2
            },
            3,
            1040
        )
        .unwrap());
    assert_eq!(
        (pane.path(), pane.state.query.as_str()),
        ("/fixture/search", "needle")
    );
    assert!(pane.state.search);
    assert_eq!(pane.swap.counters.holds, 0);
}
