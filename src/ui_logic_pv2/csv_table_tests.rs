//! The parsed table: columns, rows, header, record ordinals and the held blocks that make them.
use super::tests::{held, owned};
use super::*;

#[test]
fn csv_columns_are_as_wide_as_the_widest_row() {
    let wide = held(&[(1, "a,b", None), (2, "c,d,e", None), (3, "f", None)]);
    assert_eq!(
        table(&wide, 3, Some(',')).cols,
        3,
        "the table is as wide as its widest row"
    );
    assert_eq!(
        table(&held(&[]), 0, Some(',')).cols,
        0,
        "an empty table has no columns"
    );
}

#[test]
fn csv_table_basic() {
    let fixture = held(&[
        (1, "fixture,files", None),
        (2, "photos,1000", None),
        (3, "music", None),
    ]);
    let t = table(&fixture, 3, Some(','));
    assert_eq!(
        t.header,
        Some(owned(&["fixture", "files"])),
        "the header is line 1 of the held block"
    );
    let lines: Vec<usize> = t.rows.iter().map(|r| r.line).collect();
    assert_eq!(lines, vec![2, 3], "body rows keep their first line number");
    assert_eq!(
        t.rows[1].cells,
        owned(&["music"]),
        "a short body row keeps its own cells"
    );
    assert_eq!(t.cols, 2, "the column count is the widest row");
    assert_eq!(
        t.at_line(3).unwrap().cells[0],
        "music",
        "a body row is found by its first line"
    );
    assert!(
        t.at_line(4).is_none(),
        "a line that starts no record is absent"
    );
}

#[test]
fn csv_table_record_ordinals() {
    let dense_held = held(&[
        (1, "id,note", Some(1)),
        (2, "1,\"two", None),
        (3, "lines\"", None),
        (4, "2,last", None),
    ]);
    let dense = table(&dense_held, 4, Some(','));
    let ordinals: Vec<usize> = dense.rows.iter().map(|r| r.record).collect();
    assert_eq!(
        ordinals,
        vec![2, 3],
        "a quoted continuation takes no extra record ordinal"
    );
    assert_eq!(
        dense.at_record(3).unwrap().line,
        4,
        "record 3 is found at physical line 4"
    );
    let numbers: Vec<usize> = dense.rows.iter().map(|r| row_number(r.record)).collect();
    assert_eq!(
        numbers,
        vec![1, 2],
        "record ordinals number rows without gaps"
    );
}

#[test]
fn csv_table_distant_blocks() {
    let distant_held = held(&[
        (1, "id,note", Some(1)),
        (801, "450,\"middle", Some(451)),
        (802, "end\"", None),
        (803, "451,last", None),
    ]);
    let distant = table(&distant_held, 900, Some(','));
    assert_eq!(
        distant.at_record(451).unwrap().line,
        801,
        "a distant aligned block starts at the backend ordinal"
    );
    assert_eq!(
        distant.at_record(452).unwrap().cells[0],
        "451",
        "a distant continuation takes no delegate ordinal"
    );
    assert_eq!(
        distant.rows.len(),
        2,
        "physical gaps are not fabricated into record rows"
    );
    let gap = table(&held(&[(1, "h", None), (5, "v", None)]), 5, Some(','));
    assert_eq!(
        gap.rows.len(),
        0,
        "an unknown gap ordinal is not numbered by physical line"
    );
}

#[test]
fn csv_table_held_blocks() {
    let no_header = table(&held(&[(2, "1,2", None)]), 2, Some(','));
    assert_eq!(no_header.header, None, "no header until line 1 is held");
    assert!(
        no_header.by_line.is_empty(),
        "no body rows are keyed without a header"
    );
    let open_mid = held(&[(1, "a", None), (2, "\"b", None)]);
    assert_eq!(
        table(&open_mid, 9, Some(',')).rows.len(),
        0,
        "a block that ends mid-record leaves the open record out"
    );
    assert_eq!(
        table(&open_mid, 2, Some(',')).rows.len(),
        1,
        "a block that ends the file keeps its open record"
    );
    let aligned = held(&[
        (1, "a", None),
        (2, "1", None),
        (5, "5", Some(5)),
        (6, "6", None),
    ]);
    let lines: Vec<usize> = table(&aligned, 6, Some(','))
        .rows
        .iter()
        .map(|r| r.line)
        .collect();
    assert_eq!(
        lines,
        vec![2, 5, 6],
        "record-aligned held blocks parse independently"
    );
}

#[test]
fn csv_table_aligned_blocks() {
    let aligned = held(&[
        (1, "id,note", None),
        (2, "1,ok", None),
        (401, "2,\"one", Some(400)),
        (402, "line two\",9", None),
        (403, "3,plain", None),
    ]);
    let t = table(&aligned, 900, Some(','));
    assert_eq!(
        t.at_line(401).unwrap().cells[1],
        "one\nline two",
        "an aligned multiline block keeps its record start"
    );
    assert!(
        t.at_line(402).is_none(),
        "an aligned quoted continuation starts no row"
    );
    assert_eq!(
        t.at_line(403).unwrap().cells[0],
        "3",
        "a complete row after the aligned record is retained"
    );
    assert!(ready(&t, 2), "a complete first screen is ready");
}

#[test]
fn csv_table_pending_records() {
    let pending_held = held(&[
        (1, "id,note", None),
        (2, "1,\"open", None),
        (3, "still", None),
    ]);
    let pending = table(&pending_held, 9, Some(','));
    assert_eq!(
        pending.open,
        vec![OpenRecord {
            line: 2,
            to: 3,
            record: 2
        }],
        "an incomplete record reports its line range"
    );
    assert!(
        !ready(&pending, 3),
        "a dropped open first-screen record is not ready"
    );
    assert!(
        ready(&pending, 1),
        "a completed screen before the open record stays ready"
    );
}

#[test]
fn csv_table_eof() {
    let eof = table(&held(&[(1, "a", None), (2, "\"open", None)]), 2, Some(','));
    assert_eq!(
        eof.open.len(),
        0,
        "an open quote at EOF is retained, with no dropped record"
    );
    assert!(ready(&eof, 2), "EOF is ready even for an unclosed quote");
    let open_header = table(&held(&[(1, "\"open", None)]), 2, Some(','));
    assert_eq!(
        open_header.header, None,
        "an open header is held back until it closes"
    );
}

#[test]
fn csv_table_a_leading_bom_never_starts_a_header_field() {
    for d in [',', '\t'] {
        let comma = held(&[
            (1, &format!("\u{feff}\"Name{d} full\"{d}Amount"), None),
            (2, &format!("Alice{d}10"), None),
        ]);
        let t = table(&comma, 2, Some(d));
        assert_eq!(
            t.header,
            Some(vec![format!("Name{d} full"), "Amount".to_string()]),
            "a quoted first header after a BOM stays one cell"
        );
        assert_eq!(t.cols, 2, "the BOM adds no column");
        assert_eq!(
            t.rows[0].cells,
            owned(&["Alice", "10"]),
            "the body stays under its headers"
        );
        let multiline = held(&[
            (1, "\u{feff}\"Full", None),
            (2, &format!("name\"{d}Amount"), None),
            (3, &format!("Alice{d}10"), None),
        ]);
        let t = table(&multiline, 3, Some(d));
        assert_eq!(
            t.header,
            Some(vec!["Full\nname".to_string(), "Amount".to_string()]),
            "a quoted newline in the first header stays in the header"
        );
        assert_eq!(t.rows.len(), 1, "the header's continuation is no data row");
        assert_eq!((t.rows[0].line, t.rows[0].record), (3, 2));
    }
    let later = table(
        &held(&[(1, "a,b", None), (2, "\u{feff}x,y", None)]),
        2,
        Some(','),
    );
    assert_eq!(
        later.rows[0].cells[0], "\u{feff}x",
        "a BOM on a later line is cell text"
    );
}
