//! tests/js/clipmarks.js: a copied row carries copy and a cut row scissors, following the clipboard; an empty clipboard costs nothing.
use super::{is_empty, row_path, Marks};

fn home() -> String {
    "/home/gm/Pictures/phone".into()
}

fn copied() -> Vec<String> {
    vec![
        "/home/gm/Pictures/phone/IMG_4121.jpg".into(),
        "/home/gm/Pictures/phone/IMG_4122.jpg".into(),
        "/home/gm/Pictures/phone/IMG_4123.jpg".into(),
    ]
}

#[test]
fn an_empty_clipboard_marks_nothing() {
    let mut marks = Marks::new();
    let mut calls = 0;
    let empty: Vec<String> = Vec::new();
    assert_eq!(marks.mark_for_row(&home(), "IMG_4121.jpg", 1, &empty, false, &mut calls), "");
    assert_eq!(marks.mark_for_row(&home(), "IMG_4121.jpg", 1, &empty, false, &mut calls), "");
    assert!(!marks.cached());
    assert_eq!(calls, 2);
}

#[test]
fn an_unnamed_row_marks_nothing() {
    let mut marks = Marks::new();
    let mut calls = 0;
    let full = copied();
    assert_eq!(marks.mark_for_row(&home(), "", 1, &full, false, &mut calls), "");
}

#[test]
fn copied_and_cut_rows_carry_their_mark() {
    let mut marks = Marks::new();
    let mut calls = 0;
    let full = copied();
    marks.set_for(1, &full, false);
    assert_eq!(marks.len(), 3);
    assert!(marks.mark_for("/home/gm/Pictures/phone/IMG_4122.jpg") == "copy");
    assert_eq!(marks.mark_for_row(&home(), "IMG_4121.jpg", 1, &full, false, &mut calls), "copy");
    assert_eq!(marks.mark_for_row(&home(), "IMG_4124.jpg", 1, &full, false, &mut calls), "");
    assert_eq!(
        marks.mark_for_row("/home/gm/Pictures/other", "IMG_4121.jpg", 1, &full, false, &mut calls),
        ""
    );
    let cut = vec!["/home/gm/Pictures/phone/IMG_4121.jpg".to_string()];
    assert_eq!(marks.mark_for_row(&home(), "IMG_4121.jpg", 2, &cut, true, &mut calls), "scissors");
}

#[test]
fn a_new_selection_clears_the_old_marks_at_once() {
    let mut marks = Marks::new();
    let mut calls = 0;
    let cut = vec!["/home/gm/Pictures/phone/IMG_4121.jpg".to_string()];
    let after_copy = vec!["/home/gm/Pictures/phone/IMG_4124.jpg".to_string()];
    assert_eq!(marks.mark_for_row(&home(), "IMG_4121.jpg", 1, &cut, true, &mut calls), "scissors");
    let pair = format!(
        "{}/{}",
        marks.mark_for_row(&home(), "IMG_4121.jpg", 2, &after_copy, false, &mut calls),
        marks.mark_for_row(&home(), "IMG_4124.jpg", 2, &after_copy, false, &mut calls)
    );
    assert_eq!(pair, "/copy");
    let after_cut = vec!["/home/gm/Pictures/phone/IMG_4124.jpg".to_string()];
    let pair = format!(
        "{}/{}",
        marks.mark_for_row(&home(), "IMG_4121.jpg", 3, &after_cut, true, &mut calls),
        marks.mark_for_row(&home(), "IMG_4124.jpg", 3, &after_cut, true, &mut calls)
    );
    assert_eq!(pair, "/scissors");
}

#[test]
fn only_a_cut_spent_by_a_cut_paste_empties() {
    let cut = vec!["/home/gm/Pictures/phone/IMG_4121.jpg".to_string()];
    let full = copied();
    assert_eq!(super::marks_spent(&cut, true, true), None);
    assert_eq!(super::marks_spent(&full, false, false), Some((full.clone(), false)));
    assert_eq!(super::marks_spent(&cut, true, false), Some((cut.clone(), true)));
    assert_eq!(super::marks_spent(&full, false, true), Some((full.clone(), false)));
}

#[test]
fn a_row_path_joins_the_pane_base() {
    assert_eq!(row_path(&home(), "IMG_4121.jpg"), "/home/gm/Pictures/phone/IMG_4121.jpg");
    assert_eq!(row_path("/", "boot"), "/boot");
    assert_eq!(row_path(&home(), ""), "");
}

#[test]
fn emptying_drops_the_cached_lookup() {
    let mut marks = Marks::new();
    let mut calls = 0;
    let full = copied();
    marks.set_for(1, &full, false);
    marks.mark_for_row(&home(), "IMG_4121.jpg", 2, &[], false, &mut calls);
    assert!(!marks.cached());
    assert_eq!(marks.len(), 0);
    marks.set_for(3, &full, false);
    marks.mark_for_row(&home(), "IMG_4121.jpg", 4, &[], false, &mut calls);
    assert!(!marks.cached());
}

#[test]
fn a_copied_search_result_marks_by_its_search_base() {
    let mut marks = Marks::new();
    let mut calls = 0;
    let found = vec!["/home/gm/Pictures/trip/IMG_1.jpg".to_string()];
    assert_eq!(
        marks.mark_for_row("/home/gm/Pictures/phone", "trip/IMG_1.jpg", 1, &found, false, &mut calls),
        ""
    );
    assert_eq!(marks.mark_for_row("/home/gm/Pictures", "trip/IMG_1.jpg", 1, &found, false, &mut calls), "copy");
}

#[test]
fn emptiness_reads_through_the_hoisted_helper() {
    assert!(is_empty(&[]));
    assert!(!is_empty(&copied()));
    assert!(!is_empty(&["/home/gm/Pictures/phone/IMG_4121.jpg".to_string()]));
}

#[test]
fn the_lookup_builds_once_per_clipboard() {
    let mut marks = Marks::new();
    let full = copied();
    marks.set_for(1, &full, false);
    marks.set_for(1, &full, false);
    assert_eq!(marks.len(), 3);
    assert_eq!(marks.revision(), Some(1));
}
