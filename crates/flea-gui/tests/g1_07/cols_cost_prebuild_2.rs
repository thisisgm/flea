//! columnsprebuild.qml: a landing of any size builds no track and copies no row into the area.
use super::check::Checks;
use super::fixture::{row, Pane, Producers, SETTINGS};
use flea::protocol::RowFacts;
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::{Area, TrackRows};

const SOURCE: &str = "tests/columnsprebuild.qml";

fn sync(area: &mut Area, pane: &Pane, producers: &mut Producers, now: u64) {
    pane.with(|view| area.sync(&view, &mut producers.ports(), now));
}

/// fileRows and bigRows: count file rows named prefix plus a 4-digit index.
fn files(prefix: &str, count: usize) -> Vec<RowFacts> {
    (0..count)
        .map(|at| row(&format!("{prefix}{at:04}.txt"), false))
        .collect()
}

/// dirRows(name, 1): the 1-row folder a Back lands on.
fn folder(name: &str) -> Vec<RowFacts> {
    vec![row(&format!("{name}0000"), true)]
}

/// Tracks built or dropped since a mark, the port's count of what a landing built.
fn built_since(area: &Area, mark: (usize, usize)) -> usize {
    (area.tracks_constructed() - mark.0) + (area.tracks_dropped() - mark.1)
}

/// The active track reads the pane's own held window, so the area holds no row of the listing.
fn reads_listing(area: &Area) -> bool {
    area.tracks()
        .iter()
        .any(|view| view.slot == Slot::Active && view.rows == TrackRows::Listing)
}

#[test]
fn a_landing_builds_no_track_and_copies_no_row() {
    let mut c = Checks::new(SOURCE);
    // A 2536 by 1381 area at the shipped limit: three tracks of 845 px, the third 846.
    let mut area = Area::new(2536., 1381., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let mut pane = Pane::at("/probe", folder("big"));
    sync(&mut area, &pane, &mut producers, 0);
    let mark = (area.tracks_constructed(), area.tracks_dropped());
    let big = files("f_", 1000);

    pane.go("/probe/big", big.clone());
    sync(&mut area, &pane, &mut producers, 1);
    let built = built_since(&area, mark);
    c.eq(
        273,
        &format!("landing: built {built} delegates, want 0"),
        built,
        0,
    );
    c.ok(
        273,
        "landing: the active column copies the listing it landed",
        reads_listing(&area),
    );

    // A Back to the 1-row folder, then the 1000 rows again.
    pane.go("/probe", folder("big"));
    sync(&mut area, &pane, &mut producers, 2);
    let built = built_since(&area, mark);
    c.eq(324, &format!("back: built {built} delegates"), built, 0);
    pane.go("/probe/big", big.clone());
    sync(&mut area, &pane, &mut producers, 3);
    let built = built_since(&area, mark);
    c.eq(
        312,
        &format!("landing: built {built} delegates, want 0"),
        built,
        0,
    );
    c.ok(
        312,
        "landing: the active column copies the listing it landed",
        reads_listing(&area),
    );

    // (a) a Back, then (e) the 1000 rows once more.
    pane.go("/probe", folder("big"));
    sync(&mut area, &pane, &mut producers, 4);
    pane.go("/probe/big", big.clone());
    sync(&mut area, &pane, &mut producers, 5);
    let built = built_since(&area, mark);
    c.eq(
        237,
        &format!("e: Back after (a): the landing built {built} delegates, want 0"),
        built,
        0,
    );

    // Hidden and shown again, then a 20-row folder.
    pane.go("/probe", folder("big"));
    sync(&mut area, &pane, &mut producers, 6);
    pane.with(|view| area.set_visible(false, &view, &mut producers.ports(), 7));
    pane.with(|view| area.set_visible(true, &view, &mut producers.ports(), 8));
    pane.go("/probe/short", files("s_", 20));
    sync(&mut area, &pane, &mut producers, 9);
    let built = built_since(&area, mark);
    c.eq(
        237,
        &format!("c: a 20-row folder: the landing built {built} delegates, want 0"),
        built,
        0,
    );

    // A resize after a Back, then the 1000 rows.
    pane.go("/probe", folder("big"));
    sync(&mut area, &pane, &mut producers, 10);
    area.resize(2536., 1382.);
    area.resize(2536., 1381.);
    sync(&mut area, &pane, &mut producers, 11);
    pane.go("/probe/big", big);
    sync(&mut area, &pane, &mut producers, 12);
    let built = built_since(&area, mark);
    c.eq(
        237,
        &format!("drain: the landing built {built} delegates, want 0"),
        built,
        0,
    );
    c.ok(
        237,
        "drain: the active column copies the listing it landed",
        reads_listing(&area),
    );
    c.covers(&[
        245, 259, 264, 274, 279, 286, 287, 308, 313, 331, 332, 336, 354, 356, 360, 370, 371, 381,
        397, 405, 413,
    ]);
    c.finish();
}
