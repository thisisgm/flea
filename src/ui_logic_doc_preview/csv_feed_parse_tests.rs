// The parse holds each cell once: a line of 250,000 empty fields costs its own cells, not rows x columns.
use super::*;

fn job(lines: Vec<String>) -> Job {
    let total = lines.len();
    let held: BTreeMap<usize, Held> = lines
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            (
                i + 1,
                Held {
                    t,
                    record: (i == 0).then_some(1),
                },
            )
        })
        .collect();
    Job {
        held: Arc::new(held),
        total,
        delim: Some(','),
    }
}

#[test]
fn csv_feed_parse_holds_each_cell_once() {
    let mut lines = vec!["a,b".to_string(), ",".repeat(249_999)];
    lines.extend((3..=44).map(|_| "1,2".to_string()));
    let bytes: usize = lines.iter().map(|l| l.len() + 1).sum();
    assert!(bytes < 1 << 20, "under the backend's 1 MiB gate");
    let table = job(lines).run();
    let stored: usize = table.header.iter().flatten().count()
        + table.rows.iter().map(|row| row.cells.len()).sum::<usize>();
    assert_eq!(
        stored,
        2 + 250_000 + 42 * 2,
        "no row is padded to the widest row"
    );
    assert_eq!(table.cols, 250_000, "the widest row sets the column count");
    assert_eq!(table.rows.len(), 43, "one row per record, each kept once");
    assert_eq!(table.at_record(3).map(|row| row.cells.len()), Some(2));
}

// v0.3.12's CsvTable keeps every column and every whole cell, so the parse keeps them too.
#[test]
fn csv_feed_parse_keeps_every_column_and_whole_cell() {
    let header = (1..=1001)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    let long = format!("{}END", "abcdefghij".repeat(30));
    let mut row = vec!["1".to_string(); 1001];
    row[1] = long.clone();
    let table = job(vec![header, row.join(",")]).run();
    assert_eq!(table.cols, 1001, "every column is kept");
    assert_eq!(
        table
            .header
            .as_ref()
            .and_then(|h| h.last())
            .map(String::as_str),
        Some("c1001")
    );
    assert_eq!(table.rows[0].cells[1], long, "the whole cell is kept");
}
