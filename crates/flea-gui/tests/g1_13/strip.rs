//! The filter strip's host pieces: the slot height, the caret rule and the select-all guard.
use flea_gui::{
    list::Metrics,
    strips::filter_host::{self, CloseRows, CloseSet},
};
use gpui::px;
use std::{cell::Cell, collections::HashSet};

// The counting seam: a HashSet-backed close lookup that counts every lookup the selection makes.
struct Counting {
    set: HashSet<usize>,
    lookups: Cell<usize>,
}

impl Counting {
    fn new(rows: &[usize]) -> Self {
        Self {
            set: rows.iter().copied().collect(),
            lookups: Cell::new(0),
        }
    }
}

impl CloseRows for Counting {
    fn has(&self, row: usize) -> bool {
        self.lookups.set(self.lookups.get() + 1);
        self.set.contains(&row)
    }
}

#[test]
fn the_caret_stands_only_while_the_line_holds_the_keyboard() {
    assert!(filter_host::caret(true, true));
    assert!(!filter_host::caret(true, false));
    assert!(!filter_host::caret(false, true));
    assert!(!filter_host::caret(false, false));
}

#[test]
fn select_all_marks_exact_rows_and_never_close_ones() {
    assert_eq!(
        filter_host::select_exact(&[3, 7], &CloseSet::new(&[5])),
        vec![3, 7]
    );
    // The close fallback is what the view draws when nothing exact matched; Ctrl+A marks none of it.
    assert!(filter_host::select_exact(&[5, 6], &CloseSet::new(&[5, 6])).is_empty());
    assert!(filter_host::select_exact(&[], &CloseSet::new(&[1, 2, 3])).is_empty());
}

#[test]
fn ctrl_a_over_a_close_only_answer_touches_no_rows() {
    // Nothing exact matched, so Ctrl+A returns before probing the close rows at all.
    let close = Counting::new(&[1, 2, 3]);
    let out = filter_host::select_exact(&[], &close);
    assert!(out.is_empty());
    assert_eq!(close.lookups.get(), 0);
}

#[test]
fn ctrl_a_over_drawn_close_rows_costs_one_lookup_per_row() {
    // A one-letter typo in the 100k bench folder draws 100k close rows; Ctrl+A probes each drawn row once.
    let rows: Vec<usize> = (0..100_000).collect();
    let close = Counting::new(&rows);
    let out = filter_host::select_exact(&rows, &close);
    assert!(out.is_empty());
    assert_eq!(close.lookups.get(), rows.len());
}

#[test]
fn ctrl_a_over_drawn_close_rows_through_the_product_set() {
    // The product path: 100k drawn close rows through CloseSet, timed; 1 s leaves 100x room over 10 ms.
    let rows: Vec<usize> = (0..100_000).collect();
    let close = CloseSet::new(&rows);
    let start = std::time::Instant::now();
    let out = filter_host::select_exact(&rows, &close);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    assert!(out.is_empty());
    assert!(ms < 1000.0, "drawn-rows Ctrl+A took {ms:.1} ms");
}

#[test]
fn the_strip_height_is_the_chrome_metric_at_the_live_body_size() {
    assert_eq!(
        filter_host::height(px(12.)),
        Metrics::at(px(12.)).header,
        "stock body 12 gives 23"
    );
    assert_eq!(
        filter_host::height(px(14.)),
        Metrics::at(px(14.)).header,
        "body 14 gives 27"
    );
    assert_eq!(filter_host::height(px(14.)), 27.);
    assert_eq!(filter_host::height(px(12.)), 23.);
}
