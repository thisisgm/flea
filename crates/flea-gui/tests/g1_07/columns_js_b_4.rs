//! tests/js/columns.js runColumnMenu tail: the deferred background's consume, which failures end its listing, and a hop that still asks for its own directory.
use super::check::Checks;
use super::fixture::row;
use flea::protocol::Reply;
use flea_gui::columns::menu::{should_open, Background, Busy, Navigator as MenuPane};
use flea_gui::navigation::folder_state::{Folders, Writer};
use flea_gui::navigation::{Command, Navigator};
use flea_gui::pane::{swap::Options, Kind, Pane};
use std::{cell::RefCell, rc::Rc};

/// The pane applyPendingBackground reads: its path and listing state, recording each menu it opened.
struct Bg {
    path: String,
    state: Kind,
    opened: Vec<(f32, f32)>,
}

impl Bg {
    fn at(path: &str, state: Kind) -> Self {
        Self {
            path: path.to_owned(),
            state,
            opened: Vec::new(),
        }
    }
}

impl MenuPane for Bg {
    fn path(&self) -> &str {
        &self.path
    }
    fn busy(&self) -> Busy {
        Busy::default()
    }
    fn in_flight(&self) -> bool {
        false
    }
    fn listing_state(&self) -> Kind {
        self.state
    }
    fn open(&mut self, _path: &str) {}
    fn say_busy(&mut self) {}
    fn open_background_menu(&mut self, at: (f32, f32)) {
        self.opened.push(at);
    }
}

pub(super) fn waiting(target: &str, at: (f32, f32)) -> Background {
    Background {
        target: target.to_owned(),
        at: Some(at),
    }
}

/// A pane whose first listing landed, so nothing is in flight.
pub(super) fn settled(path: &str) -> Pane {
    let mut pane = Pane::new(path.into(), 620.);
    let rows = vec![row("a", true), row("b.txt", false)];
    let total = rows.len();
    let now = pane.now();
    let listed = Reply::Listed {
        path: path.into(),
        total,
    };
    pane.receive_at(listed, now).expect("a listed line");
    let answer = Reply::Rows {
        start: 0,
        rows,
        listing: 1,
    };
    let landed = pane.receive_at(answer, now).expect("a rows line");
    assert!(landed, "the first listing lands");
    pane
}

pub(super) fn navigator() -> Navigator {
    let settings = flea::jsondoc::parse("{}").expect("settings JSON");
    Navigator::new(
        Rc::new(RefCell::new(Folders::from_settings(&settings, Vec::new()))),
        Rc::new(RefCell::new(Writer::default())),
    )
}

/// The directories every list request names, "|" joined.
pub(super) fn lists(commands: &[Command]) -> String {
    commands
        .iter()
        .filter_map(|command| match command {
            Command::List(listing) => Some(listing.path.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// The failure doubles' state: the pane drawn at /a with its listing for /b out.
pub(super) fn asking_b() -> (Pane, Navigator) {
    let mut pane = settled("/a");
    let mut nav = navigator();
    let listed = nav.open_without_history(&mut pane, "/b", Options::default(), None, false);
    assert_eq!(listed, Ok(true), "an idle pane lists");
    nav.take();
    (pane, nav)
}

/// The shell's failure path: the pane decides whether its listing ended, then the navigator drops what that listing owed.
pub(super) fn fail(
    pane: &mut Pane,
    nav: &mut Navigator,
    operation: &str,
    path: &str,
    message: &str,
) -> bool {
    let now = pane.now();
    let ended = pane.fail(operation, path, message, 0, now);
    nav.after_failure(pane, operation);
    ended
}

#[test]
fn landed_rows_consume_the_deferred_background_once() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        484,
        "a ready listing opens its pending menu",
        should_open("/a", "/a", Kind::Ready),
        true,
    );
    c.eq(
        485,
        "an empty listing opens too",
        should_open("/a", "/a/", Kind::Empty),
        true,
    );
    c.eq(
        486,
        "a locked listing drops it",
        should_open("/a", "/a", Kind::Locked),
        false,
    );
    c.eq(
        487,
        "an error drops it",
        should_open("/a", "/a", Kind::Error),
        false,
    );
    c.eq(
        488,
        "a loading listing opens nothing",
        should_open("/a", "/a", Kind::Loading),
        false,
    );
    c.eq(
        489,
        "a later unrelated listing opens nothing",
        should_open("/b", "/a", Kind::Ready),
        false,
    );
    c.eq(
        490,
        "nothing pending opens nothing",
        should_open("/a", "", Kind::Ready),
        false,
    );
    let mut pane = Bg::at("/a", Kind::Ready);
    let mut bg = waiting("/a", (1., 2.));
    bg.apply(&mut pane);
    c.eq(
        495,
        "the drawn directory opens its menu once the rows land",
        pane.opened.len(),
        1,
    );
    c.eq(496, "and the intent is consumed", bg.target.as_str(), "");
    let mut stale = Bg::at("/b", Kind::Ready);
    let mut later = waiting("/a", (1., 2.));
    later.apply(&mut stale);
    c.eq(
        501,
        "a later unrelated listing consumes without opening",
        stale.opened.len(),
        0,
    );
    c.eq(502, "and leaves nothing pending", later.target.as_str(), "");
    let mut locked = Bg::at("/a", Kind::Locked);
    let mut unreadable = waiting("/a", (1., 2.));
    unreadable.apply(&mut locked);
    c.eq(
        507,
        "an unreadable listing drops without opening",
        locked.opened.len(),
        0,
    );
    c.eq(
        508,
        "and leaves nothing pending",
        unreadable.target.as_str(),
        "",
    );
    // Swap.failListing is the pane's fail: true only when the listing it names ended.
    let mut flight = Pane::new("/a".into(), 620.);
    let now = flight.now();
    let ended = flight.fail("stale", "", "rows out of date", 0, now);
    c.eq(510, "a stale error ends only its own request", ended, false);
    c.eq(
        511,
        "leaving the armed listing in flight",
        flight.busy(),
        true,
    );
    let mut armed_pane = Bg::at("/b", Kind::Ready);
    let mut armed = waiting("/b", (9., 9.));
    armed.apply(&mut armed_pane);
    c.eq(
        516,
        "the armed menu still opens on its own rows",
        armed_pane.opened.len(),
        1,
    );
    armed.apply(&mut armed_pane);
    c.eq(518, "and only once", armed_pane.opened.len(), 1);
    let mut failed_flight = Pane::new("/a".into(), 620.);
    let now = failed_flight.now();
    let ended = failed_flight.fail("scan", "", "no such file or directory", 0, now);
    c.ok(
        520,
        "an actual list failure ends the listing",
        ended && !failed_flight.busy(),
    );
    let mut doomed_pane = Bg::at("/b", Kind::Ready);
    let mut doomed = waiting("/b", (1., 1.));
    doomed.clear();
    doomed.apply(&mut doomed_pane);
    c.eq(
        526,
        "a failed listing drops the intent before any rows",
        doomed_pane.opened.len(),
        0,
    );
    let mut failed = waiting("/b", (1., 2.));
    failed.clear();
    c.eq(
        529,
        "a failed listing clears the intent",
        failed.target.as_str(),
        "",
    );
    c.eq(530, "with nothing left to open on", failed.at, None);
    bg.apply(&mut pane);
    c.eq(
        532,
        "a consumed intent never opens twice",
        pane.opened.len(),
        1,
    );
    // The consume reads the shared shouldOpen over the pane's own path and listing state.
    let mut empty = Bg::at("/a", Kind::Empty);
    let mut loading = Bg::at("/a", Kind::Loading);
    let mut errored = Bg::at("/a", Kind::Error);
    waiting("/a/", (1., 1.)).apply(&mut empty);
    waiting("/a", (1., 1.)).apply(&mut loading);
    waiting("/a", (1., 1.)).apply(&mut errored);
    c.ok(
        685,
        "the consume opens only its own successful listing",
        empty.opened.len() == 1 && loading.opened.is_empty() && errored.opened.is_empty(),
    );
    c.finish();
}

#[test]
fn only_a_failure_that_ends_the_listing_lets_it_go() {
    let mut c = Checks::new("tests/js/columns.js");
    // The failure path runs compiled here, so the source's extraction of onFailed and its two throws have no counterpart.
    c.covers(&[538, 554]);
    let (mut stale, mut nav) = asking_b();
    let ended = fail(
        &mut stale,
        &mut nav,
        "stale",
        "trash [0]",
        "rows out of date",
    );
    c.ok(
        576,
        "and leaves the armed listing in flight",
        !ended && stale.busy() && stale.place() == "/b",
    );
    let (mut scan, mut nav) = asking_b();
    let ended = fail(&mut scan, &mut nav, "scan", "/b", "permission denied");
    c.ok(585, "and ends the listing", ended && !scan.busy());
    c.finish();
}

#[test]
fn a_hop_with_a_waiting_background_still_asks_for_its_own_directory() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut keeps = settled("/a");
    let mut nav = navigator();
    let listed = nav.open_without_history(&mut keeps, "/b", Options::default(), None, false);
    c.eq(
        643,
        "a hop to the armed target keeps its request",
        (listed, lists(&nav.take())),
        (Ok(true), "/b".to_owned()),
    );
    let mut drops = settled("/a");
    let mut nav = navigator();
    let listed = nav.open_without_history(&mut drops, "/c", Options::default(), None, false);
    c.eq(
        649,
        "a hop elsewhere still asks for its own directory",
        (listed, lists(&nav.take())),
        (Ok(true), "/c".to_owned()),
    );
    c.finish();
}
