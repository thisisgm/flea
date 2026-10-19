//! tests/columns-pwid-legs.js: clicks, jitter, a lost grab and a hidden edge on the preview edge.
use super::check::Checks;
use super::fixture::{row, Pane, Producers, SETTINGS};
use flea_gui::columns::Area;

const SOURCE: &str = "tests/columns-pwid-legs.js";

/// The harness area: 1200 by 600 at the default third, a file under the cursor.
fn shell() -> (Area, Pane, Producers) {
    let mut area = Area::new(1200., 600., None, 0., SETTINGS);
    let pane = Pane::at("/fixture", vec![row("a.txt", false), row("sub", true)]);
    let mut producers = Producers::default();
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    (area, pane, producers)
}

/// The drawn columns' widths left to right, the third column last.
fn widths(area: &Area) -> Vec<f32> {
    area.layout.tracks().map(|track| track.width).collect()
}

/// The preview column's left edge, where the third column begins.
fn edge_x(area: &Area) -> f32 {
    area.layout.edge()
}

/// A double click on the zone: press and release, then a press that restores, then its release.
fn double_click(area: &mut Area, x: f32, saves: &mut Vec<u32>) {
    area.edge_press(x);
    saves.extend(area.edge_release());
    area.edge_press(x);
    saves.push(area.edge_restore());
    saves.extend(area.edge_release());
}

const THIRD: [f32; 3] = [400., 400., 400.];

#[test]
fn clicks_jitter_and_lost_grabs_store_no_drag() {
    let mut c = Checks::new(SOURCE);
    let (mut area, pane, mut producers) = shell();
    let mut saves: Vec<u32> = Vec::new();
    // A click or a jitter on the edge is not a drag: it writes nothing and keeps the default third.
    area.edge_press(800.);
    area.edge_move(801.);
    saves.extend(area.edge_release());
    c.eq(
        12,
        "jitter: a 1 px move between press and release writes nothing",
        saves.clone(),
        vec![],
    );
    c.eq(
        13,
        "jitter: the default third stays",
        widths(&area),
        THIRD.to_vec(),
    );
    area.edge_press(800.);
    area.edge_move(801.);
    area.edge_move(800.);
    saves.extend(area.edge_release());
    c.eq(
        20,
        "jitter: out and back writes nothing",
        saves.clone(),
        vec![],
    );
    c.eq(
        21,
        "jitter: out and back keeps the default third",
        widths(&area),
        THIRD.to_vec(),
    );
    area.edge_press(800.);
    saves.extend(area.edge_release());
    c.eq(
        25,
        "click: a plain click writes nothing",
        saves.clone(),
        vec![],
    );
    c.eq(
        26,
        "click: a plain click keeps the default third",
        widths(&area),
        THIRD.to_vec(),
    );
    // A grab lost mid-drag ends the drag like a release.
    area.edge_press(800.);
    area.edge_move(700.);
    c.eq(
        32,
        "cancel: the drag is live before the grab goes",
        widths(&area),
        vec![350., 350., 500.],
    );
    saves.extend(area.edge_release());
    c.eq(
        37,
        "cancel: the edge is no longer dragging",
        area.host.edge.dragging,
        false,
    );
    c.eq(
        38,
        "cancel: the live width is let go",
        area.host.edge.last,
        None,
    );
    area.edge_move(500.);
    c.eq(
        42,
        "cancel: later moves with no grab leave the width alone",
        widths(&area),
        vec![348., 348., 504.],
    );
    c.eq(
        43,
        "cancel: the width the user saw is the one saved",
        saves.clone(),
        vec![42],
    );
    // The button comes up after the grab went, and the zone it left takes nothing.
    saves.extend(area.edge_release());
    let x = edge_x(&area);
    double_click(&mut area, x, &mut saves);
    c.eq(
        49,
        "cancel: a double click restores the default third",
        widths(&area),
        THIRD.to_vec(),
    );
    c.eq(
        50,
        "cancel: the restore wrote 0",
        saves.clone(),
        vec![42, 0],
    );
    // The edge hidden mid-drag, the preview column gone under a held button, ends it the same way.
    area.edge_press(800.);
    area.edge_move(700.);
    c.eq(
        57,
        "hide: the drag is live before the edge goes",
        widths(&area),
        vec![350., 350., 500.],
    );
    area.settings.column = false;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    c.eq(
        62,
        "hide: the edge is no longer dragging",
        area.host.edge.dragging,
        false,
    );
    c.eq(
        63,
        "hide: the live width is let go",
        area.host.edge.last,
        None,
    );
    area.edge_move(500.);
    c.eq(
        67,
        "hide: later moves leave the width alone",
        widths(&area),
        vec![348., 348., 504.],
    );
    area.settings.column = true;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 2));
    saves.extend(area.edge_release());
    // A double click restores the third while its second press is held, then it jitters 1 px.
    area.edge_press(800.);
    saves.push(area.edge_restore());
    area.edge_move(801.);
    saves.extend(area.edge_release());
    c.eq(
        81,
        "double click with jitter: the default third is back and stays",
        widths(&area),
        THIRD.to_vec(),
    );
    c.finish();
}

/// The same legs with every stored share read from the Area, the hidden edge's release among them.
#[test]
fn the_hidden_edge_reports_its_write() {
    let mut c = Checks::new(SOURCE);
    let (mut area, pane, mut producers) = shell();
    area.edge_press(800.);
    area.edge_move(801.);
    area.edge_release();
    area.edge_press(800.);
    area.edge_move(801.);
    area.edge_move(800.);
    area.edge_release();
    area.edge_press(800.);
    area.edge_release();
    area.edge_press(800.);
    area.edge_move(700.);
    area.edge_release();
    area.edge_move(500.);
    area.edge_release();
    let x = edge_x(&area);
    double_click(&mut area, x, &mut Vec::new());
    area.edge_press(800.);
    area.edge_move(700.);
    area.settings.column = false;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 1));
    area.edge_move(500.);
    c.eq(
        68,
        "hide: the width the user saw is the one saved",
        area.stored.clone(),
        vec![42, 0, 42],
    );
    area.settings.column = true;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 2));
    area.edge_release();
    area.edge_press(800.);
    area.edge_restore();
    area.edge_move(801.);
    area.edge_release();
    c.eq(
        82,
        "double click with jitter: only the restore wrote",
        area.stored.clone(),
        vec![42, 0, 42, 0],
    );
    c.finish();
}
