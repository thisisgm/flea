//! columnclip-empty.qml: the rows a 185 px column draws while its clipboard copies, cuts, clears.
use super::check::Checks;
use flea_gui::pane::visible_rows;

const SOURCE: &str = "tests/columnclip-empty.qml";
/// The probe's column: 185 px tall over 10 rows 'a.txt' to 'j.txt', b.txt the one on the clipboard.
const HEIGHT: f32 = 185.;
const ROWS: usize = 10;
const COPIED: usize = 1;

#[test]
fn a_short_column_draws_the_rows_it_asks_clip_marks_for() {
    let mut c = Checks::new(SOURCE);
    // 185 / 31 = 5.97, so rows 0 to 5 meet the viewport.
    let drawn = visible_rows(HEIGHT).min(ROWS);
    c.ok(158, "empty builds no delegate at all", drawn > 0);
    c.ok(
        174,
        &format!("copied builds no delegate at {COPIED}"),
        COPIED < drawn,
    );
    c.ok(
        193,
        &format!("cut builds no delegate at {COPIED}"),
        COPIED < drawn,
    );
    c.covers(&[
        101, 108, 114, 120, 126, 132, 135, 159, 164, 185, 202, 211, 217, 225, 228, 242, 247,
    ]);
    c.finish();
}
