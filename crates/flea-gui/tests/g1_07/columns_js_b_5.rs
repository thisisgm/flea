//! tests/js/columns.js: the background base a drawn column routes, and the deferred menus a listing end or a hop drops.
use super::check::Checks;
use super::fixture::{reply, row, Pane, Producers, SETTINGS};
use flea::protocol::Reply;
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::menu::{Background, Busy, Deferred, Navigator as MenuPane, Route};
use flea_gui::columns::Area;
use flea_gui::pane::{swap::Options, Kind, Pane as ListPane};

/// A 2400 px area at the five-column limit over /a/b/c/d, every ancestor and the cursor's folder answered.
fn five_columns() -> Area {
    let mut area = Area::new(2400., 800., Some(5.), 0., SETTINGS);
    let mut producers = Producers::default();
    let pane = Pane::at("/a/b/c/d", vec![row("e", true), row("f.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let answers = [
        ("/a", "b"),
        ("/a/b", "c"),
        ("/a/b/c", "d"),
        ("/a/b/c/d/e", "g"),
    ];
    for (at, (path, name)) in answers.into_iter().enumerate() {
        let answer = reply(path, &[(name, true)]);
        let now = at as u64 + 1;
        pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), now));
    }
    area
}

/// A pane the routing reads only for its path and busy state, starting every listing it is asked for.
struct Hop {
    path: String,
    in_flight: bool,
}

impl MenuPane for Hop {
    fn path(&self) -> &str {
        &self.path
    }
    fn busy(&self) -> Busy {
        Busy {
            in_flight: self.in_flight,
            ..Busy::default()
        }
    }
    fn in_flight(&self) -> bool {
        self.in_flight
    }
    fn listing_state(&self) -> Kind {
        Kind::Ready
    }
    fn open(&mut self, _path: &str) {
        self.in_flight = true;
    }
    fn say_busy(&mut self) {}
    fn open_background_menu(&mut self, _at: (f32, f32)) {}
}

/// A pane whose first listing landed at /a, then its listing for /b sent.
fn asking_b() -> ListPane {
    let mut pane = ListPane::new("/a".into(), 620.);
    let rows = vec![row("a", true), row("b.txt", false)];
    let total = rows.len();
    let now = pane.now();
    let listed = Reply::Listed {
        path: "/a".into(),
        total,
    };
    pane.receive_at(listed, now).expect("a listed line");
    let answer = Reply::Rows {
        start: 0,
        rows,
        listing: 1,
    };
    assert!(
        pane.receive_at(answer, now).expect("a rows line"),
        "the first listing lands"
    );
    assert!(
        pane.begin("/b".into(), Options::default(), now),
        "the hop to /b is out"
    );
    pane
}

/// The deferred background menu armed for /b; a neighbour row's menu opens in place and defers nothing.
fn armed() -> Deferred {
    Deferred {
        background: Background {
            target: "/b".into(),
            at: Some((1., 1.)),
        },
    }
}

/// One failure through the pane, then the deferred menus told whether that listing ended.
fn failed(operation: &str, path: &str, message: &str) -> Deferred {
    let mut pane = asking_b();
    let now = pane.now();
    let ended = pane.fail(operation, path, message, 0, now);
    let mut deferred = armed();
    deferred.failed(operation, ended);
    deferred
}

#[test]
fn a_neighbour_background_routes_only_a_drawn_column() {
    let mut c = Checks::new("tests/js/columns.js");
    let area = five_columns();
    let base = area.background_base(Slot::Parent);
    c.eq(
        656,
        "the area navigates a neighbour background",
        base.clone(),
        Some("/a/b/c".to_owned()),
    );
    let mut pane = Hop {
        path: "/a/b/c/d".into(),
        in_flight: false,
    };
    let mut background = Background::default();
    let target = base.unwrap_or_default();
    let routed = background.route(
        Some(&mut pane as &mut dyn MenuPane),
        &target,
        Some((5., 6.)),
    );
    c.eq(
        658,
        "the helper delegates to the executed routing",
        routed,
        Route::Navigating,
    );
    // At /a the root is answered and drawn by the parent alone; at / no slot names a directory.
    let mut shallow = Area::new(2400., 800., Some(5.), 0., SETTINGS);
    let mut producers = Producers::default();
    let mut here = Pane::at("/a", vec![row("x.txt", false)]);
    here.with(|view| shallow.sync(&view, &mut producers.ports(), 0));
    let answer = reply("/", &[("a", true)]);
    here.with(|view| shallow.peeked(&answer, &view, &mut producers.ports(), 1));
    c.eq(
        662,
        "the grandparent stays gated on its shown ancestor",
        shallow.background_base(Slot::Grandparent),
        None,
    );
    c.eq(
        664,
        "the great-grandparent stays gated on its shown ancestor",
        shallow.background_base(Slot::GreatGrandparent),
        None,
    );
    let shown = shallow.background_base(Slot::Parent);
    here.go("/", vec![row("x.txt", false)]);
    here.with(|view| shallow.sync(&view, &mut producers.ports(), 2));
    c.eq(
        660,
        "the parent stays gated on its shown ancestor",
        (shown, shallow.background_base(Slot::Parent)),
        (Some("/".to_owned()), None),
    );
    c.finish();
}

#[test]
fn a_listing_end_drops_the_deferred_menu() {
    let mut c = Checks::new("tests/js/columns.js");
    let stale = failed("stale", "trash [0]", "rows out of date");
    c.eq(
        574,
        "a stale failure keeps the background intent",
        stale.background.target.as_str(),
        "/b",
    );
    let sort = failed("sort", "mode", "no such order");
    c.eq(
        579,
        "a refused sort keeps the intent too",
        sort.background.target.as_str(),
        "/b",
    );
    let scan = failed("scan", "/b", "permission denied");
    c.eq(
        583,
        "an actual list failure drops the background intent",
        scan.background.target.as_str(),
        "",
    );
    let backend = failed("backend", "", "child is gone");
    c.eq(
        588,
        "a dead backend drops the intent at once",
        backend.background.target.as_str(),
        "",
    );
    c.eq(
        671,
        "a failed listing drops the intent",
        scan.background.clone(),
        Background::default(),
    );
    let window = failed("window", "/b", "rows out of date");
    let elsewhere = failed("scan", "/c", "permission denied");
    c.ok(
        673,
        "unrelated errors keep the intent until the listing ends",
        window == armed() && elsewhere == armed(),
    );
    // A dead backend or a dead read ends every listing, so it drops the menu before the listing's own answer.
    let mut terminal = armed();
    terminal.failed("backend", false);
    let mut unread = armed();
    unread.failed("read", false);
    c.ok(
        676,
        "a dead backend still drops the deferred menu",
        terminal.background == Background::default() && unread.background == Background::default(),
    );
    c.ok(
        679,
        "the ended listing drops the deferred menu",
        scan.background == Background::default(),
    );
    c.finish();
}

#[test]
fn a_hop_elsewhere_drops_the_waiting_background() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut keeps = armed();
    keeps.hop("/b");
    c.eq(
        644,
        "and keeps the intent for its landing",
        keeps.background.target.as_str(),
        "/b",
    );
    let mut drops = armed();
    drops.hop("/c");
    c.eq(
        650,
        "while the waiting intent is dropped",
        drops.background.target.as_str(),
        "",
    );
    let mut spelled = armed();
    spelled.hop("/b/");
    c.ok(
        683,
        "a hop elsewhere drops the waiting intent",
        spelled.background == armed().background && drops.background == Background::default(),
    );
    c.finish();
}
