// Review round 3 proofs: widths and alignment take every first-screen row, a late column costs its first screen.
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
        !feed.frozen,
        "the first reply holds only part of the first screen"
    );
    assert!(
        feed.widths.is_empty(),
        "no column is measured before the first screen is whole"
    );
    drive(&mut feed, &mut engine, f64::INFINITY);
    assert!(feed.frozen, "the whole first screen froze the widths");
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
    assert!(feed.frozen, "the first screen froze");
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

// v0.3.12 freezes [22.0] [true] for the far column; measuring it over every held row reads a 40-character width.
#[test]
fn rev_r3_far_column_is_measured_over_the_first_screen() {
    let (mut feed, _, _) = late_setup();
    feed.measure(f64::INFINITY, &mut measure);
    assert_eq!(feed.widths.len(), 40, "every column is kept");
    assert_eq!(
        feed.widths[39], 22.0,
        "the far column takes the first screen's width"
    );
    assert!(
        feed.rights[39],
        "the far column keeps the first screen's alignment"
    );
}

// The deterministic per-frame cost: a column the frame reaches late is measured over the first screen's rows, never
// over the rows the scroll has held.
#[test]
fn rev_r3_far_column_measure_cost_follows_the_first_screen() {
    let (mut feed, reached, before) = late_setup();
    let first = feed
        .table
        .rows
        .iter()
        .filter(|row| row.record <= feed.first_to)
        .count();
    feed.measure(f64::INFINITY, &mut measure);
    assert_eq!(
        feed.counters.measured - before,
        (40 - reached) * (first + 1),
        "one frame to the right measures its columns over the first screen only"
    );
}
