// Review round 3 proofs, and S9G-8: widths and alignment start from every first-screen row, then widths widen to the held cells.
use super::engine::*;
use super::*;

// One ask-reply-parse round with the frame's reach, as the view's layout measures what it reaches.
fn drive(feed: &mut Feed, engine: &mut Engine, limit: f64) {
    let mut sent = 0;
    while let Some(ask) = feed.ask() {
        sent += 1;
        let reply = engine.reply(&ask);
        feed.sent(reply.token);
        assert!(feed.answered(&reply), "the ask in flight takes its reply");
        let table = feed.job().run();
        feed.apply(table);
        feed.measure(limit, &mut measure);
        assert!(sent < 500, "the asks end");
    }
}

// Three-line records, 25 bodies: the first reply's line-counted ask holds only part of the first screen, and body 20
// carries the wide name and the text code the partial screen never sees.
fn multiline() -> String {
    let mut text = String::from("name,code,city\n");
    for i in 1..=25 {
        if i == 20 {
            text.push_str("abcdefghijklmnopqrstuvwxy1234,n/a,\"gg\nhh\nii\"\n");
        } else {
            text.push_str(&format!("{i},100,\"aa\nbb\ncc\"\n"));
        }
    }
    text
}

// A column measured over the partial first screen would read [29.0, 29.0, 58.0] with the code column right-aligned;
// v0.3.12 measures every column over the first screen's rows on every parse until the freeze.
#[test]
fn rev_r3_first_screen_widths_take_every_first_screen_row() {
    let (mut feed, mut engine) = open(&multiline(), ',', look(), 24);
    let ask = feed.ask().expect("the first screen is asked");
    let reply = engine.reply(&ask);
    feed.sent(reply.token);
    assert!(feed.answered(&reply), "the ask in flight takes its reply");
    let table = feed.job().run();
    feed.apply(table);
    feed.measure(f64::INFINITY, &mut measure);
    assert!(
        !feed.frozen(),
        "the first reply holds only part of the first screen"
    );
    assert!(
        feed.widths.is_empty(),
        "no column is measured before the first screen is whole"
    );
    drive(&mut feed, &mut engine, f64::INFINITY);
    assert!(feed.frozen(), "the whole first screen froze the widths");
    assert_eq!(
        feed.widths,
        vec![209.0, 29.0, 58.0],
        "the 29-character name sets the first column"
    );
    assert_eq!(
        feed.rights,
        vec![false, false, false],
        "the text code keeps its column left"
    );
}

// Forty columns, 2,500 single-line rows: the last column is short and numeric on the first screen and 40 characters
// of text after it, and the 1100 px frame never reaches it until the reader scrolls right.
fn sideways() -> String {
    const COLS: usize = 40;
    let mut text = (0..COLS)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    for r in 0..2500 {
        let row = (0..COLS)
            .map(|c| {
                if c == COLS - 1 && r >= 48 {
                    "y".repeat(40)
                } else {
                    ((r + c) % 10).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        text.push_str(&row);
        text.push('\n');
    }
    text
}

// Serves the first screen through a frame's reach, scrolls down with a wide margin until the rows are held, and
// leaves the late columns for one frame to the right: the reached count, the held rows and the measured count.
fn late_setup() -> (Feed, usize, usize) {
    let (mut feed, mut engine) = open(&sideways(), ',', look(), 24);
    drive(&mut feed, &mut engine, 1100.0);
    assert!(feed.frozen(), "the first screen froze");
    let reached = feed.widths.len();
    assert!(
        (1..40).contains(&reached),
        "the frame reaches some columns but not all"
    );
    feed.margin = 200;
    feed.set_view(24, 2000);
    drive(&mut feed, &mut engine, 1100.0);
    assert!(
        feed.table.rows.len() >= 2400,
        "the scroll down holds its rows"
    );
    let measured = feed.counters.measured;
    (feed, reached, measured)
}

// Measure calls, one a frame, until no held cell is left to widen by; the count of calls taken.
fn widen_all(feed: &mut Feed) -> usize {
    let mut frames = 1;
    feed.measure(f64::INFINITY, &mut measure);
    while feed.widening() {
        frames += 1;
        assert!(frames < 10_000, "the widening ends");
        feed.measure(f64::INFINITY, &mut measure);
    }
    frames
}

// S9G-8 (board ql-office-sheet): the far column widens from 22 px to its 40-character held cells, its alignment kept.
#[test]
fn s9g8_far_column_widens_to_its_widest_held_cell() {
    let (mut feed, _, _) = late_setup();
    let frames = widen_all(&mut feed);
    assert!(frames > 1, "the held rows take several frames: {frames}");
    assert_eq!(feed.widths.len(), 40, "every column is kept");
    assert_eq!(
        feed.widths[39], 288.0,
        "the far column takes its widest held cell"
    );
    assert!(
        feed.rights[39],
        "the far column keeps the first screen's alignment"
    );
}

// Cost: a frame to the right takes the late columns' first screen and at most WIDEN_CELLS held cells; each held cell once in all.
#[test]
fn s9g8_far_column_cost_is_the_first_screen_and_a_bounded_slice() {
    let (mut feed, reached, before) = late_setup();
    let first = feed
        .table
        .rows
        .iter()
        .filter(|row| row.record <= feed.first_to)
        .count();
    feed.measure(f64::INFINITY, &mut measure);
    let frame = feed.counters.measured - before;
    assert_eq!(
        frame,
        (40 - reached) * (first + 1) + WIDEN_CELLS,
        "one frame to the right measures the first screen's cells and one whole slice"
    );
    // The next frame resumes at the row the last one stopped on, never at the first row.
    let resumed = feed.widen_from();
    assert!(resumed > 0 && feed.widening(), "the slice stopped part way");
    feed.measure(f64::INFINITY, &mut measure);
    assert!(
        feed.widen_from() > resumed,
        "the next slice walks on from row {resumed}"
    );
    widen_all(&mut feed);
    let columns = feed.widths.len();
    assert!(
        feed.table
            .rows
            .iter()
            .all(|row| feed.folded.get(&row.record) == Some(&columns)),
        "every held row is walked"
    );
    assert_eq!(
        feed.counters.measured,
        40 * (1 + feed.table.rows.len()),
        "each head and held cell is measured once"
    );
}

// S9G-8's long.csv: values past the first screen are wider than its own, and a scroll to the end widens all three.
#[test]
fn s9g8_a_scroll_past_the_first_screen_widens_to_the_wider_values() {
    let text: String = std::iter::once("a,b,c\n".to_string())
        .chain((0..600).map(|i| format!("{i},{},{}\n", i * 2, i * 3)))
        .collect();
    let (mut feed, mut engine) = open(&text, ',', look(), 24);
    pump(&mut feed, &mut engine);
    assert_eq!(
        feed.widths,
        vec![15., 15., 22.],
        "the first screen's two- and three-digit values"
    );
    feed.set_view(24, 601 - 23);
    pump(&mut feed, &mut engine);
    widen_all(&mut feed);
    assert_eq!(
        feed.widths,
        vec![22., 29., 29.],
        "599, 1198 and 1797 set the columns"
    );
}

// A table whose first screen is not whole measures nothing, so it never keeps the view asking for frames.
#[test]
fn s9g8_a_table_not_yet_frozen_is_not_widening() {
    let (mut feed, mut engine) = open(&multiline(), ',', look(), 24);
    let ask = feed.ask().expect("the first screen is asked");
    let reply = engine.reply(&ask);
    feed.sent(reply.token);
    assert!(feed.answered(&reply), "the ask in flight takes its reply");
    let table = feed.job().run();
    feed.apply(table);
    assert!(
        !feed.frozen() && !feed.widening(),
        "a partial first screen asks for no widening frame"
    );
}

#[test]
fn csv_engine_parse_holds_asks_until_it_lands_then_resumes_the_partial_screen() {
    let (mut feed, mut engine) = open(&multiline(), ',', look(), 24);
    assert_eq!(feed.parse_phase, Parse::Engine);
    let reply = engine.reply(&feed.ask().unwrap());
    feed.sent(reply.token);
    assert!(feed.answered(&reply));
    assert_eq!(feed.parse_phase, Parse::ParsingEngine);
    assert!(feed.ask().is_none(), "no ask races its parse");
    let table = feed.job().run();
    assert!(feed.ask().is_none(), "running a job does not publish it");
    feed.apply(table);
    assert_eq!(feed.parse_phase, Parse::Engine);
    assert_eq!(feed.column_phase, Columns::Waiting);
    assert!(feed.ask().is_some(), "the partial first screen resumes");
}

#[test]
fn csv_plain_parse_measure_and_reset_return_to_the_engine_phase() {
    let mut feed = Feed::new(Some(','), look());
    feed.set_view(24, 2);
    feed.answer_plain("a,b\n1,2\n3,4\n");
    assert_eq!(feed.parse_phase, Parse::ParsingPlain);
    assert!(feed.ask().is_none());
    let table = feed.job().run();
    feed.apply(table);
    assert_eq!(feed.parse_phase, Parse::Plain);
    assert_eq!(feed.column_phase, Columns::Widening(0));
    feed.measure(f64::INFINITY, &mut measure);
    assert_eq!(feed.column_phase, Columns::Measured);
    assert!(!feed.wants_measure(f64::INFINITY));
    assert!(feed.ask().is_none(), "plain text never asks the engine");
    feed.reset(Some(','));
    assert_eq!(feed.parse_phase, Parse::Engine);
    assert_eq!(feed.column_phase, Columns::Waiting);
    assert!(feed.widths.is_empty());
    assert!(feed.ask().is_some(), "reset accepts an engine source again");
}
