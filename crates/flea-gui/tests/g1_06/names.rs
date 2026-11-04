use flea_gui::row::{cells_of, middle_elide, right_elide};

fn cells(text: &str) -> usize {
    text.chars().map(cells_of).sum()
}

// tests/js/names.js's Format.middleElide cases, the one elide every list row draws its name through.
#[test]
fn names_js_middle_elide_cases_hold_for_the_row_name() {
    let full = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";
    assert_eq!(
        middle_elide(full, 49),
        "screenshot-2026-08-30-fi\u{2026}m-after-the-bench-v3.png",
        "the board sample elides to the board string"
    );
    assert!(
        middle_elide(full, 49).ends_with(".png"),
        "the board sample keeps its extension"
    );
    assert_eq!(
        middle_elide("IMG_4121.jpg", 49),
        "IMG_4121.jpg",
        "a name shorter than the width is untouched"
    );
    assert_eq!(
        middle_elide("1234567890", 10),
        "1234567890",
        "a name exactly at the width is untouched"
    );
    assert_eq!(
        middle_elide("a-very-long-filename-with-no-extension-at-all", 20),
        "a-very-lon\u{2026}on-at-all",
        "a name without an extension still elides in the middle"
    );
    assert_eq!(
        middle_elide(".bashrc", 20),
        ".bashrc",
        "a dotfile shorter than the width keeps its full ink"
    );
    assert!(
        middle_elide(".a-very-long-hidden-config-name", 20).starts_with('.'),
        "a long dotfile keeps its leading dot"
    );
    assert_eq!(
        middle_elide(".a-very-long-hidden-config-name", 20),
        ".a-very-lo\u{2026}nfig-name",
        "a long dotfile still elides in the middle"
    );
    assert_eq!(middle_elide("", 20), "", "an empty name stays empty");
    assert_eq!(
        middle_elide("photo-\u{1f4f7}-2026-08-30-final-review-v3.png", 20),
        "photo-\u{1f4f7}-2\u{2026}ew-v3.png",
        "a multi-byte name never splits a pair"
    );
    assert!(
        cells(&middle_elide(&format!("{}.jpg", "\u{5199}".repeat(30)), 20)) <= 20,
        "a CJK name elides to its cell budget"
    );
    assert_eq!(
        middle_elide("ab\u{1f4f7}cdefghij", 6),
        "ab\u{2026}ij",
        "a pair straddling the cut stays whole"
    );
    assert_eq!(
        middle_elide("abcde\u{301}x", 5),
        "ab\u{2026}e\u{301}x",
        "a combining mark rides with its base through the cut"
    );
    assert_eq!(
        middle_elide("abcdefghij", 3),
        "a\u{2026}j",
        "a tiny width keeps one character each side"
    );
    assert_eq!(
        middle_elide("abcdefghij", 2),
        "a\u{2026}",
        "a two-wide budget keeps the head and the mark"
    );
    assert_eq!(
        middle_elide(&"a".repeat(40), usize::MAX),
        "a".repeat(40),
        "an unbounded budget hands the name through"
    );
    // Wide glyphs count two cells, narrow ones one, as Format.cellWidthOf() counts them.
    assert_eq!(
        (cells_of('\u{1f389}'), cells_of('\u{4e2d}'), cells_of('a')),
        (2, 2, 1)
    );
}

#[test]
fn a_matched_name_keeps_its_head_and_never_splits_a_wide_glyph() {
    assert_eq!(
        right_elide("notes-from-the-meeting.txt", 10),
        "notes-fro\u{2026}"
    );
    assert_eq!(
        right_elide("\u{4e2d}\u{4e2d}\u{4e2d}\u{4e2d}", 5),
        "\u{4e2d}\u{4e2d}\u{2026}"
    );
    assert_eq!(right_elide("short", 10), "short");
}
