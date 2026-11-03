//! columnrow-geom.qml: a row's cells and the name budget each track hands it, at two advances.
use super::check::Checks;
use flea_gui::columns::geometry::{cells, name_budget, GAP, LANE, MARK_SLOT, PAD_X, SIZE_W};
use flea_gui::columns::paint::{CLIP, CUT};
use flea_gui::pane::visible_rows;

const SOURCE: &str = "tests/columnrow-geom.qml";
const LONG: &str = concat!(
    "a-very-long-filename-that-must-elide-in-the-middle-",
    "to-keep-its-extension-visible-",
    "0123456789abcdef0123456789abcdef0123456789abcdef.png"
);
const SHORT: &str = "columnrow-geom.txt";
/// The probe's columns are 200 px tall.
const COLUMN_H: f32 = 200.;
// Body advances: a 0.6 em face at 14 px and a narrower face, so no budget is pinned to one face.
const ADVANCES: [f32; 2] = [8.4, 8.0];

/// One checkPaneRow call: label, width, size shown, chevron, row, name, clipped, tight, budgets.
type PaneRow = (
    &'static str,
    f32,
    bool,
    bool,
    usize,
    &'static str,
    bool,
    bool,
    [usize; 2],
);

const PANE_ROWS: [PaneRow; 14] = [
    (
        "an active column at 366",
        366.,
        true,
        false,
        0,
        LONG,
        false,
        true,
        [24, 25],
    ),
    (
        "an active column at 853",
        853.,
        true,
        false,
        0,
        LONG,
        false,
        true,
        [82, 86],
    ),
    (
        "a clipped row at 366",
        366.,
        true,
        false,
        0,
        LONG,
        true,
        false,
        [24, 25],
    ),
    (
        "a peek column at 366",
        366.,
        false,
        false,
        0,
        LONG,
        false,
        true,
        [34, 36],
    ),
    (
        "a peek column at 853",
        853.,
        false,
        false,
        0,
        LONG,
        false,
        true,
        [92, 97],
    ),
    (
        "a short name at 366",
        366.,
        true,
        false,
        1,
        SHORT,
        false,
        false,
        [24, 25],
    ),
    (
        "a short peek name at 366",
        366.,
        false,
        false,
        1,
        SHORT,
        false,
        false,
        [34, 36],
    ),
    (
        "a chevron row at 366",
        366.,
        true,
        true,
        2,
        LONG,
        false,
        true,
        [22, 24],
    ),
    (
        "a chevron row at 853",
        853.,
        true,
        true,
        2,
        LONG,
        false,
        true,
        [80, 84],
    ),
    (
        "a chevron peek at 366",
        366.,
        false,
        true,
        2,
        LONG,
        false,
        true,
        [33, 35],
    ),
    (
        "a chevron peek at 853",
        853.,
        false,
        true,
        2,
        LONG,
        false,
        true,
        [91, 95],
    ),
    (
        "a file under the cursor at 366",
        366.,
        true,
        false,
        0,
        LONG,
        false,
        true,
        [24, 25],
    ),
    (
        "a directory off the cursor at 366",
        366.,
        true,
        false,
        2,
        LONG,
        false,
        true,
        [24, 25],
    ),
    (
        "a lifted directory at 366",
        366.,
        true,
        true,
        2,
        LONG,
        false,
        true,
        [22, 24],
    ),
];

#[test]
fn a_row_holds_its_name_between_the_mark_and_the_size() {
    let mut c = Checks::new(SOURCE);
    // A standalone row 280 wide is a track 280 wide plus its scroll lane, with its size shown.
    let track = 280. + LANE;
    let rows = [
        ("a file row", false, 132., 187., 266.),
        ("a directory row", true, 120., 175., 254.),
    ];
    for (label, chevron, want_name, want_size, want_chevron) in rows {
        let row = cells(track, true, chevron);
        let name_w = row.name_right - row.name_x;
        let size_right = row.size_right.unwrap_or(f32::NAN);
        let size_x = size_right - SIZE_W;
        // A row with no chevron keeps a chevron slot 0 wide at the content edge less the padding.
        let chevron_x = row.chevron_x.unwrap_or(track - LANE - PAD_X);
        let mark_right = row.mark_x + MARK_SLOT;
        let end = row.name_x + name_w;
        let left_of = format!("at or left of the mark edge {mark_right}");
        c.ok(
            221,
            &format!("{label} draws its name {name_w} wide, want > 0"),
            name_w > 0.,
        );
        c.eq(
            221,
            &format!("{label} draws its name {name_w} wide"),
            name_w,
            want_name,
        );
        c.ok(
            225,
            &format!("{label} ends its name at {end} over the size at {size_x}"),
            end <= size_x + 0.5,
        );
        c.eq(
            225,
            &format!("{label} starts its size at"),
            size_x,
            want_size,
        );
        c.ok(
            227,
            &format!("{label} ends its size at {size_right} over the chevron at {chevron_x}"),
            size_right <= chevron_x + 0.5,
        );
        c.eq(
            227,
            &format!("{label} sets its chevron at"),
            chevron_x,
            want_chevron,
        );
        c.ok(
            229,
            &format!("{label} starts its size at {size_x} {left_of}"),
            size_x > mark_right,
        );
    }
    c.covers(&[202, 209, 212, 332, 333, 334, 335, 423]);
    c.finish();
}

#[test]
fn a_track_hands_each_row_the_largest_budget_that_fits() {
    let mut c = Checks::new(SOURCE);
    let shown = visible_rows(COLUMN_H);
    for (at, advance) in ADVANCES.into_iter().enumerate() {
        for (label, width, size, chevron, index, name, clipped, tight, want) in PANE_ROWS {
            let label = format!("{label} at advance {advance}");
            let budget = name_budget(width, size, chevron, advance);
            let row = cells(width, size, chevron);
            let mut slot = row.name_right - row.name_x;
            let mut take = 0;
            if clipped {
                // The clip mark takes a gap and its square from the slot, in cells from the budget.
                slot -= GAP + CLIP;
                take = ((GAP + CLIP) / advance).ceil() as usize;
            }
            let cells_drawn = name.chars().count().min(budget.saturating_sub(take));
            let implicit = cells_drawn as f32 * advance;
            c.ok(
                236,
                &format!("{label} builds no delegate at {index}"),
                index < shown,
            );
            c.eq(
                240,
                &format!("{label} hands no budget, got {budget}"),
                budget,
                want[at],
            );
            c.ok(
                257,
                &format!("{label} overflows its slot: implicit {implicit} over {slot}"),
                implicit <= slot,
            );
            if !tight {
                continue;
            }
            c.ok(
                265,
                &format!("{label} fixture name too short for width {budget}"),
                budget + 1 < name.chars().count(),
            );
            c.ok(
                274,
                &format!("{label} builds no delegate for the tight check"),
                index < shown,
            );
            c.ok(
                283,
                &format!("{label} has no drawn name for the tight check"),
                slot > 0.,
            );
            let over = (budget + 1) as f32 * advance;
            let loose = format!("{label} budget {budget} is not tight");
            c.ok(
                288,
                &format!("{loose}: budget+1 still fits at {over} over {slot}"),
                over > slot,
            );
        }
        // The short name under the cursor is a file: it keeps the plain budget and fits it whole.
        let budget = name_budget(366., true, false, advance);
        c.ok(
            364,
            "a short name at 366 builds no delegate at 1",
            1 < shown,
        );
        c.ok(
            366,
            &format!("a short name draws elided at 366 (advance {advance}, budget {budget})"),
            SHORT.chars().count() <= budget,
        );
    }
    c.covers(&[
        270, 279, 336, 337, 338, 339, 340, 341, 342, 343, 344, 345, 346, 348, 351, 352, 354, 355,
        357, 358, 361, 367, 373, 378, 379, 384, 391, 392, 406, 414, 416, 417,
    ]);
    c.finish();
}

#[test]
fn a_cut_row_dims_at_the_board_opacity() {
    let mut c = Checks::new(SOURCE);
    c.eq(
        307,
        &format!("a scissors row dims at {CUT}, want 0.55"),
        CUT,
        0.55,
    );
    c.covers(&[310, 311]);
    c.finish();
}
