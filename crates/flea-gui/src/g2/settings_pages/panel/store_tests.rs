use crate::g2::settings_pages::{seams::MemoryStore, tests::panel};
use flea::jsondoc::parse;

const BEFORE: &str =
    r#"{"places":{"favourites":[{"label":"One","path":"/one"},{"label":"Two","path":"/two"}]}}"#;
const AFTER: &str = r#"{"places":{"favourites":[{"label":"One","path":"/one"}]}}"#;

#[test]
fn fxuish2b_tail_external_favourite_removal_returns_its_cursor_to_add() {
    let mut view = panel(BEFORE);
    view.show_section("places");
    view.cursor = view
        .rows
        .iter()
        .position(|row| row.id == "favourite:1")
        .unwrap();
    view.replace_store(Box::new(MemoryStore::new(parse(AFTER).unwrap())));
    assert_eq!(view.rows[view.cursor].action, "addFavourite");
}

#[test]
fn fxuish2b_tail_external_favourite_removal_keeps_the_control_below_it_focused() {
    let mut view = panel(BEFORE);
    view.show_section("places");
    view.cursor = view
        .rows
        .iter()
        .position(|row| row.id == "places.sidebarWidth")
        .unwrap();
    view.replace_store(Box::new(MemoryStore::new(parse(AFTER).unwrap())));
    assert_eq!(view.rows[view.cursor].id, "places.sidebarWidth");
}

#[test]
fn fxuish2b_tail_external_favourite_removal_keeps_the_first_control_after_the_favourites() {
    let mut view = panel(BEFORE);
    view.show_section("places");
    view.cursor = view
        .rows
        .iter()
        .position(|row| row.id == "places.showHome")
        .unwrap();
    view.replace_store(Box::new(MemoryStore::new(parse(AFTER).unwrap())));
    assert_eq!(view.rows[view.cursor].id, "places.showHome");
}

#[test]
fn fxuish2b_tail_an_unrelated_refresh_keeps_the_favourite_cursor() {
    let mut view = panel(BEFORE);
    view.show_section("places");
    let index = view
        .rows
        .iter()
        .position(|row| row.id == "favourite:1")
        .unwrap();
    view.cursor = index;
    view.replace_store(Box::new(MemoryStore::new(parse(BEFORE).unwrap())));
    assert_eq!(view.cursor, index);
}
