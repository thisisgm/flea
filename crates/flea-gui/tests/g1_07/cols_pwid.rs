//! columns-pwid.qml: the preview edge drags, clamps, stores once, restores, and relaunches at 42.
use super::check::Checks;
use super::fixture::{reply, row, Pane, Producers, SETTINGS};
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::preview_host::{Cursor, Edge, START_DRAG_DISTANCE};
use flea_gui::columns::{Area, TrackRows};

const SOURCE: &str = "tests/columns-pwid.qml";
const WIDTH: f32 = 1200.;
const HEIGHT: f32 = 600.;
const HERE: &str = "/probe/studio/deep/er";

fn sync(area: &mut Area, pane: &Pane, producers: &mut Producers, now: u64) {
    pane.with(|view| area.sync(&view, &mut producers.ports(), now));
}

/// The stub pane: a file under the cursor over a folder, so the third column previews the file.
fn studio(share: f64) -> (Area, Pane, Producers) {
    let mut area = Area::new(WIDTH, HEIGHT, None, share, SETTINGS);
    let pane = Pane::at(HERE, vec![row("a.txt", false), row("sub", true)]);
    let mut producers = Producers::default();
    sync(&mut area, &pane, &mut producers, 0);
    (area, pane, producers)
}

/// The cursor on the folder: its peek answers kid.txt and the third shows the folder.
fn folder(area: &mut Area, pane: &mut Pane, producers: &mut Producers, now: u64) {
    pane.cursor = 1;
    sync(area, pane, producers, now);
    let sub = format!("{HERE}/sub");
    let rows = [("kid.txt", false)];
    pane.with(|view| area.peeked(&reply(&sub, &rows), &view, &mut producers.ports(), now + 1));
}

/// The swap's owner releasing the file hold once the meta reply lands, so the file shows again.
fn release_hold(area: &mut Area, pane: &Pane, producers: &mut Producers, now: u64) {
    producers.swap.holding = false;
    let cursor = Cursor {
        row: true,
        is_dir: false,
        child: "",
    };
    area.host.show_cursor_row(cursor, false, false);
    sync(area, pane, producers, now);
}

fn rects(area: &Area) -> Vec<(f32, f32)> {
    area.layout
        .tracks()
        .map(|track| (track.x, track.width))
        .collect()
}

fn widths(area: &Area) -> Vec<f32> {
    area.layout.tracks().map(|track| track.width).collect()
}

/// The rects tile the area: each starts where the last ended, none is empty, the last ends at 1200.
fn tiled(rects: &[(f32, f32)]) -> bool {
    let mut at = 0.;
    for &(x, width) in rects {
        if x != at || width < 1. {
            return false;
        }
        at += width;
    }
    at == WIDTH
}

#[test]
fn the_edge_drags_clamps_stores_once_and_restores() {
    let mut c = Checks::new(SOURCE);
    let (mut area, mut pane, mut producers) = studio(0.);
    c.eq(
        233,
        "setup: a file under the cursor shows the preview column",
        area.host.third_shown(area.settings),
        true,
    );
    c.eq(
        234,
        "rest: every rect is today's third of 1200",
        widths(&area),
        vec![400., 400., 400.],
    );
    c.ok(235, "rest: the rects tile the area", tiled(&rects(&area)));
    c.eq(
        236,
        "rest: the edge zone exists",
        Edge::zone(&area.layout),
        (795., 9.),
    );
    c.eq(
        238,
        "rest: nothing was saved",
        (area.stored.len(), area.share),
        (0, 0.),
    );

    // Twenty moves of 6 px left: the edge holds within the drag distance, then follows each move.
    area.edge_press(800.);
    let built = (
        area.tracks_constructed(),
        area.tracks_dropped(),
        area.tracks_built(),
    );
    let mut good = true;
    for step in 1..=20 {
        let shift = 6. * step as f32;
        area.edge_move(800. - shift);
        let want = if shift > START_DRAG_DISTANCE {
            800. - shift
        } else {
            800.
        };
        good &= area.layout.edge() == want && tiled(&rects(&area));
    }
    c.ok(
        281,
        "drag: the edge held under the threshold, followed past it, and every frame tiled",
        good,
    );
    c.eq(
        282,
        "drag: mid-drag widths are whole pixels",
        widths(&area),
        vec![340., 340., 520.],
    );
    let after = (
        area.tracks_constructed(),
        area.tracks_dropped(),
        area.tracks_built(),
    );
    c.eq(
        283,
        "drag: no delegate was built or dropped in 20 moves",
        after,
        built,
    );
    c.eq(
        284,
        "drag: nothing is written while the pointer is down",
        (area.stored.len(), area.share),
        (0, 0.),
    );
    let _ = area.edge_release();
    c.eq(
        288,
        "release: exactly one write, the whole percent",
        area.stored.clone(),
        vec![43],
    );
    c.eq(288, "release: the stored share", area.share, 43.);
    c.eq(
        289,
        "release: the stored percent lays the rects out",
        widths(&area),
        vec![342., 342., 516.],
    );

    // Far left: the last column stops where each ancestor keeps 300 px.
    area.edge_press(684.);
    area.edge_move(300.);
    area.edge_move(0.);
    c.eq(
        295,
        "clamp: far left leaves each ancestor 300 px",
        widths(&area),
        vec![300., 300., 600.],
    );
    let _ = area.edge_release();
    c.eq(
        299,
        "clamp: far left writes 50",
        area.stored.clone(),
        vec![43, 50],
    );

    // Far right: the last column stops at 300 px.
    area.edge_press(600.);
    area.edge_move(900.);
    area.edge_move(1190.);
    c.eq(
        304,
        "clamp: far right leaves the last column 300 px",
        widths(&area),
        vec![450., 450., 300.],
    );
    let _ = area.edge_release();
    c.eq(
        308,
        "clamp: far right writes 25",
        area.stored.clone(),
        vec![43, 50, 25],
    );

    // j: the folder under the cursor shows in the last column at the stored width.
    folder(&mut area, &mut pane, &mut producers, 1);
    c.eq(
        314,
        "j: the folder shows in the last column",
        area.host.shown.is_dir,
        true,
    );
    c.eq(
        315,
        "j: file to folder leaves every edge where it was",
        widths(&area),
        vec![450., 450., 300.],
    );

    // k: back to the file; the swap holds for its preview until the meta reply releases it.
    pane.cursor = 0;
    sync(&mut area, &pane, &mut producers, 3);
    release_hold(&mut area, &pane, &mut producers, 4);
    c.eq(
        324,
        "k: the file shows in the last column again",
        area.host.shown.is_dir,
        false,
    );
    c.eq(
        325,
        "k: folder back to file leaves every edge where it was",
        widths(&area),
        vec![450., 450., 300.],
    );

    // A double click: press, release, press, the restore, release, with no pulled move.
    area.edge_press(900.);
    let _ = area.edge_release();
    area.edge_press(900.);
    let _ = area.edge_restore();
    let _ = area.edge_release();
    c.eq(
        329,
        "double click: every rect is today's third again",
        widths(&area),
        vec![400., 400., 400.],
    );
    c.eq(
        331,
        "double click: it writes 0",
        area.stored.clone(),
        vec![43, 50, 25, 0],
    );

    // The last drag, from the default third: 100 px left stores 42.
    let base = area.stored.len();
    area.edge_press(800.);
    area.edge_move(750.);
    area.edge_move(700.);
    let _ = area.edge_release();
    c.eq(
        345,
        "last drag writes 42",
        area.stored[base..].to_vec(),
        vec![42],
    );
    c.covers(&[34, 35]);
    c.finish();
}

#[test]
fn a_relaunch_lays_out_the_stored_share_on_its_first_pass() {
    let mut c = Checks::new(SOURCE);
    // The stored 42 percent: the third round(1200 * 0.42) = 504, each ancestor 696 / 2 = 348.
    let (area, _pane, _producers) = studio(42.);
    c.eq(355, "relaunch: the stored percent is read", area.share, 42.);
    // The first rebuild is what the first frame draws, so it must already carry the stored width.
    let first: Vec<(f32, f32)> = area
        .tracks()
        .iter()
        .map(|view| (view.x, view.width))
        .collect();
    let third = area
        .tracks()
        .iter()
        .find(|view| view.slot == Slot::Child)
        .map(|view| view.rows.clone());
    c.eq(
        356,
        "relaunch: a frame drew the preview column",
        third,
        Some(TrackRows::Preview),
    );
    let first_widths: Vec<f32> = first.iter().map(|rect| rect.1).collect();
    c.eq(
        358,
        "relaunch: the first frame already has the stored width",
        first_widths,
        vec![348., 348., 504.],
    );
    c.ok(
        359,
        "relaunch: the first frame tiles the area",
        tiled(&first),
    );
    c.eq(
        361,
        "relaunch: the width lays the rects out",
        widths(&area),
        vec![348., 348., 504.],
    );
    c.ok(
        362,
        "relaunch: the rects tile the area",
        tiled(&rects(&area)),
    );
    c.eq(364, "relaunch: nothing was written", area.stored.len(), 0);
    c.finish();
}

/// resting(): with no hover and no drag the edge draws neither its accent line nor its mark.
fn resting(c: &mut Checks, area: &Area, name: &str) {
    let edge = &area.host.edge;
    let lines = usize::from(edge.line(&area.layout, HEIGHT).is_some());
    let marks = usize::from(edge.mark(&area.layout, HEIGHT).is_some());
    c.eq(
        193,
        &format!("{name}: no accent line on the edge"),
        lines,
        0,
    );
    c.eq(194, &format!("{name}: no mark by the edge"), marks, 0);
}

#[test]
fn the_edge_draws_its_line_and_mark_only_while_hovered() {
    let mut c = Checks::new(SOURCE);
    let (mut area, mut pane, mut producers) = studio(0.);
    resting(&mut c, &area, "rest");
    area.edge_hover(100.);
    resting(&mut c, &area, "pointer elsewhere");
    area.edge_hover(800.);
    let line = area.host.edge.line(&area.layout, HEIGHT);
    let mark = area.host.edge.mark(&area.layout, HEIGHT);
    c.eq(
        247,
        "hover: one accent line",
        usize::from(line.is_some()),
        1,
    );
    c.eq(248, "hover: one mark", usize::from(mark.is_some()), 1);
    c.eq(
        252,
        "hover: the line is 1 px wide on the edge, full height",
        line,
        Some((799., 0., 1., HEIGHT)),
    );
    c.eq(
        257,
        "hover: the 16 px mark centres on the hairline and on the middle row (GM 2026-10-20)",
        mark,
        Some((791.5, (HEIGHT - 16.) / 2., 16.)),
    );
    area.edge_hover(100.);
    resting(&mut c, &area, "pointer left");

    // j: a folder's contents in the third at the stored 25 percent, the edge at 900.
    area.set_share(25.);
    folder(&mut area, &mut pane, &mut producers, 1);
    area.edge_hover(900.);
    let live = (
        usize::from(area.host.edge.line(&area.layout, HEIGHT).is_some()),
        usize::from(area.host.edge.mark(&area.layout, HEIGHT).is_some()),
    );
    c.eq(
        319,
        "j: the handle is live over a folder's contents",
        live,
        (1, 1),
    );
    let (relaunch, _pane, _producers) = studio(42.);
    resting(&mut c, &relaunch, "relaunch");
    c.covers(&[237, 242, 263, 363]);
    c.finish();
}

/// placed(): every drawn track sits where the layout puts it, the running sum from the left.
fn placed(area: &Area) -> bool {
    let drawn: Vec<(f32, f32)> = area
        .tracks()
        .iter()
        .map(|view| (view.x, view.width))
        .collect();
    drawn == rects(area)
}

#[test]
fn a_stored_share_places_every_drawn_track() {
    let mut c = Checks::new(SOURCE);
    let (mut area, mut pane, mut producers) = studio(0.);
    area.edge_press(800.);
    for step in 1..=20 {
        area.edge_move(800. - 6. * step as f32);
    }
    let _ = area.edge_release();
    c.ok(
        290,
        "release: the row placed every column where its width says",
        placed(&area),
    );
    // j and k re-lay the drawn tracks at 43 percent, then a double click restores the third.
    folder(&mut area, &mut pane, &mut producers, 1);
    pane.cursor = 0;
    sync(&mut area, &pane, &mut producers, 3);
    release_hold(&mut area, &pane, &mut producers, 4);
    area.edge_press(684.);
    let _ = area.edge_release();
    area.edge_press(684.);
    let _ = area.edge_restore();
    let _ = area.edge_release();
    c.ok(
        330,
        "double click: the row placed every column where its width says",
        placed(&area),
    );
    c.finish();
}
