use super::*;
use flea_gui::navigation::{history::Stack, Refusal};
use flea_gui::pane::swap::Options;

#[test]
fn the_newest_navigation_wins_and_a_stale_answer_for_the_one_let_go_draws_nothing() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    pane.cursor = 3;
    assert_eq!(nav.open(&mut pane, "/f/a"), Ok(true));
    assert_eq!(nav.open(&mut pane, "/f/b"), Ok(true));
    assert_eq!(lists(&nav.take()), ["/f/a", "/f/b"]);
    assert_eq!(
        pane.path(),
        "/f/base",
        "the drawn folder stays until an answer lands"
    );
    assert_eq!(pane.place(), "/f/b");
    assert_eq!(
        nav.history.paths(Stack::Back),
        ["/f/base"],
        "a place that never landed is not pushed"
    );
    assert!(nav.history.forward.is_empty());
    assert!(
        !land(&mut pane, &mut nav, &mut sel, ("/f/a", 10, 0, 2), named),
        "stale A after B"
    );
    assert_eq!(pane.path(), "/f/base");
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/b", 5, 0, 3),
        named
    ));
    assert_eq!(pane.path(), "/f/b");
    assert_eq!(nav.back(&mut pane), Ok(true));
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 40, 0, 4),
        named
    ));
    assert_eq!(pane.cursor, 3, "Back lands on the row it left, by name");
    assert_eq!(nav.history.paths(Stack::Forward), ["/f/b"]);
}

#[test]
fn two_rapid_backs_step_twice_and_keep_the_pending_place_on_the_forward_branch() {
    let (mut pane, mut nav, mut sel) = settled("/f/c", 40);
    nav.history.push(
        Stack::Back,
        flea_gui::navigation::history::Entry::new("/f/a", named(7), 7),
    );
    nav.history.push(
        Stack::Back,
        flea_gui::navigation::history::Entry::new("/f/b", named(2), 2),
    );
    assert_eq!(nav.back(&mut pane), Ok(true));
    assert_eq!(nav.back(&mut pane), Ok(true));
    assert_eq!(lists(&nav.take()), ["/f/b", "/f/a"]);
    assert_eq!(nav.history.paths(Stack::Forward), ["/f/c", "/f/b"]);
    assert!(nav.history.back.is_empty());
    assert!(!land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/b", 40, 0, 2),
        named
    ));
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/a", 40, 0, 3),
        named
    ));
    assert_eq!((pane.path(), pane.cursor), ("/f/a", 7));
    assert_eq!(nav.forward(&mut pane), Ok(true));
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/b", 40, 0, 4),
        named
    ));
    assert_eq!(
        pane.cursor, 2,
        "the forward entry carries the row the pending place owed"
    );
}

#[test]
fn an_open_over_a_pending_back_keeps_the_drawn_place_and_clears_the_forward_branch() {
    let (mut pane, mut nav, _) = settled("/f/gm", 40);
    nav.history.back = vec![
        flea_gui::navigation::history::Entry::new("/", "", 0),
        flea_gui::navigation::history::Entry::new("/f", "", 0),
    ];
    nav.open(&mut pane, "/f/x").unwrap();
    nav.back(&mut pane).unwrap();
    assert_eq!(nav.history.paths(Stack::Forward), ["/f/x"]);
    nav.open(&mut pane, "/f/y").unwrap();
    assert_eq!(nav.history.paths(Stack::Back), ["/", "/f", "/f/gm"]);
    assert!(nav.history.forward.is_empty());
}

#[test]
fn back_owed_a_row_past_the_first_window_locates_it_and_lands_once_in_its_own_window() {
    let mut pane = Pane::new("/f/base".into(), 620.);
    let mut nav = navigator("{}");
    let mut sel = Vec::new();
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 1000, 680, 1),
        named
    ));
    pane.cursor = 800;
    nav.open(&mut pane, "/f/x").unwrap();
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/x", 3, 0, 2),
        named
    ));
    nav.take();
    nav.back(&mut pane).unwrap();
    assert!(!land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 1000, 0, 3),
        named
    ));
    let commands = nav.take();
    assert_eq!(
        commands[1..],
        [Command::Locate {
            path: "/f/base/file-00800".into()
        }]
    );
    assert_eq!(pane.path(), "/f/x", "the parked pair waits");
    assert!(
        !nav.take_located(&mut pane, "/f/other", "/f/base/file-00800", 800),
        "foreign directory"
    );
    assert!(nav.take_located(&mut pane, "/f/base", "/f/base/file-00800", 800));
    assert_eq!(
        nav.take(),
        [Command::Window {
            start: 800,
            count: 320
        }]
    );
    let now = pane.now();
    pane.receive_at(
        Reply::Rows {
            start: 800,
            rows: rows(800, 200, named),
            listing: 3,
        },
        now,
    )
    .unwrap();
    assert!(pane.tick(now));
    nav.after_reply(&mut pane, "/f/x", &mut sel);
    assert_eq!(
        (pane.path(), pane.cursor, pane.held.start),
        ("/f/base", 800, 800)
    );
    assert_eq!(
        pane.swap.counters.commits, 3,
        "one commit for the restored listing"
    );
}

#[test]
fn up_selects_the_folder_left_climbs_from_a_pending_listing_and_stops_at_root() {
    let (mut pane, mut nav, mut sel) = settled("/f/base/sub", 4);
    nav.parent(&mut pane).unwrap();
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert_eq!(
        (listing.path.as_str(), listing.select.as_deref()),
        ("/f/base", Some("/f/base/sub"))
    );
    let name = |i| if i == 2 { "sub".to_string() } else { named(i) };
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 4, 0, 2),
        name
    ));
    assert_eq!((pane.cursor, sel.clone()), (2, vec![2]));
    nav.open(&mut pane, "/f/x/y").unwrap();
    nav.take();
    nav.parent(&mut pane).unwrap();
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert_eq!(
        (listing.path.as_str(), listing.select.as_deref()),
        ("/f/x", Some("/f/x/y"))
    );
    let (mut root, mut nav, _) = settled("/", 4);
    assert_eq!(nav.parent(&mut root), Ok(false));
    assert!(nav.take().is_empty());
}

#[test]
fn a_requested_spelling_is_kept_byte_for_byte_and_another_spelling_is_not_its_answer() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 4);
    nav.open(&mut pane, "/f/a/").unwrap();
    assert_eq!(lists(&nav.take()), ["/f/a/"]);
    assert!(!land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/a", 4, 0, 2),
        named
    ));
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/a/", 4, 0, 3),
        named
    ));
    assert_eq!(pane.path(), "/f/a/");
}

#[test]
fn deep_select_rides_the_first_list_and_a_shown_folder_is_a_no_op() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 4);
    assert_eq!(nav.open_place(&mut pane, "/f/base"), Ok(false));
    assert_eq!(nav.open_place(&mut pane, "/f/base/"), Ok(false));
    nav.pending_select = Some("/f/deep/file-00900".into());
    nav.open(&mut pane, "/f/deep").unwrap();
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert_eq!(listing.select.as_deref(), Some("/f/deep/file-00900"));
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/deep", 1000, 680, 2),
        named
    ));
    assert_eq!((pane.cursor, sel.clone()), (900, vec![900]));
    assert_eq!(nav.counters.windows, 0, "no second first-window round trip");
}

#[test]
fn a_busy_pane_refuses_a_plain_relist_and_enter_on_an_unloaded_row() {
    let (mut pane, mut nav, _) = settled("/f/base", 1000);
    pane.cursor = 900;
    assert_eq!(nav.open_cursor(&mut pane), Err(Refusal::NotLoaded));
    nav.open(&mut pane, "/f/x").unwrap();
    assert_eq!(nav.refresh(&mut pane, None), Err(Refusal::Busy));
    assert_eq!(Refusal::Busy.key(), "pane.busy");
    assert_eq!(
        nav.mouse_back(&mut pane),
        Ok(true),
        "Back supersedes a listing out"
    );
}

#[test]
fn a_refused_listing_climbs_from_the_folder_it_asked_for_and_drops_the_owed_landing() {
    let (mut pane, mut nav, _) = settled("/f/base", 4);
    nav.open(&mut pane, "/f/base/locked").unwrap();
    nav.take();
    let now = pane.now();
    assert!(pane.fail("scan", "/f/base/locked", "Permission denied", 0o40700, now));
    nav.after_failure(&mut pane, "scan");
    assert!(nav.restore.is_none() && nav.path_failed(&pane));
    nav.parent(&mut pane).unwrap();
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert_eq!(
        (listing.path.as_str(), listing.select.as_deref()),
        ("/f/base", Some("/f/base/locked"))
    );
}

#[test]
fn a_search_landing_keeps_the_armed_return_select_for_the_search_exit() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    nav.pending_select = Some("/f/base/file-00030".into());
    let search = Options {
        search: true,
        supersede: true,
        ..Options::default()
    };
    nav.open_without_history(&mut pane, "/f/base", search, None, false)
        .unwrap();
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 5, 0, 2),
        named
    ));
    assert!(pane.state.search);
    assert_eq!(
        nav.pending_select.as_deref(),
        Some("/f/base/file-00030"),
        "a walk's landing leaves it armed"
    );
}
