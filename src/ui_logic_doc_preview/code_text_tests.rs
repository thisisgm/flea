use super::*;

// Board ql-code: 7 lines take a two-digit gutter plus a cell; ql-code-wrap's 164 lines take three.
#[test]
fn code_gutter_fits_the_widest_number() {
    assert_eq!(gutter_cells(7), 3, "short files keep two digits and a cell");
    assert_eq!(gutter_cells(99), 3);
    assert_eq!(gutter_cells(164), 4, "three digits and a cell");
    assert_eq!(gutter_cells(0), 3, "an empty file keeps the gutter");
}

#[test]
fn code_wrap_breaks_at_a_word_or_anywhere() {
    assert_eq!(wrap("", 10), vec![0..0], "an empty line is one empty row");
    assert_eq!(wrap("short", 10), vec![0..5], "a line that fits is one row");
    assert_eq!(
        wrap("alpha beta gamma", 11),
        vec![0..11, 11..16],
        "a row ends after its last space in reach"
    );
    assert_eq!(
        wrap("alpha beta gamma", 8),
        vec![0..6, 6..11, 11..16],
        "every row breaks after a space"
    );
    assert_eq!(
        wrap("abcdefghij", 4),
        vec![0..4, 4..8, 8..10],
        "a word longer than the row breaks anywhere"
    );
    assert_eq!(
        wrap("abcd efgh", 4),
        vec![0..5, 5..9],
        "a space at the limit stays at the end of its row"
    );
}

#[test]
fn code_elide_cuts_before_the_edge() {
    assert_eq!(elide("short", 10), None, "a line that fits is drawn whole");
    assert_eq!(
        elide("0123456789ab", 10),
        Some(9),
        "a long line is cut to leave the ellipsis its cell"
    );
}

#[test]
fn code_runs_split_across_rows() {
    let lengths = [2, 3, 4];
    assert_eq!(
        runs_in(&lengths, 0..9),
        vec![(0, 0..2), (1, 0..3), (2, 0..4)],
        "a row holding every run"
    );
    assert_eq!(
        runs_in(&lengths, 3..7),
        vec![(1, 1..3), (2, 0..2)],
        "a wrapped row starts inside a run"
    );
    assert!(
        runs_in(&lengths, 9..9).is_empty(),
        "an empty row holds no run"
    );
}
