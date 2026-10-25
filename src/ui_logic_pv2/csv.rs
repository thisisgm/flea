//! Pure rules for the CSV and TSV table preview: delimiter, records, numbers, alignment and layout.

use std::collections::BTreeMap;

/// One parsed record: its cells, the physical lines it spans, and whether its quote never closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub cells: Vec<String>,
    pub span: usize,
    pub open: bool,
}

/// A held physical line: its text and, for a record-aligned block, its starting record ordinal.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub t: String,
    pub record: Option<usize>,
}

/// A table row: its first physical line, record ordinal, line span and cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub line: usize,
    pub record: usize,
    pub span: usize,
    pub cells: Vec<String>,
}

/// An unfinished record: its first line, last line and record ordinal.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenRecord {
    pub line: usize,
    pub to: usize,
    pub record: usize,
}

/// The parsed table: header, body rows held once, indexes of rows by line and by record, column count and open records.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub header: Option<Vec<String>>,
    pub rows: Vec<Row>,
    pub by_line: BTreeMap<usize, usize>,
    pub by_record: BTreeMap<usize, usize>,
    pub cols: usize,
    pub open: Vec<OpenRecord>,
}

impl Table {
    /// The body row whose first physical line is `line`.
    pub fn at_line(&self, line: usize) -> Option<&Row> {
        self.by_line.get(&line).map(|&i| &self.rows[i])
    }

    /// The body row with record ordinal `record`.
    pub fn at_record(&self, record: usize) -> Option<&Row> {
        self.by_record.get(&record).map(|&i| &self.rows[i])
    }
}

/// The field separator for a table by extension, in any case; None for any other name.
pub fn delimiter(path: &str) -> Option<char> {
    let dot = path.rfind('.')?;
    match path[dot + 1..].to_lowercase().as_str() {
        "csv" => Some(','),
        "tsv" => Some('\t'),
        _ => None,
    }
}

/// Records of whole lines, read from the first line to the last.
pub fn records(lines: &[String], delim: Option<char>) -> Vec<Record> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let rec = record(lines, at, delim);
        at += rec.span;
        out.push(rec);
    }
    out
}

/// One record from lines[at]; a quoted field may run on into the following lines.
pub fn record(lines: &[String], at: usize, delim: Option<char>) -> Record {
    let mut cells: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoting = false;
    let mut field_start = true;
    let mut i = at;
    loop {
        let chars: Vec<char> = lines[i].chars().collect();
        let mut p = 0;
        while p < chars.len() {
            let c = chars[p];
            if quoting {
                if c != '"' {
                    field.push(c);
                } else if chars.get(p + 1) == Some(&'"') {
                    field.push('"');
                    p += 1;
                } else {
                    quoting = false;
                }
            } else if Some(c) == delim {
                cells.push(std::mem::take(&mut field));
                field_start = true;
                p += 1;
                continue;
            } else if c == '"' && field_start {
                quoting = true;
            } else {
                field.push(c);
            }
            field_start = false;
            p += 1;
        }
        if !quoting {
            cells.push(field);
            return Record {
                cells,
                span: i - at + 1,
                open: false,
            };
        }
        i += 1;
        if i >= lines.len() {
            cells.push(field);
            return Record {
                cells,
                span: i - at,
                open: true,
            };
        }
        field.push('\n');
    }
}

/// Parses the held lines into a table; each record-aligned block carries its starting ordinal.
pub fn table(held: &BTreeMap<usize, Held>, total: usize, delim: Option<char>) -> Table {
    let last = held.keys().copied().max().unwrap_or(0);
    let mut all: Vec<Row> = Vec::new();
    let mut open: Vec<OpenRecord> = Vec::new();
    let mut i = 1;
    while i <= last {
        if !held.contains_key(&i) {
            i += 1;
            continue;
        }
        let start = i;
        let mut lines: Vec<String> = Vec::new();
        while i <= last {
            match held.get(&i) {
                Some(entry) => lines.push(entry.t.clone()),
                None => break,
            }
            i += 1;
        }
        let first = &held[&start];
        if start != 1 && first.record.is_none() {
            continue;
        }
        let mut at = start;
        let from = match first.record {
            Some(record) if record != 0 => record,
            _ => 1,
        };
        if start == 1 {
            strip_bom(&mut lines);
        }
        let last_line = start + lines.len() - 1;
        for (ordinal, rec) in (from..).zip(records(&lines, delim)) {
            if rec.open && last_line < total {
                open.push(OpenRecord {
                    line: at,
                    to: last_line,
                    record: ordinal,
                });
                break;
            }
            all.push(Row {
                line: at,
                record: ordinal,
                span: rec.span,
                cells: rec.cells,
            });
            at += rec.span;
        }
    }
    if all.is_empty() || all[0].line != 1 {
        return Table {
            header: None,
            rows: Vec::new(),
            by_line: BTreeMap::new(),
            by_record: BTreeMap::new(),
            cols: 0,
            open,
        };
    }
    // Rows are never padded: a missing cell reads as empty, and each row is kept once.
    let cols = all.iter().map(|row| row.cells.len()).max().unwrap_or(0);
    let rows = all.split_off(1);
    let header = all.pop().map(|row| row.cells);
    let mut by_line = BTreeMap::new();
    let mut by_record = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        by_line.insert(row.line, i);
        by_record.insert(row.record, i);
    }
    Table {
        header,
        rows,
        by_line,
        by_record,
        cols,
        open,
    }
}

/// True when every unfinished record starts after ordinal to, so the first screen is complete.
pub fn ready(t: &Table, to: usize) -> bool {
    t.open.iter().all(|rec| rec.record > to)
}

/// A UTF-8 byte-order mark opening the file is no field text; one later in the file stays cell text.
pub fn strip_bom(lines: &mut [String]) {
    if let Some(rest) = lines.first().and_then(|line| line.strip_prefix('\u{feff}')) {
        lines[0] = rest.to_string();
    }
}

/// The raw lines of backend text: a terminal newline adds no line, and one trailing CR is removed from each line.
pub fn plain_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
        .iter()
        .map(|&line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect()
}

/// A cell drawn on one line: each newline shows as a space.
pub fn one_line(text: &str) -> String {
    text.replace('\n', " ")
}

#[path = "csv_number.rs"]
mod number;
pub use number::{is_number, numeric_column, numeric_columns};
#[path = "csv_layout.rs"]
mod layout;
pub use layout::{data_rows, fit, grid, row_number, Slot};

#[cfg(test)]
#[path = "csv_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "csv_table_tests.rs"]
mod table_tests;
