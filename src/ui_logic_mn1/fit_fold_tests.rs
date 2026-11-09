//! The menu fold when the edge at full height falls just under a separator.
use super::entry::{Entry, Text};
use super::fit::*;
use super::testkit::check;

#[test]
fn an_edge_under_a_separator_trims_to_the_half_row_above_it() {
    let row = |i: usize| Entry::row(format!("row{i}"), Text::Data(format!("Row {i}")), "file");
    // The HIG file menu's shape: open, edit, compress, send, copy, toggles, then Move to Trash alone.
    let items: Vec<Entry> = [4, 5, 2, 2, 1, 2, 1]
        .iter()
        .enumerate()
        .flat_map(|(group, count)| {
            (group > 0)
                .then(Entry::separator)
                .into_iter()
                .chain((0..*count).map(row))
        })
        .collect();
    // At 220 the bottom edge misses the half-row cue, and no scroll cuts both edges at half.
    check(
        "the full height cuts no half row",
        cue_ok(&items, 0, 220),
        false,
    );
    let (y, h) = menu_cut(&items, 0, 220, None);
    check(
        "the view stops at the half row above the separator",
        (y, cue_ok(&items, y, h)),
        (0, true),
    );
    check(
        "the trim stays within one row and one separator",
        h > 220 - ROW - SEP,
        true,
    );
}

#[test]
fn a_view_short_of_two_half_rows_still_cuts_both_edges_near_half() {
    let row = |i: usize| Entry::row(format!("row{i}"), Text::Data(format!("Row {i}")), "file");
    // The copyas-menu-small file menu's shape; Copy As is the sixth row of the fifth group.
    let items: Vec<Entry> = [3, 5, 1, 2, 7, 2, 2]
        .iter()
        .enumerate()
        .flat_map(|(group, count)| {
            (group > 0)
                .then(Entry::separator)
                .into_iter()
                .chain((0..*count).map(row))
        })
        .collect();
    let copy_as = 3 + 5 + 1 + 2 + 4 + 5;
    // A 230 px frame at a 5 px inset leaves 218: five whole rows and two halves need at least 219.
    let view = 218;
    let (y, h) = menu_cut(
        &items,
        reveal(&items, copy_as, 0, view),
        view,
        Some(copy_as),
    );
    let (top, bottom) = edge_cuts(&items, y, h);
    check(
        "the full view stays, cut by half rows under both fades",
        (
            h,
            top.map(|(_, shown)| shown),
            bottom.map(|(_, shown)| shown),
        ),
        (view, Some(16), Some(17)),
    );
    check(
        "both cuts are rows, Copy As keeps clear of the fade",
        (
            top.is_some_and(|(index, _)| !items[index].separator),
            bottom.is_some_and(|(index, _)| !items[index].separator),
            keeps_clear(&items, Some(copy_as), y, h),
        ),
        (true, true, true),
    );
}
