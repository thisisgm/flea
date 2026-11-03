use super::*;
use std::collections::BTreeMap;

pub(super) fn owned(items: &[&str]) -> Vec<String> {
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

pub(super) fn held(entries: &[(usize, &str, Option<usize>)]) -> BTreeMap<usize, Held> {
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
    assert_eq!(
        delimiter("/home/gm/bench.csv"),
        Some(','),
        "a csv name takes a comma"
    );
    assert_eq!(
        delimiter("/home/gm/DATA.CSV"),
        Some(','),
        "an upper case csv name takes a comma"
    );
    assert_eq!(
        delimiter("/home/gm/data.tsv"),
        Some('\t'),
        "a tsv name takes a tab"
    );
    assert_eq!(
        delimiter("/home/gm/DATA.Tsv"),
        Some('\t'),
        "an upper case tsv name takes a tab"
    );
    assert_eq!(
        delimiter("/home/gm/notes.txt"),
        None,
        "a text name is not a table"
    );
    assert_eq!(
        delimiter("/home/gm/xcsv"),
        None,
        "a name that only ends in csv is not a table"
    );
}

#[test]
fn csv_records_split() {
    let comma = Some(',');
    assert_eq!(
        cells_of(&["a,b,c"], comma),
        grid_of(vec![vec!["a", "b", "c"]]),
        "plain fields split on the delimiter"
    );
    assert_eq!(
        cells_of(&["a,,c"], comma),
        grid_of(vec![vec!["a", "", "c"]]),
        "an empty field between delimiters is an empty cell"
    );
    assert_eq!(
        cells_of(&["a,b,"], comma),
        grid_of(vec![vec!["a", "b", ""]]),
        "a trailing delimiter ends with an empty cell"
    );
    assert_eq!(
        cells_of(&["x,\"a,b\",y"], comma),
        grid_of(vec![vec!["x", "a,b", "y"]]),
        "a comma inside quotes stays in one cell"
    );
    assert_eq!(
        cells_of(&["\"say \"\"hi\"\"\",2"], comma),
        grid_of(vec![vec!["say \"hi\"", "2"]]),
        "a doubled quote inside quotes is one quote"
    );
    assert_eq!(
        cells_of(&["5\" disk,x"], comma),
        grid_of(vec![vec!["5\" disk", "x"]]),
        "a quote inside an unquoted field is a literal"
    );
    assert_eq!(
        cells_of(&["a", "", "b"], comma),
        grid_of(vec![vec!["a"], vec![""], vec!["b"]]),
        "an empty line is one empty cell"
    );
    assert_eq!(
        cells_of(&["a\tb,c\td"], Some('\t')),
        grid_of(vec![vec!["a", "b,c", "d"]]),
        "a tsv line splits on tabs and keeps its commas"
    );
    assert_eq!(
        cells_of(&["a,b"], Some('\t')),
        grid_of(vec![vec!["a,b"]]),
        "a tsv line keeps a comma as text, not a split"
    );
}

#[test]
fn csv_records_multiline() {
    let multi = records(&owned(&["1,\"two", "lines\",3", "4,5"]), Some(','));
    assert_eq!(
        multi.len(),
        2,
        "a quoted newline makes one record over two lines"
    );
    assert_eq!(
        multi[0].cells,
        owned(&["1", "two\nlines", "3"]),
        "a quoted newline is kept inside its cell"
    );
    assert_eq!(multi[0].span, 2, "the record reports the lines it spans");
    assert_eq!(
        multi[1].span, 1,
        "the next record starts after the spanned lines"
    );
    assert!(
        !multi[0].open,
        "a record closed on its last line is not open"
    );
}

#[test]
fn csv_records_open_quote() {
    let open = records(&owned(&["1,\"open", "still"]), Some(','));
    assert!(
        open[0].open,
        "a quote still open at the last line makes an open record"
    );
    assert_eq!(
        open[0].cells,
        owned(&["1", "open\nstill"]),
        "an open record keeps the text read so far"
    );
    assert_eq!(
        cells_of(&["\"a\"\"", "b\""], Some(',')),
        grid_of(vec![vec!["a\"\nb"]]),
        "a doubled quote split across a line end is two lines"
    );
}

#[test]
fn csv_one_line() {
    assert_eq!(
        one_line("two\nlines"),
        "two lines",
        "a quoted newline shows on one line"
    );
}

#[test]
fn csv_plain_lines() {
    assert_eq!(plain_lines("").len(), 0, "empty bytes have no plain lines");
    assert_eq!(
        plain_lines("a,b\n1,2\n"),
        owned(&["a,b", "1,2"]),
        "a terminal newline adds no phantom plain row"
    );
    assert_eq!(
        plain_lines("a\n\n"),
        owned(&["a", ""]),
        "an intentional blank final line is kept"
    );
    assert_eq!(
        plain_lines("a\nb"),
        owned(&["a", "b"]),
        "an unterminated final line is kept"
    );
    assert_eq!(
        plain_lines("a\r\nb\r"),
        owned(&["a", "b"]),
        "raw plain lines strip CR as the backend does"
    );
}
