//! tests/columns-peekdrag.qml: a peek folder row stores no device of its own, and a drag asks devices for the visible peek folder rows.
use super::check::Checks;
use super::fixture::{area, peek_row, reply, row, Pane, Producers, SETTINGS};
use flea_gui::columns::peeks::peek_key;
use flea_gui::columns::Area;

#[test]
fn a_peek_folder_row_carries_no_device_of_its_own() {
    let mut c = Checks::new("tests/columns-peekdrag.qml");
    // The FAIL line is this test's finish.
    c.covers(&[45]);
    let mut area = area();
    let mut producers = Producers::default();
    let mut pane = Pane::at(
        "/fx/sub/work",
        vec![row("own", true), row("file.txt", false)],
    );
    pane.cursor = 1;
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    // The parent column /fx/sub answers one folder row, in the backend's n, d, i wire shape.
    let answer = reply("/fx/sub", &[("inbox", true)]);
    pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), 0));
    let held = area.peeks.rows(&peek_key("/fx/sub", false, false));
    let held = held.map(|rows| rows.to_vec());
    let label = "a peek folder row carries no device of its own";
    c.eq(143, label, held, Some(vec![peek_row("inbox", true)]));
    c.finish();
}

/// The probe's folderColumn and askColumn rows in one /fx/sub column, less "big", whose own device no PeekRow carries; far79 sits below the viewport.
fn ask_rows() -> Vec<(String, bool)> {
    let mut rows = vec![("inbox".to_owned(), true), ("a.txt".to_owned(), false)];
    rows.push(("deep".to_owned(), true));
    rows.extend((0..5).map(|at| (format!("pad{at}"), false)));
    rows.extend((0..80).map(|at| (format!("far{at}"), at == 79)));
    rows
}

#[test]
fn a_drag_asks_the_visible_peek_folder_rows() {
    let mut c = Checks::new("tests/columns-peekdrag.qml");
    // A 400 px tall view over /fx/sub/work, whose own listing holds no folder and whose cursor is a file.
    let mut area = Area::new(1200., 400., None, 0., SETTINGS);
    let mut producers = Producers::default();
    let pane = Pane::at("/fx/sub/work", vec![row("file.txt", false)]);
    pane.with(|view| area.sync(&view, &mut producers.ports(), 0));
    let rows = ask_rows();
    let rows: Vec<(&str, bool)> = rows
        .iter()
        .map(|(name, dir)| (name.as_str(), *dir))
        .collect();
    let answer = reply("/fx/sub", &rows);
    pane.with(|view| area.peeked(&answer, &view, &mut producers.ports(), 0));
    // RowDrag.enter with token t1: one askDropDevs over every drawn column's visible folder rows that name no device.
    let asked = format!("t1:{}", area.drop_asks().join(","));
    let label =
        "the drag asks the visible peek folder rows that name no device, in the drawn columns only";
    c.eq(
        146,
        label,
        asked,
        "t1:/fx/sub/inbox,/fx/sub/deep".to_owned(),
    );
    c.finish();
}
