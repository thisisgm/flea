//! tests/columnsfilter-edge.qml: a filter key that hides the cursor's folder never lands an empty third column while a match remains.
use super::check::Checks;
use super::cols_filter::{build_rows, Columns, BASE};
use super::fixture::{row, Pane};

/// Filter.apply over a pane whose shown is a binding: the query hides the cursor for one pass, then the cursor takes the first kept row.
fn apply(cols: &mut Columns, query: &str) {
    cols.key(query);
    let cursor = cols.pane.pane.cursor;
    let hidden = cols
        .pane
        .kept
        .as_ref()
        .filter(|kept| !kept.contains(&cursor));
    if let Some(first) = hidden.and_then(|kept| kept.first().copied()) {
        cols.cursor(first);
    }
}

fn file() -> (bool, bool, String) {
    (true, false, String::new())
}

fn empty() -> (bool, bool, String) {
    (false, false, String::new())
}

fn folder(path: &str) -> (bool, bool, String) {
    (true, true, path.to_owned())
}

#[test]
fn a_filter_that_hides_the_cursor_folder_never_draws_an_empty_third_column() {
    let mut c = Checks::new("tests/columnsfilter-edge.qml");
    // The stage timer, the fail helpers, the stage report and the pass line are this test's steps and finish.
    c.covers(&[
        157, 159, 167, 169, 170, 179, 195, 208, 231, 241, 245, 250, 257,
    ]);
    let mut pane = Pane::at(BASE, build_rows());
    pane.cursor = 1;
    pane.first = 35;
    let mut cols = Columns::new(1100., 600., pane);
    let clips = format!("{BASE}/Clips");
    let apps = format!("{BASE}/Apps");
    let none = Vec::<String>::new();
    cols.sync();

    // Stage 0: the cursor on the Clips folder, its rows answered and drawn.
    cols.answer(&clips, &[("take1.mov", false)]);
    c.eq(
        178,
        "stage 0 rest draws, want true,true,/probe/studio/Clips",
        cols.shown(),
        folder(&clips),
    );

    // Stage 1: "t" hides every folder and keeps balsam.txt first, so the file holds over the Clips picture.
    c.ok(
        182,
        "stage 1 rest leaves the third column held",
        cols.idle(),
    );
    cols.log.clear();
    apply(&mut cols, "");
    apply(&mut cols, "t");
    c.eq(
        187,
        "stage 1 t drew an empty third column",
        cols.blanks(),
        none.clone(),
    );

    // Stage 2: the file's preview landed and released the hold.
    cols.release();
    c.eq(
        190,
        "stage 2 t, after the hold drew an empty third column",
        cols.blanks(),
        none.clone(),
    );
    c.eq(
        191,
        "stage 2 t, after the hold draws, want true,false,",
        cols.shown(),
        file(),
    );
    c.ok(
        192,
        "stage 2 t, after the hold leaves the third column held",
        cols.idle(),
    );
    apply(&mut cols, "");
    cols.cursor(1);

    // Stage 3: "a" hides Clips and keeps the unanswered Apps first, so Clips stays up until Apps answers.
    c.eq(
        198,
        "stage 3 back on Clips draws, want the Clips column",
        cols.shown(),
        folder(&clips),
    );
    c.ok(
        199,
        "stage 3 back on Clips leaves the third column held",
        cols.idle(),
    );
    cols.log.clear();
    apply(&mut cols, "");
    apply(&mut cols, "a");
    c.eq(
        204,
        "stage 3 a drew an empty third column",
        cols.blanks(),
        none.clone(),
    );
    cols.answer(&apps, &[("take1.mov", false)]);
    c.eq(
        206,
        "stage 3 a, Apps answered draws, want the Apps column",
        cols.shown(),
        folder(&apps),
    );
    c.eq(
        207,
        "stage 3 a, Apps answered drew an empty third column",
        cols.blanks(),
        none,
    );

    // Stage 4: a query matching nothing has no row to move to, so the column empties at once.
    apply(&mut cols, "az");
    c.eq(
        212,
        "stage 4 az draws, want false,false,",
        cols.shown(),
        empty(),
    );
    apply(&mut cols, "");

    // Stage 5: Escape restores the Apps column; then a listing refresh renames Apps under "ap".
    c.eq(
        217,
        "stage 5 esc draws, want the Apps column",
        cols.shown(),
        folder(&apps),
    );
    for query in ["", "a", "ap"] {
        apply(&mut cols, query);
    }
    cols.pane.pane.rows[0] = row("Bins", true);
    cols.pane.pane.listing += 1;
    cols.pane.query("ap");
    cols.sync();
    // Qt.callLater's settleThird: the deferred settle is due at once, and the shell's tick lands it.
    let due = cols.area.next_deadline();
    c.ok(
        226,
        "stage 6 refresh asks for a tick at once to settle the third",
        due.is_some_and(|at| at <= cols.now),
    );
    cols.tick();

    // Stage 6: the cursor stands hidden while matches remain, so the column empties.
    c.eq(
        226,
        "stage 6 refresh draws, want false,false,",
        cols.shown(),
        empty(),
    );
    c.ok(
        227,
        "stage 6 refresh leaves the third column held",
        cols.idle(),
    );
    cols.key("");
    cols.pane.pane.rows = build_rows();
    cols.pane.pane.listing += 1;
    cols.sync();
    cols.cursor(3);
    cols.release();

    // Stage 7: the file row is shown; then a zero-row answer nulls the row under the unmoved index.
    c.eq(
        234,
        "stage 7 on balsam.txt draws, want true,false,",
        cols.shown(),
        file(),
    );
    cols.pane.pane.rows = Vec::new();
    cols.pane.pane.listing += 1;
    cols.sync();

    // Stage 8: the column drops the file, then the rows come back under index 3.
    c.eq(
        238,
        "stage 8 zero-row answer draws, want false,false,",
        cols.shown(),
        empty(),
    );
    c.ok(
        239,
        "stage 8 zero-row answer leaves the third column held",
        cols.idle(),
    );
    cols.pane.pane.rows = build_rows();
    cols.pane.pane.listing += 1;
    cols.sync();
    cols.release();

    // Stage 9: the file shows again.
    c.eq(
        244,
        "stage 9 rows back draws, want true,false,",
        cols.shown(),
        file(),
    );
    c.finish();
}
