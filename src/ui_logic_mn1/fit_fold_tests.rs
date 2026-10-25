//! The menu fold when the edge at full height falls just under a separator.
use super::entry::{Entry, Text};
use super::fit::*;
use super::testkit::check;

#[test]
fn an_edge_under_a_separator_trims_to_the_half_row_above_it() {
    let row = |i: usize| Entry::row(format!("row{i}"), Text::Data(format!("Row {i}")), "file");
    // The file menu's shape: Open's four rows, Move to Trash alone, then the edit, compress, send, copy and toggle groups.
    let items: Vec<Entry> = [4, 1, 5, 2, 2, 1, 2]
        .iter()
        .enumerate()
        .flat_map(|(group, count)| (group > 0).then(Entry::separator).into_iter().chain((0..*count).map(row)))
        .collect();
    // At 220 the bottom edge shows 15 px of the row after the second separator, and no scroll cuts both edges at half.
    check("the full height cuts no half row", cue_ok(&items, 0, 220), false);
    let (y, h) = menu_cut(&items, 0, 220, None);
    check("the view stops at the half row above the separator", (y, cue_ok(&items, y, h)), (0, true));
    check("the trim stays within one row and one separator", h > 220 - ROW - SEP, true);
}
