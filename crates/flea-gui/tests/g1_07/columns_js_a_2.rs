//! tests/js/columns.js runColumnCount, runColumnsLimitWire, runNeighbourAsks: count and asks.
use super::check::Checks;
use super::fixture::SETTINGS;
use flea::jsondoc::Json;
use flea_gui::columns::geometry::{
    ancestors_for_count, capped_limit, count_for_width, neighbour_asks,
};
use flea_gui::columns::Area;

/// A columnsLimit as a hand edit of ui.json writes it, read as a number or as no number at all.
fn raw_limit(text: &str) -> Option<f64> {
    let doc = flea::jsondoc::parse(&format!("{{\"columnsLimit\":{text}}}")).expect("limit is JSON");
    doc.get("columnsLimit").and_then(Json::as_f64)
}

#[test]
fn the_columns_count_follows_the_window_width() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        139,
        "a default install at 2560 px shows 3 columns",
        count_for_width(2560., None),
        3,
    );
    c.eq(
        140,
        "a stored 5 still opens 5 on a wide window",
        count_for_width(2560., Some(5.)),
        5,
    );
    c.eq(
        141,
        "a narrow window still shows 2 on the shipped default",
        count_for_width(800., None),
        2,
    );
    c.eq(
        142,
        "below 900 px the view draws 2 columns",
        count_for_width(899., Some(5.)),
        2,
    );
    c.eq(
        143,
        "at 900 px the view draws 3 columns",
        count_for_width(900., Some(5.)),
        3,
    );
    c.eq(
        144,
        "at 1700 px the view draws 4 columns",
        count_for_width(1700., Some(5.)),
        4,
    );
    c.eq(
        145,
        "at 2300 px the view draws 5 columns",
        count_for_width(2300., Some(5.)),
        5,
    );
    c.eq(
        146,
        "just under each step stays down",
        format!(
            "{}|{}",
            count_for_width(1699., Some(5.)),
            count_for_width(2299., Some(5.))
        ),
        "3|4".to_owned(),
    );
    c.eq(
        148,
        "the limit caps a wide window",
        count_for_width(2300., Some(3.)),
        3,
    );
    c.eq(
        149,
        "the limit caps a narrow window too",
        count_for_width(1200., Some(2.)),
        2,
    );
    c.eq(
        150,
        "a limit below the floor still draws 2",
        count_for_width(2300., Some(1.)),
        2,
    );
    c.eq(
        151,
        "a missing limit reads as the shipped 3",
        count_for_width(2300., None),
        3,
    );
    c.eq(
        152,
        "an empty limit reads as the shipped 3, not the floor",
        capped_limit(raw_limit("\"\"")),
        3,
    );
    c.eq(
        153,
        "a false limit reads as the shipped 3, not the floor",
        capped_limit(raw_limit("false")),
        3,
    );
    c.eq(154, "2 columns need no ancestor", ancestors_for_count(2), 0);
    c.eq(
        155,
        "3 columns read one ancestor",
        ancestors_for_count(3),
        1,
    );
    c.eq(
        156,
        "4 columns read two ancestors",
        ancestors_for_count(4),
        2,
    );
    c.eq(
        157,
        "5 columns read three ancestors",
        ancestors_for_count(5),
        3,
    );
    c.finish();
}

#[test]
fn the_area_passes_the_raw_stored_limit_through() {
    let mut c = Checks::new("tests/js/columns.js");
    let label = "the limit is not an int property";
    let mut area = Area::new(2560., 800., raw_limit("false"), 0., SETTINGS);
    c.eq(267, label, area.limit, None);
    c.eq(267, label, area.count(), 3);
    area.set_limit(raw_limit("\"\""));
    c.eq(267, label, area.count(), 3);
    area.set_limit(raw_limit("5"));
    c.eq(267, label, area.count(), 5);
    // The negative control: a limit coerced to 0 would draw the floor instead.
    area.set_limit(Some(0.));
    c.eq(267, label, area.count(), 2);
    c.finish();
}

#[test]
fn the_neighbour_asks_are_the_shown_ancestors_oldest_first() {
    let mut c = Checks::new("tests/js/columns.js");
    c.eq(
        312,
        "a narrow window asks no ancestor",
        neighbour_asks("/x/y", 800., Some(3.)).join("|"),
        String::new(),
    );
    c.eq(
        313,
        "widening asks the parent",
        neighbour_asks("/x/y", 1100., Some(3.)).join("|"),
        "/x".to_owned(),
    );
    c.eq(
        314,
        "an empty path asks nothing",
        neighbour_asks("", 1100., Some(3.)).join("|"),
        String::new(),
    );
    c.eq(
        315,
        "a fresh path asks its parent",
        neighbour_asks("/x/y", 1100., Some(3.)).join("|"),
        "/x".to_owned(),
    );
    c.eq(
        316,
        "at / no width asks anything",
        neighbour_asks("/", 2560., Some(5.)).join("|"),
        String::new(),
    );
    c.eq(
        317,
        "a wide window asks three ancestors",
        neighbour_asks("/a/b/c", 2560., Some(5.)).join("|"),
        "/|/a|/a/b".to_owned(),
    );
    c.finish();
}
