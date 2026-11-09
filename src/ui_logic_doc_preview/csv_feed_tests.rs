use super::engine::*;
use super::*;

#[test]
fn csv_feed_geometry_is_the_boards() {
    let g = look();
    assert_eq!(
        (g.row_height, g.head_height),
        (28., 33.),
        "Quick Look rows are 28 and the header 33 with its rule"
    );
    assert_eq!(
        (g.number_width, g.pad_x, g.edge),
        (42., 14., 14.),
        "the row numbers fill 42 and cells pad 14"
    );
    let c = column();
    assert_eq!(
        (c.row_height, c.head_height, c.edge, c.gap),
        (22., 23., 12., 6.),
        "the column's rows pad 3 and inset 12"
    );
    assert_eq!(
        Geometry::new(true, 18., 24., 21., 1.).number_width,
        63.,
        "the board values scale with the caption"
    );
}

// tests/csv-preview.qml: the column draws bench.csv as a table, asked raw, numbers on the right.
#[test]
fn csv_feed_column_draws_a_whole_first_screen() {
    let (mut feed, mut engine) = open(&bench(), ',', column(), 27);
    assert!(!feed.first_screen(), "nothing draws before an answer");
    let asks = pump(&mut feed, &mut engine);
    assert_eq!(asks, 1, "one ask fills the column's screen");
    assert!(feed.first_screen(), "the first screen is whole");
    assert_eq!(
        feed.table.header.as_deref(),
        Some(&["fixture", "files", "folders", "depth"].map(String::from)[..])
    );
    assert_eq!(feed.body_rows(), 24, "24 body rows");
    assert_eq!(
        feed.rights,
        vec![false, true, true, true],
        "numbers right-aligned, the rest left"
    );
    assert!(feed.frozen, "the first screen froze the widths");
    let slots = feed.layout(400.);
    let end = slots.last().map(|s| s.x + s.w).unwrap();
    assert!(
        end <= 400. - 2. * feed.geometry.edge,
        "the drawn columns end inside the frame"
    );
    assert_eq!(
        feed.cell_box(&slots[0]).0,
        12.,
        "the header has only the board's cell inset"
    );
    let line = ask_line(7, "column", "/d/bench.csv", ',', &engine.asks[0]);
    assert!(
        line.contains(r#""raw":true"#) && line.contains(r#""csv":",""#),
        "every ask is raw with its delimiter: {line}"
    );
}

#[test]
fn csv_feed_a_long_first_cell_elides_and_a_wide_file_keeps_the_columns_the_frame_holds() {
    let long = format!("name,size\n{},1\n", "x".repeat(200));
    let (mut feed, mut engine) = open(&long, ',', column(), 27);
    pump(&mut feed, &mut engine);
    let first = &feed.layout(400.)[0];
    let cells = (first.w / 7.2).floor() as usize;
    let (drawn, cut) = elide(&feed.table.at_line(2).unwrap().cells[0], cells);
    assert!(
        cut && drawn.ends_with('\u{2026}') && drawn.chars().count() == cells,
        "the long first cell is elided"
    );
    let mut wide = (1..=12)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    wide.push('\n');
    wide.push_str(
        &(1..=12)
            .map(|c| format!("value-{c}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    let (mut feed, mut engine) = open(&wide, ',', column(), 27);
    pump(&mut feed, &mut engine);
    assert_eq!(feed.table.cols, 12, "12 columns in the table");
    let slots = feed.layout(400.);
    assert!(
        slots.len() < 12,
        "only the columns the frame holds are drawn"
    );
    let end = slots.last().map(|s| s.x + s.w).unwrap();
    assert!(
        end <= 400. - 2. * feed.geometry.edge,
        "the drawn columns end inside the frame"
    );
    let (mut ql, mut engine) = open(&wide, ',', look(), 22);
    pump(&mut ql, &mut engine);
    assert_eq!(ql.layout(460.).len(), 12, "Quick Look keeps 12 boxes");
    assert!(ql.grid_width(460.) > 460., "the grid scrolls sideways");
}

#[test]
fn csv_feed_tsv_splits_on_tabs_and_keeps_commas() {
    let (mut feed, mut engine) = open("a\tb\tc\n1\tx,y\t3\n", '\t', column(), 27);
    pump(&mut feed, &mut engine);
    assert_eq!(
        feed.table.header.clone().unwrap(),
        vec!["a", "b", "c"],
        "the header splits on tabs"
    );
    assert_eq!(
        feed.table.at_line(2).unwrap().cells[1],
        "x,y",
        "a comma stays inside its cell"
    );
    assert!(
        ask_line(1, "look", "/d/f.tsv", '\t', &engine.asks[0]).contains(r#""csv":"\t""#),
        "the tab is the asked delimiter"
    );
}

// Quick Look: the same file as a grid, with row numbers and each box its widest cell plus both paddings.
#[test]
fn csv_feed_quick_look_grid() {
    let (mut feed, mut engine) = open(&bench(), ',', look(), 22);
    pump(&mut feed, &mut engine);
    assert!(feed.first_screen());
    let slots = feed.layout(460.);
    assert_eq!(slots.len(), 4, "4 boxes for the 4 columns");
    assert!(
        slots.iter().zip(&feed.widths).all(|(s, w)| s.w == w + 28.),
        "each box is its widest cell plus both paddings"
    );
    assert_eq!(
        slots[0].x, 42.,
        "the first box starts after the row numbers"
    );
    assert_eq!(
        feed.cell_box(&slots[0]).0,
        56.,
        "its text starts at 56, as the board's first column"
    );
    assert!(
        feed.grid_width(460.) >= 460.,
        "the grid is at least the frame wide"
    );
    assert_eq!(
        csv::row_number(feed.table.at_record(2).unwrap().record),
        1,
        "the first body row is numbered 1"
    );
}

// big.csv: the widths the first screen put are kept on a scroll and a far jump, and the last screen is one ask.
#[test]
fn csv_feed_big_file_keeps_its_widths_and_jumps_in_one_ask() {
    let mut text = String::from("id,name\n");
    for i in 1..50_000 {
        let name = if i == 60 {
            "n".repeat(120)
        } else {
            format!("f{i:05}")
        };
        text.push_str(&format!("{i},{name}\n"));
    }
    let (mut feed, mut engine) = open(&text, ',', look(), 22);
    pump(&mut feed, &mut engine);
    assert_eq!(feed.body_rows(), 49_999, "the bar reads 49,999 rows");
    assert!(feed.frozen);
    let widths = feed.widths.clone();
    let parsed = feed.counters.lines_parsed;
    assert!(
        parsed <= 2 * 22 + 2,
        "the first screen parsed only its asked lines: {parsed}"
    );
    feed.set_view(22, 2 + 40);
    pump(&mut feed, &mut engine);
    assert_eq!(
        feed.table.at_line(61).unwrap().cells[1].len(),
        120,
        "the long name is held"
    );
    assert_eq!(feed.widths, widths, "the widths did not move on scroll");
    let asked = engine.asks.len();
    feed.set_view(22, 50_000 - 21);
    pump(&mut feed, &mut engine);
    assert_eq!(
        engine.asks.len() - asked,
        1,
        "one request draws the last screen"
    );
    assert!(
        feed.table.at_record(50_000).is_some(),
        "the last record is held"
    );
    assert_eq!(feed.widths, widths, "a far jump preserves the widths");
    assert!(
        engine.asks.iter().all(|a| a.count <= LINE_CAP),
        "no ask names more than the cap"
    );
}

#[test]
fn csv_feed_a_tall_first_screen_takes_several_capped_asks_and_never_draws_early() {
    let mut text = String::from("id,v\n");
    for i in 1..2_000 {
        text.push_str(&format!("{i},{i}\n"));
    }
    let (mut feed, mut engine) = open(&text, ',', look(), 600);
    let mut early = 0;
    while let Some(ask) = feed.ask() {
        let reply = engine.reply(&ask);
        feed.sent(reply.token);
        feed.answered(&reply);
        let table = feed.job().run();
        feed.apply(table);
        feed.measure(f64::INFINITY, &mut measure);
        let last = feed.shown().min(feed.first_to);
        if feed.first_screen() && !feed.held.contains_key(&last) {
            early += 1;
        }
    }
    assert!(
        engine.asks.len() > 1,
        "a tall first screen uses multiple capped replies"
    );
    assert_eq!(
        early, 0,
        "no reply reports ready before its first screen is held"
    );
    assert!(feed.first_screen());
}

#[test]
fn csv_feed_a_leading_bom_keeps_engine_record_starts() {
    for delim in [',', '\t'] {
        let text = format!("\u{feff}\"Full\nname\"{delim}Amount\nAlice{delim}10\n");
        let (mut feed, mut engine) = open(&text, delim, look(), 22);
        let ask = feed.ask().expect("the first screen asks for its header");
        let reply = engine.reply(&ask);
        assert_eq!(
            reply.records, 2,
            "the engine counts a multiline header as one record"
        );
        assert_eq!((reply.first, reply.first_record), (1, 1));
        assert_eq!(
            reply.lines.len(),
            3,
            "the answer holds all header and body lines"
        );
        feed.sent(reply.token);
        assert!(feed.answered(&reply));
        let table = feed.job().run();
        feed.apply(table);
        assert_eq!(
            feed.table.header,
            Some(vec!["Full\nname".into(), "Amount".into()])
        );
        let body = feed
            .table
            .at_record(2)
            .expect("the body follows the whole header");
        assert_eq!((body.line, body.record), (3, 2));
        assert_eq!(body.cells, vec!["Alice", "10"]);
    }
}
