use super::engine::*;
use super::*;

// malformed.csv and open.csv: an open quote holds the record whole, through the file's end or its closing quote.
#[test]
fn csv_feed_open_quotes_close_inside_the_first_screen() {
    let mut text = String::from("id,note\n1,\"starts here\n");
    for i in 0..60 {
        text.push_str(&format!("continued {i}\n"));
    }
    text.push_str("ends\",x\n2,b\n");
    let (mut feed, mut engine) = open(&text, ',', look(), 10);
    pump(&mut feed, &mut engine);
    assert!(
        feed.held.contains_key(&63),
        "the first screen fetched the closing quote"
    );
    assert!(
        feed.table.at_line(2).is_some(),
        "the complete record is drawn"
    );
    assert!(feed.first_screen());
    let unclosed = "id,note\n1,\"never closed\nmore\n";
    let (mut feed, mut engine) = open(unclosed, ',', look(), 10);
    assert_eq!(
        pump(&mut feed, &mut engine),
        1,
        "one request draws the first screen"
    );
    assert_eq!(
        feed.held.len(),
        feed.total,
        "one aligned reply retains the record through EOF"
    );
    assert_eq!(
        csv::one_line(&feed.table.at_line(2).unwrap().cells[1]),
        "never closed more",
        "the open quote at EOF is drawn"
    );
}

#[test]
fn csv_feed_quoted_newlines_keep_one_row_per_record() {
    let (mut feed, mut engine) = open("a,b\n1,\"two\nlines\"\n2,z\n", ',', look(), 22);
    pump(&mut feed, &mut engine);
    assert_eq!(
        feed.table.at_line(2).unwrap().cells[1],
        "two\nlines",
        "the cell keeps its newline"
    );
    assert_eq!(
        csv::one_line(&feed.table.at_line(2).unwrap().cells[1]),
        "two lines",
        "the newline draws as a space"
    );
    assert!(
        feed.table.at_line(3).is_none(),
        "the continuation line starts no row"
    );
    assert_eq!(feed.body_rows(), 2, "two model rows");
    let rows: Vec<usize> = feed
        .table
        .rows
        .iter()
        .map(|r| csv::row_number(r.record))
        .collect();
    assert_eq!(rows, vec![1, 2], "drawn numbers count records");
}

#[test]
fn csv_feed_a_jump_past_a_quoted_continuation_fabricates_no_row() {
    let mut text = String::from("id,v\n");
    for i in 1..1000 {
        if i == 600 {
            text.push_str(&format!("{i},\"a\nb\nc\"\n"));
        } else {
            text.push_str(&format!("row{i},{i}\n"));
        }
    }
    let (mut feed, mut engine) = open(&text, ',', look(), 22);
    pump(&mut feed, &mut engine);
    feed.set_view(22, 600);
    pump(&mut feed, &mut engine);
    assert!(
        feed.table.at_record(601).is_some(),
        "the record start is known"
    );
    let line = feed.table.at_record(601).unwrap().line;
    assert!(
        feed.table.at_line(line + 1).is_none(),
        "a quoted continuation draws no fabricated row"
    );
    assert_eq!(
        feed.table.at_record(602).unwrap().cells[0],
        "row601",
        "the first complete row after the quote is held"
    );
}

#[test]
fn csv_feed_plain_text_counts_a_terminal_newline_once() {
    let mut feed = Feed::new(Some(','), look());
    feed.answer_plain("a,b\n1,2\n");
    assert!(feed.ask().is_none(), "the caller's own text asks nothing");
    let table = feed.job().run();
    feed.apply(table);
    feed.measure(f64::INFINITY, &mut measure);
    assert_eq!(feed.total, 2, "two lines for a terminal newline");
    assert_eq!(feed.body_rows(), 1, "one body row");
    assert!(feed.table.at_line(3).is_none(), "no phantom row");
    assert!(feed.first_screen());
}

// Binary, large and failed answers draw no table and ask nothing more; an empty file has no header and 0 rows.
#[test]
fn csv_feed_refusals_and_empty_files() {
    for state in [Answer::Binary, Answer::Large, Answer::Failed] {
        let mut feed = Feed::new(Some(','), look());
        let mut engine = Engine::refusing(state);
        assert_eq!(pump(&mut feed, &mut engine), 1, "{state:?}: one ask");
        assert_eq!(feed.answer, state);
        assert!(!feed.first_screen(), "{state:?}: the table is not drawn");
        assert!(feed.ask().is_none(), "{state:?}: nothing more is asked");
    }
    let (mut feed, mut engine) = open("", ',', look(), 22);
    pump(&mut feed, &mut engine);
    assert!(feed.table.header.is_none(), "no header is drawn");
    assert_eq!(feed.body_rows(), 0, "the bar reads 0 rows");
    assert!(feed.settled());
}

// A reply for another token, a stale one after a reset, is dropped; a reset clears the held lines and frozen widths.
#[test]
fn csv_feed_stale_replies_and_resets() {
    let (mut feed, mut engine) = open(&bench(), ',', look(), 22);
    let ask = feed.ask().unwrap();
    let reply = engine.reply(&ask);
    feed.sent(reply.token);
    feed.reset(Some(','));
    assert!(!feed.answered(&reply), "a reply after a reset is dropped");
    assert!(feed.held.is_empty() && !feed.frozen && feed.widths.is_empty());
    let ask = feed.ask().unwrap();
    let reply = engine.reply(&ask);
    feed.sent(reply.token);
    assert!(
        !feed.answered(&Reply {
            token: reply.token + 9,
            ..reply.clone()
        }),
        "another token is dropped"
    );
    assert!(feed.ask().is_none(), "the ask in flight blocks the next");
    assert!(feed.answered(&reply));
    assert!(feed.ask().is_none(), "a pending parse blocks the next ask");
}

// The asks stay inside the frame's screen in the column and the view plus one screen each side in Quick Look.
#[test]
fn csv_feed_asks_stay_near_the_view() {
    let mut text = String::from("id,v\n");
    for i in 1..3000 {
        text.push_str(&format!("{i},{i}\n"));
    }
    let (mut feed, mut engine) = open(&text, ',', column(), 27);
    pump(&mut feed, &mut engine);
    assert!(
        engine
            .asks
            .iter()
            .all(|a| a.first >= 1 && a.first + a.count - 1 <= 27 * 2 + 1),
        "column asks stay inside its screen"
    );
    let (mut feed, mut engine) = open(&text, ',', look(), 22);
    for top in [2, 300, 1500, 2990] {
        feed.set_view(22, top);
        let before = engine.asks.len();
        pump(&mut feed, &mut engine);
        for a in &engine.asks[before..] {
            assert!(
                a.first >= top.saturating_sub(22).max(1) && a.first + a.count <= top + 2 * 22,
                "ask {a:?} near {top}"
            );
        }
    }
}

// Cost: the first screen measures only its own cells once, and a later screen measures nothing.
#[test]
fn csv_feed_measures_each_first_screen_cell_once() {
    let (mut feed, mut engine) = open(&bench(), ',', look(), 22);
    pump(&mut feed, &mut engine);
    let cells = 4 * (1 + 24);
    assert_eq!(
        feed.counters.measured, cells,
        "the header and the 24 first-screen rows of 4 columns, once each"
    );
    assert_eq!(feed.counters.parses, 1, "one parse");
    feed.set_view(22, 20);
    pump(&mut feed, &mut engine);
    assert_eq!(
        feed.counters.measured, cells,
        "a scroll measures nothing again"
    );
}

#[test]
fn csv_feed_elide() {
    assert_eq!(
        elide("abc", 3),
        ("abc".into(), false),
        "a fitting cell is drawn whole"
    );
    assert_eq!(
        elide("abcdef", 4),
        ("abc\u{2026}".into(), true),
        "a long cell ends in an ellipsis"
    );
    assert_eq!(
        elide("abc", 0),
        (String::new(), true),
        "no room draws nothing"
    );
}

// tests/csv-records.js: 801 multiline records, closed or open at EOF, in CSV and TSV, from the engine or the caller.
#[test]
fn csv_feed_multiline_records_keep_one_row_and_number_each() {
    let screen = |feed: &Feed, top: usize| {
        for n in top..(top + 22).min(803) {
            let row = &feed.table.at_record(n).unwrap();
            assert_eq!(
                (row.cells[0].clone(), csv::row_number(n)),
                ((n - 1).to_string(), n - 1),
                "record {n}"
            );
        }
    };
    for (delim, closed, plain) in (0..8).map(|i| ([',', '\t'][i & 1], i & 2 == 0, i & 4 == 0)) {
        let mut text = format!("id{delim}note\n");
        for i in 1..=801 {
            text.push_str(&match i {
                1 => format!("1{delim}\"first\nsecond\"\n"),
                450 => format!("450{delim}\"middle\nsecond\nthird\"\n"),
                801 => format!("801{delim}\"last\nend{}\n", if closed { "\"" } else { "" }),
                _ => format!("{i}{delim}row{i}\n"),
            });
        }
        let (mut feed, mut engine) = open(&text, delim, look(), 22);
        if plain {
            feed.answer_plain(&text);
            let table = feed.job().run();
            feed.apply(table);
            feed.measure(f64::INFINITY, &mut measure);
        }
        pump(&mut feed, &mut engine);
        assert_eq!(feed.body_rows(), 801, "one row per record");
        screen(&feed, 2);
        for top in [451, 802 - 21] {
            feed.set_view(22, top);
            pump(&mut feed, &mut engine);
            screen(&feed, top);
        }
        assert_eq!(
            feed.table.at_record(451).unwrap().cells[1],
            "middle\nsecond\nthird",
            "the far jump lands on record 450"
        );
        assert!(
            feed.table.at_record(802).unwrap().cells[1].starts_with("last\nend"),
            "EOF is record 801"
        );
        assert_eq!(feed.body_rows(), 801, "the count holds at EOF");
    }
}
