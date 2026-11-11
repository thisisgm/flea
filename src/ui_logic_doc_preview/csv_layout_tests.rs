use super::*;

#[test]
fn csv_data_rows() {
    assert_eq!(data_rows(25), 24, "25 records including the header have 24 data rows");
    assert_eq!(data_rows(0), 0, "an empty file has no data rows");
}

#[test]
fn csv_fit_fitting_and_overflow() {
    assert_eq!(
        fit(&[40.0, 30.0], &[20.0, 10.0], &[false, true], 6.0, 100.0),
        vec![
            Slot { x: 0.0, w: 64.0, right: false },
            Slot { x: 70.0, w: 30.0, right: true },
        ],
        "a fitting table takes the first column's slack"
    );
    let narrowed = fit(&[40.0, 30.0, 50.0], &[20.0, 10.0, 10.0], &[false, false, false], 6.0, 100.0);
    assert!(
        narrowed.iter().any(|s| s.w == 20.0),
        "an overflowing table narrows the first column to its header"
    );
    let over = fit(
        &[40.0, 30.0, 50.0, 10.0],
        &[20.0, 10.0, 10.0, 5.0],
        &[false, false, false, false],
        6.0,
        100.0,
    );
    assert_eq!(over.len(), 3, "the columns past the frame's edge are not drawn");
    assert_eq!(over[2].x + over[2].w, 100.0, "the last drawn column elides at the frame's edge");
}

#[test]
fn csv_fit_empty_and_alignment() {
    assert_eq!(fit(&[40.0], &[20.0], &[false], 6.0, 0.0).len(), 0, "no column is drawn in an empty frame");
    assert!(fit(&[40.0], &[20.0], &[true], 6.0, 100.0)[0].right, "a right-aligned column keeps its alignment");
}

#[test]
fn csv_grid() {
    assert_eq!(
        grid(&[40.0], &[false], 42.0, 14.0),
        vec![Slot { x: 42.0, w: 68.0, right: false }],
        "a Quick Look box is its widest cell plus both paddings"
    );
    assert_eq!(
        grid(&[40.0, 10.0], &[false, false], 42.0, 14.0)[1].x,
        110.0,
        "the next Quick Look box starts where the last ends"
    );
    // rowCount's wording is the catalog key document.csv_rows, tested in the GUI unit.
}

#[test]
fn csv_row_number() {
    assert_eq!(row_number(2), 1, "the row number is the record ordinal less the header");
}

// tests/js/csv.js rowCount: the count reads through the catalog's plural, and grouping is the locale's.
#[test]
fn csv_row_count_reads_through_the_catalog() {
    let i18n = crate::i18n::test_catalogs::english();
    assert_eq!(i18n.trn("document.csv_rows", 1), "1 row", "one row reads as one row");
    assert_eq!(i18n.trn("document.csv_rows", 24), "24 rows", "24 rows reads as 24 rows");
    assert_eq!(i18n.trn("document.csv_rows", 0), "0 rows", "no row reads as 0 rows");
    let grouped = i18n.trn("document.csv_rows", 1024);
    assert_eq!(grouped, format!("{} rows", i18n.locale().format_count(1024)), "counts over a thousand take the locale's grouping");
}
