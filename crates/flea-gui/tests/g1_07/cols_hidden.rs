//! tests/columnshidden.qml: a hidden Columns view keeps every column it drew, and a hop behind it leaves no peek rows.
use super::check::Checks;
use super::cols_filter::Columns;
use super::fixture::{drain_asks, row, Pane};
use flea::protocol::RowFacts;
use flea_gui::columns::carry::{Leaving, ListOrder};
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::peeks::{peek_key, Facts};
use flea_gui::columns::{Area, TrackRows};

/// The probe's COLHID_DIR, under one parent so the 1400 px view draws a parent column.
const DIR: &str = "/fx/colhid";

/// Every listing the probe opens sorts by name with folders first, so a carry may seed from it.
const NAME_ORDER: ListOrder<'static> = ListOrder {
    by: "name",
    desc: false,
    folders_first: true,
    group_by_kind: false,
};

/// Each drawn track's slot, directory and rows.
type Drawn = Vec<(Slot, String, TrackRows)>;

fn dirs(names: &[&str]) -> Vec<(String, bool)> {
    names.iter().map(|name| (name.to_string(), true)).collect()
}

/// Files named the way tests/columnshidden.sh writes them with seq -w.
fn numbered(prefix: &str, count: usize, width: usize, ext: &str) -> Vec<(String, bool)> {
    (1..=count)
        .map(|n| (format!("{prefix}-{n:0width$}.{ext}"), false))
        .collect()
}

/// The fixture tree as the backend lists and peeks it, by name with folders first: 283 rows at the root.
fn tree(path: &str) -> Vec<(String, bool)> {
    if path == "/fx" {
        return dirs(&["colhid"]);
    }
    match path.strip_prefix(DIR) {
        Some("") => {
            let mut rows = dirs(&["empty", "hop", "sub"]);
            rows.extend(numbered("misc", 100, 3, "dat"));
            rows.extend(numbered("north", 60, 2, "txt"));
            rows.extend(numbered("notes", 120, 3, "md"));
            rows
        }
        Some("/sub") => numbered("notes-sub", 10, 2, "md"),
        Some("/hop") => dirs(&["empty", "sixty", "three"]),
        Some("/hop/sixty") => numbered("row", 60, 2, "txt"),
        Some("/hop/three") => numbered("row", 3, 1, "txt"),
        _ => Vec::new(),
    }
}

fn listing(path: &str) -> Vec<RowFacts> {
    tree(path)
        .iter()
        .map(|(name, dir)| row(name, *dir))
        .collect()
}

/// The real backend answers every peek out, until the answers ask for nothing more.
fn serve(cols: &mut Columns) {
    loop {
        let asked = drain_asks(&mut cols.area);
        if asked.is_empty() {
            return;
        }
        for path in asked {
            let rows = tree(&path);
            let rows: Vec<(&str, bool)> = rows
                .iter()
                .map(|(name, dir)| (name.as_str(), *dir))
                .collect();
            cols.answer(&path, &rows);
        }
    }
}

fn drawn(area: &Area) -> Drawn {
    let tracks = area.tracks().iter();
    tracks
        .map(|track| (track.slot, track.path.clone(), track.rows.clone()))
        .collect()
}

/// The rows the peek columns draw, and the rows the store holds for each peek column's directory.
fn peek_census(area: &Area) -> [usize; 2] {
    let mut census = [0, 0];
    for track in area
        .tracks()
        .iter()
        .filter(|track| track.slot != Slot::Active)
    {
        if let TrackRows::Peek(rows) = &track.rows {
            census[0] += rows.len();
        }
        if !track.path.is_empty() {
            let held = area.peeks.rows(&peek_key(&track.path, false, false));
            census[1] += held.map_or(0, |rows| rows.len());
        }
    }
    census
}

/// One op the probe awaits for 300 ms of calm: the swap's hold released and every ask answered.
fn settle(cols: &mut Columns) {
    cols.now += 300;
    cols.sync();
    cols.release();
    serve(cols);
}

fn step(cols: &mut Columns, at: usize) {
    cols.pane.pane.cursor = at;
    settle(cols);
}

/// Ctrl+2 shows the view; Ctrl+1 or Ctrl+3 hides it behind List or Grid, the view staying built.
fn set_shown(cols: &mut Columns, visible: bool) {
    cols.call(|area, view, ports, now| area.set_visible(visible, view, ports, now));
}

fn show(cols: &mut Columns) {
    set_shown(cols, true);
    settle(cols);
}

/// pane.open: the request goes out, the path lands with the listing in flight, then the flight ends.
fn open(cols: &mut Columns, path: &str) {
    let target = format!("{DIR}{path}");
    let here = cols.pane.pane.here.clone();
    let held = cols.pane.pane.rows.clone();
    let first = cols.pane.pane.first;
    cols.call(|area, view, _, now| {
        let leaving = Leaving {
            here: &here,
            target: &target,
            tag: "",
            window_size: first,
            total: held.len(),
            held_start: 0,
            narrowed: false,
            rows: &held,
            order: NAME_ORDER,
            facts: Facts {
                dev: 0,
                writable: true,
            },
        };
        area.listing_requested(leaving, &view.lens, now);
    });
    cols.pane.pane.in_flight = true;
    cols.pane.pane.go(&target, listing(&target));
    cols.call(|area, view, ports, now| area.path_landed(view, ports, now));
    cols.pane.pane.in_flight = false;
    cols.call(|area, view, ports, now| area.flight_ended(view, ports, now));
    settle(cols);
}

/// What a hide must leave as it was: every track's rows, and the count of tracks ever built.
fn census(area: &Area) -> (Drawn, usize) {
    (drawn(area), area.tracks_constructed())
}

/// Ctrl+1 or Ctrl+3: the census before the key, and once the hide settles.
fn hide(cols: &mut Columns) -> ((Drawn, usize), (Drawn, usize)) {
    let before = census(&cols.area);
    set_shown(cols, false);
    (before, census(&cols.area))
}

const HIDE_LIST: &str = "hide to list keeps every row and builds none";
const HIDE_GRID: &str = "hide to grid keeps every row and builds none";

/// Round 0's search behind List: each of n, o and r lands its matches, and Escape restores the listing on the row it left.
fn search(cols: &mut Columns) {
    let all = listing(DIR);
    let start = cols.pane.pane.cursor;
    for query in ["n", "no", "nor"] {
        let matches: Vec<RowFacts> = all
            .iter()
            .filter(|row| row.name.contains(query))
            .cloned()
            .collect();
        cols.pane.pane.rows = matches;
        cols.pane.pane.cursor = 0;
        cols.pane.pane.listing += 1;
        settle(cols);
    }
    cols.pane.pane.rows = all;
    cols.pane.pane.cursor = start;
    cols.pane.pane.listing += 1;
    settle(cols);
}

/// The three hops the moved rounds make behind the hidden view, back to the root with the cursor on row 230.
fn hop_back(cols: &mut Columns) {
    for path in ["/sub", "/empty", ""] {
        open(cols, path);
    }
    step(cols, 230);
}

#[test]
fn a_hidden_view_keeps_its_columns_and_a_hop_behind_it_holds_no_peek_rows() {
    let mut c = Checks::new("tests/columnshidden.qml");
    // The PASS and FAIL line, the op loop, its timeout and the exit status are this test's own steps and finish.
    c.covers(&[67, 228, 234, 238, 242, 255]);
    let mut cols = Columns::new(1400., 800., Pane::at(DIR, listing(DIR)));
    // open "" lists the fixture in List, so the Columns view starts hidden until Ctrl+2.
    cols.area.visible = false;
    cols.sync();
    show(&mut cols);

    // folder: the cursor on sub, whose ten rows the child column draws.
    step(&mut cols, 2);
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_LIST, at_hide.clone(), before);
    let label = "folder hidden columns keep their rows";
    c.eq(345, label, drawn(&cols.area), at_hide.0);
    show(&mut cols);

    // top: a file row at the top of the listing.
    step(&mut cols, 3);
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_LIST, at_hide.clone(), before);
    let label = "top hidden columns keep their rows";
    c.eq(345, label, drawn(&cols.area), at_hide.0);
    show(&mut cols);

    // kept: row 200, scrolled mid-column by the list pane, which owns the active track's scroll.
    step(&mut cols, 200);
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_LIST, at_hide.clone(), before);
    let label = "kept hidden columns keep their rows";
    c.eq(345, label, drawn(&cols.area), at_hide.0);
    show(&mut cols);

    // cursor: the cursor steps to row 260 while the view is hidden.
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_LIST, at_hide.clone(), before);
    step(&mut cols, 260);
    let label = "cursor hidden columns keep their rows";
    c.eq(345, label, drawn(&cols.area), at_hide.0);
    show(&mut cols);

    // moved: three hops behind List and the search, so every column's directory changed while hidden.
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_LIST, at_hide, before);
    hop_back(&mut cols);
    search(&mut cols);
    // The active track draws the list pane's own held window, so only the peek columns can hold rows of their own.
    let label = "moved hidden columns hold no rows";
    c.eq(345, label, peek_census(&cols.area), [0, 0]);
    show(&mut cols);

    // moved-grid: the same hops behind Grid.
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_GRID, at_hide, before);
    hop_back(&mut cols);
    let label = "moved-grid hidden columns hold no rows";
    c.eq(345, label, peek_census(&cols.area), [0, 0]);
    show(&mut cols);

    // moved-count: a watched re-read adds zz-added.txt after the cursor, then removes it, the cursor index unchanged.
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_LIST, at_hide, before);
    cols.pane.pane.rows.push(row("zz-added.txt", false));
    cols.pane.pane.listing += 1;
    settle(&mut cols);
    cols.pane.pane.rows.pop();
    cols.pane.pane.listing += 1;
    settle(&mut cols);
    show(&mut cols);

    // kept-pool: the closing hop opens three folders while shown, then hides behind Grid.
    for path in ["/hop/sixty", "/hop/empty", "/hop/three"] {
        open(&mut cols, path);
    }
    let (before, at_hide) = hide(&mut cols);
    c.eq(324, HIDE_GRID, at_hide.clone(), before);
    let label = "kept-pool hidden columns keep their rows";
    c.eq(345, label, drawn(&cols.area), at_hide.0);
    show(&mut cols);
    c.finish();
}
