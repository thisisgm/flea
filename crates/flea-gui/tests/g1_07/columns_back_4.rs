//! tests/js/columnsback.js runGlue: one Back and one Enter over request, landing and flight end.
use super::check::Checks;
use super::columns_back::{answer, key, list_rows, paths, peek_shape, DIR_FACTS, SOURCE, STATE};
use super::fixture::{Pane, Producers, SETTINGS};
use flea::jsondoc::Json;
use flea::protocol::RowFacts;
use flea_gui::columns::carry::{Leaving, Plan};
use flea_gui::columns::geometry::neighbour_asks;
use flea_gui::columns::peeks::PeekRow;
use flea_gui::columns::Area;

/// The oracle's stub backend: a watching peek keeps only its keep set, then arms its own path.
#[derive(Default)]
struct Backend {
    sent: Vec<String>,
    watched: Vec<String>,
}

impl Backend {
    fn take(&mut self, area: &mut Area) {
        for line in area.outbox.drain(..) {
            let doc = flea::jsondoc::parse(&line).expect("peek line is JSON");
            let path = doc
                .get("path")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_owned();
            let keep: Vec<String> = doc
                .get("keep")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(|one| one.as_str().map(str::to_owned))
                .collect();
            self.watched.retain(|one| keep.contains(one));
            if keep.contains(&path) && !self.watched.contains(&path) {
                self.watched.push(path.clone());
            }
            self.sent.push(path);
        }
    }
}

fn file(name: &str) -> PeekRow {
    PeekRow {
        name: name.into(),
        dir: false,
        icon: "text-x-generic".into(),
    }
}

fn reply(path: &str, rows: Vec<PeekRow>) -> flea_gui::columns::peeks::Peeked {
    answer(path, rows, DIR_FACTS, None)
}

#[test]
fn a_back_lands_parent_active_and_child_whole_in_its_landing_turn() {
    let mut c = Checks::new(SOURCE);
    c.covers(&[11]);
    let width = 1300.;
    let c_rows = list_rows("c", 35);
    let mut b_rows = list_rows("b", 35);
    // The folder left sits in its parent's listing, where the Back restores the cursor.
    b_rows[2] = RowFacts {
        name: "c".into(),
        ..b_rows[0].clone()
    };
    let mut area = Area::new(width, 800., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut backend = Backend::default();
    let mut pane = Pane::at("/a/b/c", c_rows.clone());
    pane.first = 35;
    // Before the Back: /a/b and the cursor's folder /a/b/c/c0 drawn, answered and watched.
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    backend.take(&mut area);
    let parent = reply("/a/b", peek_shape(&b_rows));
    pane.with(|view| area.peeked(&parent, &view, &mut producers.ports(), 1));
    let shown = reply("/a/b/c/c0", vec![file("x0")]);
    pane.with(|view| area.peeked(&shown, &view, &mut producers.ports(), 2));
    backend.take(&mut area);
    backend.sent.clear();

    // The request: the list goes out with the flight up and the listing path moved to /a/b.
    pane.in_flight = true;
    let back = Leaving {
        here: "/a/b/c",
        target: "/a/b",
        tag: "",
        window_size: 35,
        total: 300,
        held_start: 0,
        narrowed: false,
        rows: &c_rows,
        order: STATE,
        facts: DIR_FACTS,
    };
    let planned = Plan::new(&back, &neighbour_asks("/a/b", width, None));
    pane.with(|view| area.listing_requested(back, &view.lens, 3));
    let request_plans = area.carry == planned;
    backend.take(&mut area);
    c.eq(
        208,
        "the request asks the parent alone, before the list",
        backend.sent.clone(),
        paths(&["/a"]),
    );
    c.eq(
        209,
        "every drawn column stays watched, and the parent is armed",
        backend.watched.clone(),
        paths(&["/a/b", "/a/b/c/c0", "/a"]),
    );
    let grandparent = reply("/a", peek_shape(&list_rows("a", 35)));
    pane.with(|view| area.peeked(&grandparent, &view, &mut producers.ports(), 4));
    let parent_rows = area.peeks.rows(&key("/a")).map(|rows| rows.len());
    c.ok(
        211,
        "its reply lands before the list's",
        parent_rows == Some(35) && area.peeks.pending.is_empty(),
    );

    // The landing turn: the rows empty, then rows, path and the first cursor land.
    backend.sent.clear();
    let third = area.host.shown.clone();
    pane.rows = Vec::new();
    pane.with(|view| area.sync(&view, &mut producers.ports(), 5));
    let held_through_gap = area.host.shown == third;
    pane.go("/a/b", b_rows.clone());
    let asks = neighbour_asks("/a/b", width, None);
    let shown_child = area.host.shown.child.clone();
    let expected = pane.with(|view| {
        Plan::land(
            area.carry.as_ref(),
            &asks,
            &shown_child,
            &area.peeks,
            &view.lens,
        )
    });
    pane.with(|view| area.path_landed(&view, &mut producers.ports(), 6));
    let held_through_landing = area.host.shown == third;
    let landing_keeps = ["/a", "/a/b", "/a/b/c", "/a/b/c/c0"]
        .iter()
        .all(|path| expected.rows(&key(path)) == area.peeks.rows(&key(path)))
        && expected.pending == area.peeks.pending;
    backend.take(&mut area);
    c.eq(
        222,
        "the landing turn asks nothing while its cursor is still out",
        backend.sent.len(),
        0,
    );
    c.eq(
        223,
        "the landing draws the parent",
        area.peeks.rows(&key("/a")).map(|rows| rows.len()),
        Some(35),
    );
    let seeded: Vec<PeekRow> = area
        .peeks
        .rows(&key("/a/b/c"))
        .map(|rows| rows.to_vec())
        .unwrap_or_default();
    c.eq(
        225,
        "and the folder left as the child, in a peek's shape",
        seeded.clone(),
        peek_shape(&c_rows),
    );
    c.eq(226, "with no ask still out", area.peeks.pending.len(), 0);

    // The flight's end with the restored cursor on the folder left.
    pane.cursor = 2;
    pane.in_flight = false;
    pane.with(|view| area.flight_ended(&view, &mut producers.ports(), 7));
    backend.take(&mut area);
    c.eq(230, "the carry is spent", area.carry.is_none(), true);
    let landed_once = held_through_gap && held_through_landing && area.host.shown.child == "/a/b/c";
    c.ok(
        231,
        "the third column, held through the gap, takes the landed cursor once",
        landed_once,
    );
    c.eq(
        232,
        "the flight's end asks the child once, to re-arm its watch",
        backend.sent.clone(),
        paths(&["/a/b/c"]),
    );
    c.ok(
        233,
        "never the parent the request answered",
        !backend.sent.iter().any(|path| path == "/a"),
    );
    let mut watched = backend.watched.clone();
    watched.sort();
    c.eq(
        234,
        "the backend then watches exactly the drawn columns",
        watched,
        paths(&["/a", "/a/b/c"]),
    );
    let refreshed_once = backend.sent.len() == 1;
    let glue = request_plans && landing_keeps && refreshed_once;
    let label = "the request plans through Carry.plan, the landing through Carry.landing, \
                 the flight's end refreshes once";
    c.ok(165, label, glue);
    let before = area.peeks.version;
    let child = reply("/a/b/c", seeded.clone());
    pane.with(|view| area.peeked(&child, &view, &mut producers.ports(), 8));
    let landed: Option<Vec<PeekRow>> = area.peeks.rows(&key("/a/b/c")).map(|rows| rows.to_vec());
    c.eq(
        237,
        "its reply carries the seeded rows",
        landed,
        Some(seeded),
    );
    c.eq(238, "and lands as one peek", area.peeks.version, before + 1);

    // An Enter seeds the listing left into its incoming parent, armed at the flight's end.
    backend.sent.clear();
    pane.in_flight = true;
    let enter = Leaving {
        here: "/a/b",
        target: "/a/b/c",
        rows: &b_rows,
        ..back
    };
    pane.with(|view| area.listing_requested(enter, &view.lens, 9));
    backend.take(&mut area);
    c.ok(
        245,
        "an Enter carries its listing and asks nothing redundant at its request",
        area.carry.is_some() && backend.sent.is_empty(),
    );
    pane.go("/a/b/c", Vec::new());
    pane.with(|view| area.path_landed(&view, &mut producers.ports(), 10));
    backend.take(&mut area);
    c.ok(
        249,
        "its parent is seeded in the landing",
        area.peeks.answered(&key("/a/b"), "/a/b"),
    );
    c.eq(
        250,
        "its landing waits for the final cursor before asking",
        backend.sent.len(),
        0,
    );
    pane.in_flight = false;
    pane.with(|view| area.flight_ended(&view, &mut producers.ports(), 11));
    backend.take(&mut area);
    c.eq(
        253,
        "then refreshes and watches its new parent",
        backend.sent.clone(),
        paths(&["/a/b"]),
    );
    c.finish();
}
