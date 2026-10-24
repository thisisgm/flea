// Ported from tests/js/markdownpictures.js and tests/js/mddraw.js: an inline picture capped at its line, and a formula's box.
use super::*;

#[test]
fn a_picture_wider_than_its_line_scales_with_its_ratio_and_one_that_fits_stays() {
    assert_eq!(
        picture_fit(160.0, 40.0, 0.0),
        (160.0, 40.0),
        "limit 0 leaves a wide picture as is"
    );
    assert_eq!(
        picture_fit(60.0, 40.0, 100.0),
        (60.0, 40.0),
        "a picture narrower than the limit stays at its own height"
    );
    assert_eq!(
        picture_fit(160.0, 40.0, 100.0),
        (100.0, 25.0),
        "a wide picture is scaled with its ratio kept"
    );
    assert_eq!(
        picture_fit(1000.0, 1.0, 100.0),
        (100.0, 1.0),
        "a scaled height never falls to 0"
    );
}

#[test]
fn a_formula_box_reads_its_width_and_its_parts_above_and_below_the_baseline() {
    let svg = "<svg style=\"vertical-align: -3px;\" width=\"30px\" height=\"10px\" viewBox=\"0 -15 60 20\" role=\"img\"><path d=\"M0 0\"/></svg>";
    // The formula sits 15 view box units (7.5 px) above its baseline and 5 units (2.5 px) below, at 0.5 px per unit.
    assert_eq!(formula_box(svg), Some((30.0, 7.5, 2.5)));
    assert_eq!(
        formula_box("<svg><path/></svg>"),
        None,
        "an svg without a size has no box"
    );
    assert_eq!(formula_box(""), None, "no svg has no box");
}
