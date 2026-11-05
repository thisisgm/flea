use super::Lines;
use crate::list::pane::{Pane, ROW_HEIGHT};
use flea::protocol::{Reply, RowFacts};

fn listed(rows: usize) -> Pane {
    let mut pane = Pane::new("/files".into(), 620.);
    let path = pane.path().to_owned();
    let facts = (0..rows)
        .map(|i| RowFacts {
            name: format!("{i:06}"),
            ..RowFacts::default()
        })
        .collect();
    pane.receive(Reply::Listed { path, total: rows }).unwrap();
    pane.receive(Reply::Rows {
        start: 0,
        rows: facts,
        listing: 1,
    })
    .unwrap();
    pane
}

#[test]
fn a_filtered_row_is_remembered_on_the_line_it_was_drawn_on() {
    let mut pane = listed(320);
    pane.cursor = 100;
    let mut lines = Lines {
        shown: Some([5, 50, 100, 200].into()),
        ..Lines::default()
    };
    // Third of the drawn rows with the filtered list unscrolled: back unfiltered, row 100 stands third again.
    lines.remember(&pane);
    lines.shown = None;
    lines.restore(&mut pane, "000100");
    assert_eq!(pane.scroll_y, 98. * ROW_HEIGHT);
}

#[test]
fn an_unfiltered_row_keeps_its_line_on_screen() {
    let mut pane = listed(320);
    pane.cursor = 100;
    pane.scroll_to(95. * ROW_HEIGHT);
    let mut lines = Lines::default();
    lines.remember(&pane);
    pane.scroll_to(0.);
    lines.restore(&mut pane, "000100");
    assert_eq!(pane.scroll_y, 95. * ROW_HEIGHT);
}

#[test]
fn a_row_landed_on_under_a_standing_filter_takes_its_line_among_the_drawn_rows() {
    let mut pane = listed(320);
    pane.cursor = 100;
    let mut lines = Lines {
        shown: Some([5, 50, 100, 200].into()),
        ..Lines::default()
    };
    lines.remember(&pane);
    // Same folder, filter still standing: row 100 is the third drawn row, so the scroll stays at the top.
    pane.scroll_to(40. * ROW_HEIGHT);
    lines.restore(&mut pane, "000100");
    assert_eq!(pane.scroll_y, 0.);
    // A row the filter hides is left where it is.
    lines.shown = Some([5, 50].into());
    pane.scroll_to(40. * ROW_HEIGHT);
    lines.restore(&mut pane, "000100");
    assert_eq!(pane.scroll_y, 40. * ROW_HEIGHT);
}
