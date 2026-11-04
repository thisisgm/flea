//! tests/js/columns.js runListWidths, runColumnFit, runStoredWidths: drag and fit widths.
use super::check::Checks;
use super::fixture::{row, words};
use flea::jsondoc::Json;
use flea::protocol::RowFacts;
use flea_gui::columns::sets::{
    autofit_width, cell_text, clamp_list_width, stored_number, MAX_LIST_WIDTH, MIN_LIST_WIDTH,
};

#[test]
fn a_dragged_edge_clamps_and_a_fit_takes_the_widest_held_cell() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        162,
        "a drag inside the rails lands as drawn",
        clamp_list_width(100.),
        100,
    );
    c.eq(
        163,
        "a drag under the floor clamps to it",
        clamp_list_width(10.),
        MIN_LIST_WIDTH,
    );
    c.eq(
        164,
        "a drag past the ceiling clamps to it",
        clamp_list_width(9999.),
        MAX_LIST_WIDTH,
    );
    c.eq(
        165,
        "a drag rounds to whole pixels",
        clamp_list_width(100.6),
        101,
    );
    c.eq(
        166,
        "autofit takes the widest held cell",
        autofit_width(&[70., 120., 90.], 70.),
        120,
    );
    c.eq(
        167,
        "autofit narrows a dragged-wide column to the widest held cell",
        autofit_width(&[60., 80.], 300.),
        80,
    );
    c.eq(
        169,
        "autofit clamps a wide cell to the ceiling",
        autofit_width(&[9999.], 70.),
        MAX_LIST_WIDTH,
    );
    c.eq(
        171,
        "autofit on no held rows keeps the column",
        autofit_width(&[], 70.),
        70,
    );
    // runColumnFit runs as its own test below.
    c.covers(&[173]);
    c.finish();
}

#[test]
fn a_fit_measures_the_strings_a_row_draws() {
    let mut c = Checks::new("tests/js/columns.js");
    let file = RowFacts {
        mode: 33188,
        size: 18000,
        ..row("f", false)
    };
    let link = RowFacts {
        mode: 41453,
        size: 18,
        ..row("l", false)
    };
    let folder = RowFacts {
        mode: 16877,
        size: 60,
        ..row("d", true)
    };
    c.eq(
        273,
        "a file fits its size",
        cell_text("size", &file, None, None, None, &words()),
        "18.0 kB".to_owned(),
    );
    c.eq(
        274,
        "a link fits its kind, not its target length",
        cell_text("size", &link, None, None, None, &words()),
        "link".to_owned(),
    );
    c.eq(
        276,
        "a folder without a walk fits the bare mark",
        cell_text("size", &folder, None, None, None, &words()),
        "\u{b7}".to_owned(),
    );
    c.eq(
        278,
        "a folder with a walk fits its size",
        cell_text(
            "size",
            &folder,
            None,
            None,
            Some((124_700_000, false)),
            &words(),
        ),
        "124.7 MB".to_owned(),
    );
    c.eq(
        280,
        "a partial walk keeps its mark",
        cell_text("size", &folder, None, None, Some((1000, true)), &words()),
        ">1.0 kB".to_owned(),
    );
    c.eq(
        282,
        "a row with no mtime fits the bare mark",
        cell_text("date", &file, None, None, None, &words()),
        "--".to_owned(),
    );
    // The kind dictionary holds one name, so index 99 resolves to no name at all.
    let kinds = ["File"];
    c.eq(
        284,
        "a kind past the dictionary reads empty, never a crash",
        cell_text("kind", &file, None, kinds.get(99).copied(), None, &words()),
        String::new(),
    );
    c.finish();
}

#[test]
fn a_stored_width_is_only_a_finite_number() {
    let mut c = Checks::new("tests/js/columns.js");
    let num = |text: &str| Json::Num(text.to_owned());
    c.eq(
        178,
        "a stored number lands as drawn",
        stored_number(&num("120")),
        Some(120),
    );
    c.eq(
        179,
        "and rounds to whole pixels",
        stored_number(&num("120.6")),
        Some(121),
    );
    c.eq(
        180,
        "null keeps the measured width",
        stored_number(&Json::Null),
        None,
    );
    c.eq(
        181,
        "an empty string keeps it too",
        stored_number(&Json::Str(String::new())),
        None,
    );
    c.eq(
        182,
        "false keeps it too",
        stored_number(&Json::Bool(false)),
        None,
    );
    c.eq(
        183,
        "an empty array keeps it too",
        stored_number(&Json::Arr(Vec::new())),
        None,
    );
    c.eq(
        184,
        "true is not a width",
        stored_number(&Json::Bool(true)),
        None,
    );
    c.eq(
        185,
        "a numeric string is not a width",
        stored_number(&Json::Str("120".to_owned())),
        None,
    );
    c.eq(
        186,
        "a negative is not a width",
        stored_number(&num("-40")),
        None,
    );
    c.eq(187, "NaN is not a width", stored_number(&num("NaN")), None);
    c.eq(
        188,
        "Infinity is not a width",
        stored_number(&num("Infinity")),
        None,
    );
    c.finish();
}
