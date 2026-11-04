//! tests/columnsmoves.qml moves: twelve navigations, each judged over every frame from its request to its settled landing.
use super::check::Checks;
use super::cols_moves::{child_of, Sim};
use super::cols_moves_2::{Census, BASE, RAIL};
use flea_gui::columns::geometry::Slot;
use flea_gui::navigation::history::parent_of;

const SOURCE: &str = "tests/columnsmoves.qml";

const CASES: [&str; 12] = [
    "parent-click",
    "grandparent-click",
    "third-click",
    "right",
    "enter",
    "left",
    "back",
    "chrome-up",
    "backspace",
    "rail",
    "arrow",
    "parent-file-click",
];

/// The path each move lands on.
fn expected(name: &str) -> String {
    match name {
        "left" | "back" | "chrome-up" | "backspace" | "arrow" => BASE.to_owned(),
        "parent-click" => "/fixture/group/b".to_owned(),
        "parent-file-click" => "/fixture/group/file".to_owned(),
        "grandparent-click" => "/fixture/other".to_owned(),
        "third-click" => format!("{BASE}/child/grand"),
        "rail" => RAIL.to_owned(),
        _ => format!("{BASE}/child"),
    }
}

/// issue(): each move as the pane receives it; a click opens the folder its row names.
fn issue(sim: &mut Sim, c: &mut Checks, name: &str) {
    match name {
        "parent-click" | "grandparent-click" | "third-click" | "parent-file-click" => {
            let (slot, index) = match name {
                "parent-click" => (Slot::Parent, 1),
                "grandparent-click" => (Slot::Grandparent, 1),
                "third-click" => (Slot::Child, 0),
                _ => (Slot::Parent, 2),
            };
            let target = sim.click(c, slot, index);
            sim.open(&target, "");
        }
        // Right and Return open the cursor folder.
        "right" | "enter" => {
            let target = child_of(&sim.pane);
            sim.open(&target, "");
        }
        // Left, Back, Up and Backspace return to the parent with the folder left under the cursor.
        "left" | "back" | "chrome-up" | "backspace" => {
            let here = sim.pane.here.clone();
            sim.open(parent_of(&here), &here);
        }
        "rail" => sim.open(RAIL, ""),
        _ => {
            sim.pane.cursor = 1;
            sim.sync();
        }
    }
}

/// judge(): no column that has rows before and after a move draws none in between, and the third column never vanishes.
fn judge(c: &mut Checks, sim: &Sim, name: &str, before: &Census, samples: &[Census]) {
    let after = sim.census();
    c.eq(
        170,
        &format!("{name}: path=<pane.path> expected=<expected>"),
        sim.pane.here.clone(),
        expected(name),
    );
    let mut holes = Vec::new();
    let mut lost_third = Vec::new();
    for (frame, state) in samples.iter().enumerate() {
        for (column, rows) in before.columns.iter().enumerate() {
            let kept = after.columns.get(column).is_some_and(|rows| *rows > 0);
            let empty = state.columns.get(column).is_none_or(|rows| *rows == 0);
            if *rows > 0 && kept && empty {
                holes.push((frame + 1, column));
            }
        }
        if before.third && after.third && !state.third {
            lost_third.push(frame + 1);
        }
    }
    c.eq(
        178,
        &format!("{name}: frame=<n> column=<i> rows=0"),
        holes,
        Vec::new(),
    );
    c.eq(
        180,
        &format!("{name}: frame=<n> third=empty"),
        lost_third,
        Vec::new(),
    );
    c.ok(
        184,
        &format!("{name}: no rendered frame sampled"),
        !samples.is_empty(),
    );
}

#[test]
fn every_move_keeps_each_non_empty_column_drawn() {
    let mut c = Checks::new(SOURCE);
    let mut sim = Sim::moves();
    for (index, name) in CASES.iter().enumerate() {
        sim.set_limit(if *name == "grandparent-click" { 4. } else { 3. });
        sim.open(BASE, "");
        sim.settle(&mut c, name, 2, 4);
        if index == 0 {
            // The warm-up clicks the parent's second row once, then re-opens the base a frame later.
            let target = sim.click(&mut c, Slot::Parent, 1);
            sim.open(&target, "");
            sim.frame();
            sim.open(BASE, "");
            sim.settle(&mut c, name, 2, 4);
        }
        if matches!(*name, "left" | "back" | "chrome-up" | "backspace") {
            sim.open(&format!("{BASE}/child"), "");
            sim.settle(&mut c, name, 2, 4);
        }
        let before = sim.census();
        issue(&mut sim, &mut c, name);
        let samples = sim.settle(&mut c, name, 3, 5);
        judge(&mut c, &sim, name, &before, &samples);
    }
    // The item lookups, the per-move log line and the case loop are this test's own steps.
    c.covers(&[152, 153, 154, 155, 159, 160, 162, 186, 189, 342, 345, 346]);
    c.finish();
}
