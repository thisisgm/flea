//! columnscost.qml: 900 px builds three tracks and painters, 2400 px five, narrowing drops them.
use super::check::Checks;
use super::fixture::{reply, Pane, Producers, SETTINGS};
use flea_gui::columns::geometry::{Slot, DIVIDER};
use flea_gui::columns::paint::{Painters, CUT, HAIR};
use flea_gui::columns::{Area, TrackRows, TrackView, Typeface};
use flea_gui::ipc::columns::ColumnsAdapter;
use flea_gui::ipc::Adapter;

const SOURCE: &str = "tests/columnscost.qml";
const HEIGHT: f32 = 600.;

fn sync(area: &mut Area, pane: &Pane, producers: &mut Producers, now: u64) {
    pane.with(|view| area.sync(&view, &mut producers.ports(), now));
}

/// populateAncestors: each ancestor the area asked answers one row.
fn land(area: &mut Area, pane: &Pane, producers: &mut Producers, path: &str, now: u64) {
    let rows = [("ancestor.txt", false)];
    pane.with(|view| area.peeked(&reply(path, &rows), &view, &mut producers.ports(), now));
}

/// The paint states a shell keeps after a frame of the area: one per drawn track, the rest dropped.
fn draw(painters: &mut Painters, area: &Area, typeface: &Typeface) {
    painters.retain(area.tracks());
    for view in area.tracks().iter() {
        let _ = painters.painter(view.slot, typeface);
    }
}

/// The tracks a pane draws in: every ancestor, the active track, and the child only with a folder.
fn shown(area: &Area) -> Vec<&TrackView> {
    area.tracks()
        .iter()
        .filter(|view| view.slot != Slot::Child || matches!(view.rows, TrackRows::Peek(_)))
        .collect()
}

/// rowsFor: the rows a peeked track draws, None when the track is not drawn or draws no peek.
fn peek_rows(area: &Area, slot: Slot) -> Option<usize> {
    area.tracks()
        .iter()
        .find(|view| view.slot == slot)
        .and_then(|view| match &view.rows {
            TrackRows::Peek(rows) => Some(rows.len()),
            _ => None,
        })
}

/// grandparentItemAt(0) and its kin over IPC: row 0's centre, "" when the track has no such row.
fn row_at(area: &Area, pane: &Pane, function: &str) -> String {
    pane.with(|view| {
        let adapter = ColumnsAdapter {
            area,
            lens: view.lens,
            origin: (0., 0.),
            height: HEIGHT,
            widths: "{}",
            child_empty: None,
            preview: None,
        };
        adapter
            .call(function, &["0".to_owned()])
            .map(|reply| reply.stdout.trim_end().to_owned())
            .unwrap_or_default()
    })
}

/// checkDividers at 900 px with an empty third: one edge, on the parent, a hairline at 12 percent.
fn check_dividers(c: &mut Checks, area: &Area) {
    let shown = shown(area);
    let panes = shown.len();
    c.eq(
        103,
        &format!("at 900 px with an empty third shows {panes} panes, want 2"),
        panes,
        2,
    );
    let dividers = shown.iter().filter(|view| view.divider).count();
    let want = panes.saturating_sub(1);
    c.eq(
        106,
        &format!("at 900 px draws {dividers} dividers over {panes} panes, want {want}"),
        dividers + 1,
        panes,
    );
    for view in &shown {
        if view.slot == Slot::Active {
            c.ok(116, "the rightmost pane draws a divider", !view.divider);
        } else {
            c.ok(118, "a shown pane hides its divider", view.divider);
        }
    }
    c.eq(
        120,
        &format!("a column divider is {DIVIDER} wide, want the rail hairline"),
        DIVIDER,
        1.,
    );
    c.eq(
        124,
        &format!("a column divider opacifies at {HAIR}, want 0.12"),
        HAIR,
        0.12,
    );
}

#[test]
fn the_default_limit_builds_three_tracks_and_drops_what_it_stops_drawing() {
    let mut c = Checks::new(SOURCE);
    let typeface = Typeface::omarchy_default();
    let mut painters = Painters::default();
    let mut area = Area::new(900., HEIGHT, None, 0., SETTINGS);
    let mut producers = Producers::default();
    // An empty listing: no cursor row, so the third column stays idle.
    let mut pane = Pane::at("/probe/studio", Vec::new());
    sync(&mut area, &pane, &mut producers, 0);
    draw(&mut painters, &area, &typeface);
    let count = area.count();
    c.eq(
        327,
        &format!("at 900 px the default limit draws {count} columns, want 3"),
        count,
        3,
    );
    let built = area.tracks_built();
    c.eq(
        330,
        &format!("at 3 columns holds {built} ColumnPanes, want 3"),
        built,
        3,
    );
    c.eq(
        330,
        "at 3 columns holds paint states, want 3",
        painters.built(),
        3,
    );
    let deep = area
        .tracks()
        .iter()
        .any(|view| matches!(view.slot, Slot::Grandparent | Slot::GreatGrandparent));
    c.ok(
        330,
        "at 3 columns builds a grandparent or great-grandparent track",
        !deep,
    );
    check_dividers(&mut c, &area);
    c.ok(
        309,
        &format!("disabledOpacity reads {CUT}, want a real dim"),
        CUT > 0.05 && CUT < 0.95,
    );

    // Five columns under a deeper path, every ancestor peeked one row.
    pane.go("/a/b/c/d", Vec::new());
    area.set_limit(Some(5.));
    area.resize(2400., HEIGHT);
    sync(&mut area, &pane, &mut producers, 1);
    for path in ["/a/b/c", "/a/b", "/a"] {
        land(&mut area, &pane, &mut producers, path, 2);
    }
    draw(&mut painters, &area, &typeface);
    let count = area.count();
    c.eq(
        352,
        &format!("at 2400 px with limit 5 draws {count} columns, want 5"),
        count,
        5,
    );
    let built = area.tracks_built();
    c.eq(
        355,
        &format!("at 5 columns holds {built} ColumnPanes, want 5"),
        built,
        5,
    );
    c.eq(
        355,
        "at 5 columns holds paint states, want 5",
        painters.built(),
        5,
    );
    c.eq(
        357,
        "the grandparent pane is not bound to its rows",
        peek_rows(&area, Slot::Grandparent),
        Some(1),
    );
    c.eq(
        359,
        "the great-grandparent pane is not bound to its rows",
        peek_rows(&area, Slot::GreatGrandparent),
        Some(1),
    );
    let grandparent = row_at(&area, &pane, "columnGrandparentRowCentre");
    c.ok(
        361,
        "grandparentItemAt(0) answers null with 5 columns",
        !grandparent.is_empty(),
    );
    let great = row_at(&area, &pane, "columnGreatGrandparentRowCentre");
    c.ok(
        363,
        "greatGrandparentItemAt(0) answers null with 5 columns",
        !great.is_empty(),
    );

    // 1800 px draws four: the great-grandparent track and its paint state are dropped, not hidden.
    let before = (area.tracks_dropped(), painters.dropped);
    area.resize(1800., HEIGHT);
    sync(&mut area, &pane, &mut producers, 3);
    draw(&mut painters, &area, &typeface);
    let count = area.count();
    c.eq(
        372,
        &format!("at 1800 px with limit 5 draws {count} columns, want 4"),
        count,
        4,
    );
    let built = area.tracks_built();
    c.eq(
        375,
        &format!("at 4 columns holds {built} ColumnPanes, want 4"),
        built,
        4,
    );
    c.eq(
        375,
        "at 4 columns drops one track and one paint state",
        (
            area.tracks_dropped() - before.0,
            painters.dropped - before.1,
        ),
        (1, 1),
    );
    let grandparent = row_at(&area, &pane, "columnGrandparentRowCentre");
    c.ok(
        377,
        "grandparentItemAt(0) answers null with 4 columns",
        !grandparent.is_empty(),
    );
    let great = row_at(&area, &pane, "columnGreatGrandparentRowCentre");
    c.ok(
        379,
        "greatGrandparentItemAt(0) answers non-null with 4 columns",
        great.is_empty(),
    );

    // Back to the shipped limit at 900 px: both deeper ancestors go.
    area.set_limit(None);
    area.resize(900., HEIGHT);
    sync(&mut area, &pane, &mut producers, 4);
    draw(&mut painters, &area, &typeface);
    let count = area.count();
    c.eq(
        389,
        &format!("narrowed back draws {count} columns, want 3"),
        count,
        3,
    );
    let built = area.tracks_built();
    c.eq(
        392,
        &format!("narrowed back holds {built} ColumnPanes, want 3"),
        built,
        3,
    );
    c.eq(
        392,
        "narrowed back holds paint states, want 3",
        painters.built(),
        3,
    );
    let grandparent = row_at(&area, &pane, "columnGrandparentRowCentre");
    c.ok(
        394,
        "grandparentItemAt(0) answers non-null after narrowing to 3",
        grandparent.is_empty(),
    );
    let great = row_at(&area, &pane, "columnGreatGrandparentRowCentre");
    c.ok(
        396,
        "greatGrandparentItemAt(0) answers non-null after narrowing to 3",
        great.is_empty(),
    );
    c.covers(&[
        246, 253, 260, 267, 270, 331, 332, 333, 334, 335, 337, 339, 340, 341, 364, 380, 397, 398,
        399, 402, 407, 416,
    ]);
    c.finish();
}
