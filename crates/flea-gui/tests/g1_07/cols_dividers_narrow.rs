//! columndividers.qml and columnnarrow.qml: populated track edges and the empty-active exception; narrowing.
use super::check::Checks;
use super::fixture::{reply, row, Pane, Producers, SETTINGS};
use flea_gui::columns::geometry::{Slot, DIVIDER};
use flea_gui::columns::preview_host::Cursor;
use flea_gui::columns::{Area, TrackRows, TrackView};
use flea_gui::pane::visible_rows;

const DIVIDERS: &str = "tests/columndividers.qml";
const NARROW: &str = "tests/columnnarrow.qml";

fn sync(area: &mut Area, pane: &Pane, producers: &mut Producers, now: u64) {
    pane.with(|view| area.sync(&view, &mut producers.ports(), now));
}

/// One peeked reply for a path the area asked, as populateChild and populateAncestors write it.
fn land(area: &mut Area, pane: &Pane, producers: &mut Producers, path: &str, now: u64) {
    let rows: &[(&str, bool)] = if path.ends_with("/sub") {
        &[("kid.txt", false)]
    } else {
        &[("ancestor.txt", false)]
    };
    pane.with(|view| area.peeked(&reply(path, rows), &view, &mut producers.ports(), now));
}

/// The swap's owner releasing a file hold: the preview is ready, so the third shows the cursor row.
fn release_hold(area: &mut Area, pane: &Pane, producers: &mut Producers, now: u64) {
    producers.swap.holding = false;
    let cursor = Cursor {
        row: pane.cursor_row().is_some(),
        is_dir: false,
        child: "",
    };
    area.host.show_cursor_row(cursor, false, false);
    sync(area, pane, producers, now);
}

fn third_shown(area: &Area) -> bool {
    area.host.third_shown(area.settings)
}

/// The tracks a pane draws in: every ancestor, the active track, and the child only with a folder.
fn shown(area: &Area) -> Vec<&TrackView> {
    area.tracks()
        .iter()
        .filter(|view| view.slot != Slot::Child || matches!(view.rows, TrackRows::Peek(_)))
        .collect()
}

fn panes(area: &Area) -> usize {
    shown(area).len()
}

/// The x of every drawn divider, each on its shown track's right pixel.
fn edges(area: &Area) -> Vec<f32> {
    shown(area)
        .into_iter()
        .filter(|view| view.divider)
        .map(|view| view.x + view.width - DIVIDER)
        .collect()
}

/// checkActive: the populated active track keeps its edge at x 599 at 900 px, including with previews off.
fn check_active(c: &mut Checks, area: &Area, want: bool, label: &str) {
    let active = area
        .tracks()
        .iter()
        .find(|view| view.slot == Slot::Active && view.rows == TrackRows::Listing);
    c.ok(
        83,
        &format!("{label} finds no active pane"),
        active.is_some(),
    );
    let Some(active) = active else {
        return;
    };
    let divider = active.divider;
    c.eq(
        87,
        &format!("{label} active showDivider is {divider}, want {want}"),
        divider,
        want,
    );
    let edge = active.x + active.width - DIVIDER;
    let drawn = edges(area).contains(&edge);
    c.eq(
        92,
        &format!("{label} active divider visible is {drawn}, want {want}"),
        drawn,
        want,
    );
    c.eq(92, &format!("{label} active divider sits at"), edge, 599.);
}

/// checkWide: N shown tracks draw N - 1 edges, every ancestor one and the rightmost none.
fn check_wide(c: &mut Checks, area: &Area, label: &str, ancestors: &[f32]) {
    let panes = panes(area);
    let edges = edges(area);
    let want = panes.saturating_sub(1);
    c.eq(
        100,
        &format!(
            "{label} draws {} dividers over {panes} panes, want {want}",
            edges.len()
        ),
        edges.len() + 1,
        panes,
    );
    for view in shown(area) {
        if view.slot == Slot::Active {
            c.ok(
                109,
                &format!("{label} the rightmost pane draws a divider"),
                !view.divider,
            );
        } else {
            c.ok(
                111,
                &format!("{label} a shown non-rightmost pane hides its divider"),
                view.divider,
            );
        }
    }
    c.eq(
        111,
        &format!("{label} a shown non-rightmost pane hides its divider"),
        edges,
        ancestors.to_vec(),
    );
}

#[test]
fn the_active_column_keeps_its_edge_with_a_file_and_previews_off() {
    let mut c = Checks::new(DIVIDERS);
    let mut area = Area::new(900., 600., None, 0., SETTINGS);
    let mut producers = Producers::default();
    // An empty listing: the cursor names no row, so the third column stays blank.
    let mut pane = Pane::at("/a/b/c", Vec::new());
    sync(&mut area, &pane, &mut producers, 0);
    let count = area.count();
    c.eq(
        269,
        &format!("at 900 px the default limit draws {count} columns, want 3"),
        count,
        3,
    );
    c.eq(
        271,
        &format!(
            "at 900 px with an empty third shows {} panes, want 2",
            panes(&area)
        ),
        panes(&area),
        2,
    );
    c.eq(
        273,
        "an empty third shows thirdShown true, want false",
        third_shown(&area),
        false,
    );
    check_active(&mut c, &area, false, "empty third");

    // A child folder under the cursor draws in the third column once its peek lands.
    pane.rows = vec![row("sub", true)];
    pane.listing += 1;
    sync(&mut area, &pane, &mut producers, 1);
    land(&mut area, &pane, &mut producers, "/a/b/c/sub", 2);
    c.eq(
        291,
        "a child folder with rows leaves thirdShown false, want true",
        third_shown(&area),
        true,
    );
    c.eq(
        293,
        &format!(
            "a child folder with rows shows {} panes, want 3",
            panes(&area)
        ),
        panes(&area),
        3,
    );
    c.eq(
        295,
        &format!(
            "a child folder with rows draws {} dividers, want 2",
            edges(&area).len()
        ),
        edges(&area),
        vec![299., 599.],
    );
    check_active(&mut c, &area, true, "child folder with rows");

    // A file row: the swap holds for its preview, and the third shows it once the hold releases.
    pane.rows = vec![row("a.txt", false)];
    pane.listing += 1;
    sync(&mut area, &pane, &mut producers, 3);
    release_hold(&mut area, &pane, &mut producers, 4);
    c.eq(
        312,
        "a file preview with the preview column on leaves thirdShown false, want true",
        third_shown(&area),
        true,
    );
    check_active(
        &mut c,
        &area,
        true,
        "file preview with the preview column on",
    );

    area.settings.column = false;
    sync(&mut area, &pane, &mut producers, 5);
    c.eq(
        326,
        "with the preview column off a file keeps thirdShown true, want false",
        third_shown(&area),
        false,
    );
    check_active(
        &mut c,
        &area,
        true,
        "populated file with preview column off",
    );
    assert!(matches!(
        area.tracks().last().unwrap().rows,
        TrackRows::Blank
    ));

    // A new View state names limit 5 alone, so the preview column reads its default again.
    area.settings = SETTINGS;
    area.set_limit(Some(5.));
    area.resize(2400., 600.);
    pane.go("/a/b/c/d", Vec::new());
    sync(&mut area, &pane, &mut producers, 6);
    for path in ["/a/b/c", "/a/b", "/a"] {
        land(&mut area, &pane, &mut producers, path, 7);
    }
    let count = area.count();
    c.eq(
        346,
        &format!("at 2400 px with limit 5 draws {count} columns, want 5"),
        count,
        5,
    );
    c.eq(
        348,
        &format!(
            "at 5 columns with an empty third shows {} panes, want 4",
            panes(&area)
        ),
        panes(&area),
        4,
    );
    check_wide(
        &mut c,
        &area,
        "at 2400 px with limit 5",
        &[479., 959., 1439.],
    );

    area.resize(1800., 600.);
    sync(&mut area, &pane, &mut producers, 8);
    let count = area.count();
    c.eq(
        363,
        &format!("at 1800 px with limit 5 draws {count} columns, want 4"),
        count,
        4,
    );
    c.eq(
        365,
        &format!(
            "at 4 columns with an empty third shows {} panes, want 3",
            panes(&area)
        ),
        panes(&area),
        3,
    );
    check_wide(&mut c, &area, "at 1800 px with limit 5", &[449., 899.]);
    c.covers(&[
        225, 232, 239, 246, 253, 260, 263, 274, 275, 283, 284, 296, 297, 304, 305, 313, 314, 318,
        319, 327, 328, 338, 339, 349, 350, 355, 356, 366, 369, 371, 372, 378, 387,
    ]);
    c.finish();
}

#[test]
fn a_column_of_any_row_count_draws_rows() {
    let mut c = Checks::new(NARROW);
    // A 600 px column draws ceil(600 / 31) = 20 rows of 40, of the 20 narrowed, and of 40 widened.
    for (label, total) in [("full", 40), ("narrowed", 20), ("widened", 40)] {
        let drawn = visible_rows(600.).min(total);
        c.ok(110, &format!("{label} draws no row"), drawn > 0);
    }
    c.covers(&[93, 96, 120, 129, 152, 157, 164]);
    c.finish();
}
