//! A name holding bidi override or isolate controls draws in its stored order (S3-13).
use super::bench::{frame, held, open, real_cx};
use super::pane::ROW_HEIGHT;
use flea::protocol::RowFacts;
use std::rc::Rc;

#[test]
fn an_override_name_draws_without_its_controls_so_it_cannot_spoof_its_extension() {
    let mut cx = real_cx();
    let names = ["\u{202e}txt.exe", "a\u{2066}b\u{2069}c.pdf", "plain.txt"];
    let rows = names
        .iter()
        .map(|name| RowFacts {
            name: (*name).into(),
            ..RowFacts::default()
        })
        .collect();
    let window = open(&mut cx, held(rows), 900., 4. * ROW_HEIGHT);
    frame(&mut cx, window, 0.);
    let list = window
        .update(&mut cx, |view, _, _| Rc::clone(&view.list))
        .expect("the bench window is open");
    let state = list.borrow();
    let drawn: Vec<String> = state
        .visible()
        .iter()
        .filter_map(|shown| {
            shown
                .prepared
                .as_ref()
                .map(|prepared| prepared.label.clone())
        })
        .collect();
    assert_eq!(drawn, ["txt.exe", "abc.pdf", "plain.txt"]);
}
