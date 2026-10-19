use super::*;
use std::collections::BTreeMap;

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn grid_of(rows: Vec<Vec<&str>>) -> Vec<Vec<String>> {
    rows.iter().map(|row| owned(row)).collect()
}

fn cells_of(lines: &[&str], delim: Option<char>) -> Vec<Vec<String>> {
    records(&owned(lines), delim)
        .into_iter()
        .map(|rec| rec.cells)
        .collect()
}

fn held(entries: &[(usize, &str, Option<usize>)]) -> BTreeMap<usize, Held> {
    entries
        .iter()
        .map(|(key, text, record)| {
            (
                *key,
                Held {
                    t: text.to_string(),
                    record: *record,
                },
            )
        })
        .collect()
}

#[test]
fn csv_delimiter() {
    assert_eq!(delimiter("/home/gm/bench.csv"), Some(','), "a csv name takes a comma");
    assert_eq!(delimiter("/home/gm/DATA.CSV"), Some(','), "an upper case csv name takes a comma");
    assert_eq!(delimiter("/home/gm/data.tsv"), Some('\t'), "a tsv name takes a tab");
    assert_eq!(delimiter("/home/gm/DATA.Tsv"), Some('\t'), "an upper case tsv name takes a tab");
    assert_eq!(delimiter("/home/gm/notes.txt"), None, "a text name is not a table");
    assert_eq!(delimiter("/home/gm/xcsv"), None, "a name that only ends in csv is not a table");
}

#[test]
fn csv_records_split() {
    let comma = Some(',');
    assert_eq!(cells_of(&["a,b,c"], comma), grid_of(vec![vec!["a", "b", "c"]]), "plain fields split on the delimiter");
    assert_eq!(cells_of(&["a,,c"], comma), grid_of(vec![vec!["a", "", "c"]]), "an empty field between delimiters is an empty cell");
    assert_eq!(cells_of(&["a,b,"], comma), grid_of(vec![vec!["a", "b", ""]]), "a trailing delimiter ends with an empty cell");
    assert_eq!(cells_of(&["x,\"a,b\",y"], comma), grid_of(vec![vec!["x", "a,b", "y"]]), "a comma inside quotes stays in one cell");
    assert_eq!(cells_of(&["\"say \"\"hi\"\"\",2"], comma), grid_of(vec![vec!["say \"hi\"", "2"]]), "a doubled quote inside quotes is one quote");
    assert_eq!(cells_of(&["5\" disk,x"], comma), grid_of(vec![vec!["5\" disk", "x"]]), "a quote inside an unquoted field is a literal");
    assert_eq!(cells_of(&["a", "", "b"], comma), grid_of(vec![vec!["a"], vec![""], vec!["b"]]), "an empty line is one empty cell");
    assert_eq!(cells_of(&["a\tb,c\td"], Some('\t')), grid_of(vec![vec!["a", "b,c", "d"]]), "a tsv line splits on tabs and keeps its commas");
    assert_eq!(cells_of(&["a,b"], Some('\t')), grid_of(vec![vec!["a,b"]]), "a tsv line keeps a comma as text, not a split");
}

#[test]
fn csv_records_multiline() {
    let multi = records(&owned(&["1,\"two", "lines\",3", "4,5"]), Some(','));
    assert_eq!(multi.len(), 2, "a quoted newline makes one record over two lines");
    assert_eq!(multi[0].cells, owned(&["1", "two\nlines", "3"]), "a quoted newline is kept inside its cell");
    assert_eq!(multi[0].span, 2, "the record reports the lines it spans");
    assert_eq!(multi[1].span, 1, "the next record starts after the spanned lines");
    assert!(!multi[0].open, "a record closed on its last line is not open");
}

#[test]
fn csv_records_open_quote() {
    let open = records(&owned(&["1,\"open", "still"]), Some(','));
    assert!(open[0].open, "a quote still open at the last line makes an open record");
    assert_eq!(open[0].cells, owned(&["1", "open\nstill"]), "an open record keeps the text read so far");
    assert_eq!(
        cells_of(&["\"a\"\"", "b\""], Some(',')),
        grid_of(vec![vec!["a\"\nb"]]),
        "a doubled quote split across a line end is two lines"
    );
}

#[test]
fn csv_padding_and_columns() {
    assert_eq!(padded(&owned(&["c"]), 3), owned(&["c", "", ""]), "a short row pads to the table's columns");
    assert_eq!(padded(&owned(&["a", "b", "c"]), 2), owned(&["a", "b", "c"]), "a long row keeps its extra cells");
    assert_eq!(
        column_count(&grid_of(vec![vec!["a", "b"], vec!["c", "d", "e"], vec!["f"]])),
        3,
        "the table is as wide as its widest row"
    );
    assert_eq!(column_count(&[]), 0, "an empty table has no columns");
}

#[test]
fn csv_table_basic() {
    let fixture = held(&[(1, "fixture,files", None), (2, "photos,1000", None), (3, "music", None)]);
    let t = table(&fixture, 3, Some(','));
    assert_eq!(t.header, Some(owned(&["fixture", "files"])), "the header is line 1 of the held block");
    let lines: Vec<usize> = t.rows.iter().map(|r| r.line).collect();
    assert_eq!(lines, vec![2, 3], "body rows keep their first line number");
    assert_eq!(t.rows[1].cells, owned(&["music", ""]), "a short body row pads to the table's columns");
    assert_eq!(t.cols, 2, "the column count is the widest row");
    assert_eq!(t.by_line[&3].cells[0], "music", "a body row is found by its first line");
    assert!(!t.by_line.contains_key(&4), "a line that starts no record is absent");
}

#[test]
fn csv_one_line() {
    assert_eq!(one_line("two\nlines"), "two lines", "a quoted newline shows on one line");
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
    assert_eq!(ordinals, vec![2, 3], "a quoted continuation takes no extra record ordinal");
    assert_eq!(dense.by_record[&3].line, 4, "record 3 is found at physical line 4");
    let numbers: Vec<usize> = dense.rows.iter().map(|r| row_number(r.record)).collect();
    assert_eq!(numbers, vec![1, 2], "record ordinals number rows without gaps");
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
    assert_eq!(distant.by_record[&451].line, 801, "a distant aligned block starts at the backend ordinal");
    assert_eq!(distant.by_record[&452].cells[0], "451", "a distant continuation takes no delegate ordinal");
    assert_eq!(distant.rows.len(), 2, "physical gaps are not fabricated into record rows");
    let gap = table(&held(&[(1, "h", None), (5, "v", None)]), 5, Some(','));
    assert_eq!(gap.rows.len(), 0, "an unknown gap ordinal is not numbered by physical line");
}

#[test]
fn csv_table_held_blocks() {
    let no_header = table(&held(&[(2, "1,2", None)]), 2, Some(','));
    assert_eq!(no_header.header, None, "no header until line 1 is held");
    assert!(no_header.by_line.is_empty(), "no body rows are keyed without a header");
    let open_mid = held(&[(1, "a", None), (2, "\"b", None)]);
    assert_eq!(table(&open_mid, 9, Some(',')).rows.len(), 0, "a block that ends mid-record leaves the open record out");
    assert_eq!(table(&open_mid, 2, Some(',')).rows.len(), 1, "a block that ends the file keeps its open record");
    let aligned = held(&[(1, "a", None), (2, "1", None), (5, "5", Some(5)), (6, "6", None)]);
    let lines: Vec<usize> = table(&aligned, 6, Some(',')).rows.iter().map(|r| r.line).collect();
    assert_eq!(lines, vec![2, 5, 6], "record-aligned held blocks parse independently");
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
    assert_eq!(t.by_line[&401].cells[1], "one\nline two", "an aligned multiline block keeps its record start");
    assert!(!t.by_line.contains_key(&402), "an aligned quoted continuation starts no row");
    assert_eq!(t.by_line[&403].cells[0], "3", "a complete row after the aligned record is retained");
    assert!(ready(&t, 2), "a complete first screen is ready");
}

#[test]
fn csv_table_pending_records() {
    let pending_held = held(&[(1, "id,note", None), (2, "1,\"open", None), (3, "still", None)]);
    let pending = table(&pending_held, 9, Some(','));
    assert_eq!(
        pending.open,
        vec![OpenRecord { line: 2, to: 3, record: 2 }],
        "an incomplete record reports its line range"
    );
    assert!(!ready(&pending, 3), "a dropped open first-screen record is not ready");
    assert!(ready(&pending, 1), "a completed screen before the open record stays ready");
}

#[test]
fn csv_table_eof() {
    let eof = table(&held(&[(1, "a", None), (2, "\"open", None)]), 2, Some(','));
    assert_eq!(eof.open.len(), 0, "an open quote at EOF is retained, with no dropped record");
    assert!(ready(&eof, 2), "EOF is ready even for an unclosed quote");
    let open_header = table(&held(&[(1, "\"open", None)]), 2, Some(','));
    assert_eq!(open_header.header, None, "an open header is held back until it closes");
}

#[test]
fn csv_plain_lines() {
    assert_eq!(plain_lines("").len(), 0, "empty bytes have no plain lines");
    assert_eq!(plain_lines("a,b\n1,2\n"), owned(&["a,b", "1,2"]), "a terminal newline adds no phantom plain row");
    assert_eq!(plain_lines("a\n\n"), owned(&["a", ""]), "an intentional blank final line is kept");
    assert_eq!(plain_lines("a\nb"), owned(&["a", "b"]), "an unterminated final line is kept");
    assert_eq!(plain_lines("a\r\nb\r"), owned(&["a", "b"]), "raw plain lines strip CR as the backend does");
}
