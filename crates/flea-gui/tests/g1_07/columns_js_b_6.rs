//! tests/js/columns.js onFailed and landing tail against the Navigator: which failure drops the armed cursor file, the undo-shaped stranded re-read, and the landed background.
use super::check::Checks;
use super::columns_js_b_4::{asking_b, fail, lists, navigator, settled, waiting};
use super::fixture::row;
use flea::protocol::Reply;
use flea_gui::navigation::{Command, Navigator};
use flea_gui::pane::{swap::Options, Pane};

/// One listed and rows pair for the listing out, then the navigator's landing, as the shell runs them.
fn land(pane: &mut Pane, nav: &mut Navigator, path: &str, listing: usize) {
    let before = pane.path().to_owned();
    let now = pane.now();
    let listed = Reply::Listed {
        path: path.into(),
        total: 1,
    };
    pane.receive_at(listed, now).expect("a listed line");
    let answer = Reply::Rows {
        start: 0,
        rows: vec![row("a", true)],
        listing,
    };
    assert!(
        pane.receive_at(answer, now).expect("a rows line"),
        "the listing lands"
    );
    nav.after_reply(pane, &before, &mut Vec::new());
}

/// The selections every list request carries, "|" joined, empty for none.
fn selects(commands: &[Command]) -> String {
    commands
        .iter()
        .filter_map(|command| match command {
            Command::List(listing) => Some(listing.select.clone().unwrap_or_default()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn only_a_failed_esc_relist_drops_the_cursor_file_it_armed() {
    let mut c = Checks::new("tests/js/columns.js");
    let (mut scan, mut nav) = asking_b();
    nav.pending_select = Some("/b/kept.txt".into());
    let ended = fail(&mut scan, &mut nav, "scan", "/b", "permission denied");
    c.eq(
        594,
        "a listing failing after a search ended keeps the cursor file it armed",
        (ended, nav.pending_select.as_deref()),
        (true, Some("/b/kept.txt")),
    );
    let mut relist = settled("/a");
    let mut nav = navigator();
    let options = Options {
        hold_walk: true,
        relist: true,
        ..Options::default()
    };
    let listed = nav.open_without_history(&mut relist, "/b", options, None, false);
    assert_eq!(listed, Ok(true), "an idle pane lists");
    nav.pending_select = Some("/b/kept.txt".into());
    let ended = fail(&mut relist, &mut nav, "scan", "/b", "permission denied");
    c.eq(
        600,
        "while the Esc relist failing leaves none armed",
        (ended, nav.pending_select.as_deref()),
        (true, None),
    );
    c.finish();
}

#[test]
fn an_undo_shaped_stranded_failure_re_reads_selecting_nothing() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut pane = settled("/a");
    let mut nav = navigator();
    nav.pending_select = Some("/a/old.txt".into());
    fail(
        &mut pane,
        &mut nav,
        "rename-stranded",
        "/a/old.txt",
        "input/output failed",
    );
    let out = nav.take();
    c.eq(
        625,
        "an undo-shaped stranded failure re-reads the listing selecting nothing",
        (lists(&out), selects(&out)),
        ("/a".to_owned(), String::new()),
    );
    let (mut kept, mut nav) = asking_b();
    fail(
        &mut kept,
        &mut nav,
        "rename-kept",
        "/a/new.txt",
        "input/output failed",
    );
    let out = nav.take();
    assert_eq!(
        (lists(&out), selects(&out)),
        (kept.path().to_owned(), String::new()),
        "a kept twin ends the listing out and re-reads the drawn one once"
    );
    c.finish();
}

#[test]
fn a_landed_listing_consumes_the_waiting_background_by_identity() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut pane = settled("/a");
    let mut nav = navigator();
    nav.menus.background = waiting("/b", (4., 5.));
    let listed = nav.open_without_history(&mut pane, "/b", Options::default(), None, false);
    assert_eq!(listed, Ok(true), "an idle pane lists");
    land(&mut pane, &mut nav, "/b", 2);
    c.eq(
        682,
        "a landed listing consumes it by identity",
        (
            nav.background_at.take(),
            nav.menus.background.target.as_str(),
        ),
        (Some((4., 5.)), ""),
    );
    // A hop elsewhere drops it before any rows; a failed listing drops it with the listing.
    let mut hop = settled("/a");
    let mut nav = navigator();
    nav.menus.background = waiting("/b", (4., 5.));
    let _ = nav.open_without_history(&mut hop, "/c", Options::default(), None, false);
    land(&mut hop, &mut nav, "/c", 2);
    assert_eq!(
        (nav.background_at, nav.menus.background.target.as_str()),
        (None, "")
    );
    let (mut failed, mut nav) = asking_b();
    nav.menus.background = waiting("/b", (4., 5.));
    assert!(fail(
        &mut failed,
        &mut nav,
        "scan",
        "/b",
        "no such file or directory"
    ));
    assert_eq!(
        nav.menus.background.target.as_str(),
        "",
        "the ended listing drops the intent"
    );
    c.finish();
}
