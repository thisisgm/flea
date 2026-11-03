use super::*;
use flea_gui::pane::{requests::RowAction, ROW_HEIGHT};

// A file another program created sorts first, so every old row moves down by one.
fn shifted(index: usize) -> String {
    if index == 0 {
        "a-new".into()
    } else {
        named(index - 1)
    }
}

#[test]
fn a_watched_reread_reanchors_cursor_marks_and_view_offset_by_filename() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    pane.cursor = 100;
    pane.scroll_to(90. * ROW_HEIGHT);
    sel.extend([50, 900]);
    nav.changed(&pane, "/f/base", 0);
    assert!(nav.fire_watch(&mut pane, &sel, 400));
    assert_eq!(
        nav.take(),
        [Command::Paths(RowAction {
            listing: 1,
            rows: vec![900]
        })]
    );
    assert!(nav.gate.paths_pending);
    assert!(nav.take_paths(&mut pane, &["/f/base/file-00900".into()]));
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert_eq!(
        listing.select.as_deref(),
        Some("/f/base/file-00100"),
        "the cursor's file rides the list"
    );
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 1001, 0, 2),
        shifted
    ));
    assert_eq!(pane.cursor, 101);
    assert_eq!(
        pane.scroll_y,
        91. * ROW_HEIGHT,
        "the cursor keeps its screen offset"
    );
    let commands = nav.take();
    let Command::LocateMany { paths, id } = &commands[0] else {
        panic!("{commands:?}")
    };
    assert_eq!(paths, &["/f/base/file-00900".to_string()]);
    assert!(sel.is_empty(), "marks land once, not half");
    assert!(
        nav.busy(&pane),
        "an unfinished anchor holds the next re-read"
    );
    assert!(nav.take_located_many(
        &pane,
        "/f/base",
        *id,
        true,
        &[("/f/base/file-00900".into(), 901)],
        &mut sel
    ));
    assert_eq!(
        sel,
        [51, 901],
        "zero index-based retargeting: marks follow their files"
    );
    assert!(!nav.busy(&pane));
}

#[test]
fn a_deleted_anchor_lands_on_its_old_index_and_a_gone_mark_stays_gone() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 300);
    pane.cursor = 10;
    sel.extend([10, 299]);
    nav.changed(&pane, "/f/base", 0);
    assert!(nav.fire_watch(&mut pane, &sel, 400));
    nav.take();
    let gone = |i: usize| if i >= 10 { named(i + 1) } else { named(i) };
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 299, 0, 2),
        gone
    ));
    assert_eq!(pane.cursor, 10, "whatever took the deleted row's place");
    let commands = nav.take();
    let Some(Command::LocateMany { id, paths }) = commands.first() else {
        panic!()
    };
    assert_eq!(paths, &["/f/base/file-00010".to_string()]);
    assert!(nav.take_located_many(&pane, "/f/base", *id, true, &[], &mut sel));
    assert_eq!(sel, [298], "file-00299 is now row 298; file-00010 is gone");
}

#[test]
fn a_navigation_while_the_anchor_waits_drops_it_so_the_debt_never_travels() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    pane.cursor = 5;
    nav.changed(&pane, "/f/base", 0);
    assert!(nav.fire_watch(&mut pane, &sel, 400));
    nav.open(&mut pane, "/f/b").unwrap();
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/b", 40, 0, 2),
        named
    ));
    assert!(nav.anchor.is_none());
    assert_eq!(pane.cursor, 0);
}

#[test]
fn a_manual_reload_asks_for_the_change_count_and_keeps_the_cursor_file() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 40);
    pane.cursor = 7;
    assert!(nav.reread(&mut pane, &sel, true, false));
    let Command::List(listing) = &nav.take()[0] else {
        panic!()
    };
    assert!(listing.want_changed);
    assert!(land(
        &mut pane,
        &mut nav,
        &mut sel,
        ("/f/base", 41, 0, 2),
        shifted
    ));
    assert_eq!(pane.cursor, 8);
}

#[test]
fn a_failed_reread_releases_the_anchor_and_the_watch() {
    let (mut pane, mut nav, _) = settled("/f/base", 40);
    nav.changed(&pane, "/f/base", 0);
    assert!(nav.fire_watch(&mut pane, &[], 400));
    let now = pane.now();
    assert!(pane.fail("scan", "/f/base", "No such file or directory", 0, now));
    nav.after_failure(&mut pane, "scan");
    assert!(nav.anchor.is_none() && !nav.busy(&pane));
}
