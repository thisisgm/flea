//! tests/js/columns.js runFolderHold, runTabWiring and the neighbour background routes: what a column waits on, opens in a tab and routes.
use super::check::Checks;
use super::fixture::{area, reply, row, Pane, Producers, SETTINGS};
use flea_gui::columns::geometry::{folder_data_hold, Slot};
use flea_gui::columns::menu::{
    tapped_column, Background, Busy, Button, Navigator as MenuPane, Route, Verb,
};
use flea_gui::columns::peeks::Landed;
use flea_gui::columns::preview_host::{Cursor, Move, PreviewSettings};
use flea_gui::columns::{Area, TrackRows, TrackView};
use flea_gui::navigation::history::join;
use flea_gui::pane::Kind;

/// A 2400 px area at the five-column limit over /a/b/c/d, every ancestor and the cursor's folder answered.
fn five_columns() -> (Area, Pane, Producers) {
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
    (area, pane, producers)
}

/// The first row a track draws: a peek's own rows, or the pane's listing for the active track.
fn first_row(view: &TrackView, pane: &Pane) -> Option<(String, bool)> {
    match &view.rows {
        TrackRows::Peek(rows) => rows.first().map(|row| (row.name.clone(), row.dir)),
        TrackRows::Listing => pane.rows.first().map(|row| (row.name.clone(), row.dir)),
        _ => None,
    }
}

/// Tap.tabTarget over the model: a folder row middle-clicked once opens its track's directory joined with its name.
fn tab_target(view: &TrackView, name: &str, dir: bool) -> String {
    match tapped_column(Some(dir), Button::Middle, 1) {
        Verb::OpenTab => join(&view.path, name),
        _ => String::new(),
    }
}

/// A pane the routing reads only for its path and busy state, starting every listing it is asked for.
struct Hop {
    path: String,
    in_flight: bool,
    asked: Vec<String>,
}

impl Hop {
    fn at(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            in_flight: false,
            asked: Vec::new(),
        }
    }
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
    fn open(&mut self, path: &str) {
        self.asked.push(path.to_owned());
        self.in_flight = true;
    }
    fn say_busy(&mut self) {}
    fn open_background_menu(&mut self, _at: (f32, f32)) {}
}

/// One empty-space tap on a track routed over the directory the track draws: the route, the stored target, the listings asked.
fn route_track(view: &TrackView, here: &str) -> (Route, String, Vec<String>) {
    let mut pane = Hop::at(here);
    let mut background = Background::default();
    let route = background.route(
        Some(&mut pane as &mut dyn MenuPane),
        &view.path,
        Some((5., 6.)),
    );
    (route, background.target, pane.asked)
}

#[test]
fn an_unanswered_folder_waits_by_data_and_the_held_picture_stays() {
    let mut c = Checks::new("tests/js/columns.js");
    let mut area = area();
    let mut producers = Producers::default();
    let rows = vec![row("a.txt", false), row("docs", true), row("more", true)];
    let mut pane = Pane::at("/w", rows);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let gate = folder_data_hold(true, false)
        && !folder_data_hold(true, true)
        && !folder_data_hold(false, false);
    c.ok(329, "the wait follows the unanswered folder", gate);
    // A file hold is live when the cursor steps onto the unanswered folder.
    producers.swap.holding = true;
    pane.cursor = 1;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 10));
    let moved = pane.with(|view| area.move_third(&view, &mut producers.ports(), false, 20));
    c.eq(
        329,
        "the wait follows the unanswered folder",
        moved,
        Move::FolderWait,
    );
    let calls = &producers.swap.calls;
    let only_cap_stops = !calls.is_empty() && calls.iter().all(|call| call == "stopCap");
    c.ok(
        330,
        "the swap holds it",
        producers.swap.holding && only_cap_stops,
    );
    let before = area.host.shown.clone();
    let queued = Cursor {
        row: true,
        is_dir: true,
        child: "/w/docs",
    };
    let drew = area.host.show_cursor_row(queued, false, false);
    c.ok(
        330,
        "the swap holds it",
        !drew && area.host.shown == before && !before.is_dir,
    );
    let answer = reply("/w/docs", &[("x", false)]);
    let landed = pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), 30));
    let moved = pane.with(|view| area.move_third(&view, &mut producers.ports(), false, 40));
    c.ok(
        329,
        "the wait follows the unanswered folder",
        landed == Landed::Stored && moved != Move::FolderWait && area.host.shown.is_dir,
    );
    pane.cursor = 0;
    let moved = pane.with(|view| area.move_third(&view, &mut producers.ports(), false, 50));
    c.ok(
        329,
        "the wait follows the unanswered folder",
        moved != Move::FolderWait,
    );
    c.finish();
}

#[test]
fn each_column_opens_its_own_drawn_folder_in_a_tab() {
    let mut c = Checks::new("tests/js/columns.js");
    let (area, pane, _producers) = five_columns();
    let tracks = area.tracks();
    let wiring = [
        (
            357,
            358,
            Slot::GreatGrandparent,
            "great-grandparent",
            "/a/b",
        ),
        (359, 360, Slot::Grandparent, "grandparent", "/a/b/c"),
        (361, 362, Slot::Parent, "parent", "/a/b/c/d"),
        (363, 364, Slot::Active, "current", "/a/b/c/d/e"),
        (365, 366, Slot::Child, "child", "/a/b/c/d/e/g"),
    ];
    let mut answering = 0;
    for (own, opens, slot, name, want) in wiring {
        let views: Vec<&TrackView> = tracks.iter().filter(|view| view.slot == slot).collect();
        let label = format!("{name} tab stays in its own column");
        c.eq(own, &label, views.len(), 1);
        let target = views
            .first()
            .and_then(|view| first_row(view, &pane).map(|(row, dir)| tab_target(view, &row, dir)))
            .unwrap_or_default();
        let label = format!("{name} opens its drawn folder in a tab");
        c.eq(opens, &label, target.as_str(), want);
        answering += usize::from(!target.is_empty());
    }
    c.eq(367, "each of the five columns answers a tab", answering, 5);
    c.finish();
}

#[test]
fn a_neighbour_background_routes_the_directory_its_column_draws() {
    let mut c = Checks::new("tests/js/columns.js");
    let (area, _pane, _producers) = five_columns();
    let routes = [
        (
            659,
            Slot::Parent,
            "the parent routes its drawn directory",
            "/a/b/c",
        ),
        (
            661,
            Slot::Grandparent,
            "the grandparent routes its own",
            "/a/b",
        ),
        (
            663,
            Slot::GreatGrandparent,
            "the great-grandparent routes its own",
            "/a",
        ),
        (
            665,
            Slot::Child,
            "the child routes its drawn folder",
            "/a/b/c/d/e",
        ),
    ];
    for (line, slot, label, want) in routes {
        let routed = area
            .tracks()
            .iter()
            .find(|view| view.slot == slot)
            .map(|view| route_track(view, "/a/b/c/d"));
        let expected = (Route::Navigating, want.to_owned(), vec![want.to_owned()]);
        c.eq(line, label, routed, Some(expected));
    }
    // A manual preview shows a file the moment the cursor lands on it, with no hold to release.
    let manual = PreviewSettings {
        automatic: false,
        ..SETTINGS
    };
    let mut area = Area::new(1200., 800., None, 0., manual);
    let mut producers = Producers::default();
    let rows = vec![row("docs", true), row("more", true), row("a.txt", false)];
    let mut pane = Pane::at("/w", rows);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let answer = reply("/w/docs", &[("x", true)]);
    pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), 1));
    pane.cursor = 1;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 2));
    let child = |area: &Area| {
        area.tracks()
            .iter()
            .find(|view| view.slot == Slot::Child)
            .cloned()
    };
    let waiting = child(&area);
    let shown = waiting.as_ref().is_some_and(|view| view.path == "/w/docs");
    let routed = waiting.as_ref().map(|view| route_track(view, "/w").1);
    c.ok(
        667,
        "and the child waits on the shown folder, never the pending one",
        shown && routed.as_deref() == Some("/w/docs"),
    );
    pane.cursor = 2;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 3));
    let preview = child(&area);
    let no_directory = preview
        .as_ref()
        .is_some_and(|view| view.path.is_empty() && view.rows == TrackRows::Preview);
    let routed = preview.as_ref().map(|view| route_track(view, "/w").0);
    c.ok(
        669,
        "and a file preview offers no directory",
        no_directory && routed == Some(Route::Ignored),
    );
    c.finish();
}
