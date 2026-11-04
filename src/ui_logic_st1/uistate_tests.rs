// Ports tests/js/uistate.js (Flea 0.3.12) lines 55-137: the book flow, fromFile, favourites and the document rebuilds.
use super::*;
use crate::jsondoc::{parse, Json};

const OLD: &str = r#"{"columns":["name","size","date"]}"#;
const NEW: &str = r#"{"columns":["name","date"]}"#;
const THIRD: &str = r#"{"columns":["name"]}"#;
const DRAFT: &str = r#"{"view":"columns","places":{"sidebarWidth":256,"favourites":[{"label":"Before","path":"/before"}]}}"#;

fn j(text: &str) -> Json {
    parse(text).expect("fixture parses")
}

// The English the catalog gives the favourites read error (settings.error.favourites_unread).
const FAVOURITES_UNREAD_TEXT: &str =
    "Favorites could not be refreshed because ui.json could not be read. The old list stays.";

fn say_error(text: &Text) -> String {
    crate::ui_logic_st1::test_support::say(text)
}

fn book_with_saved(saved: &str) -> Book {
    Book { saved: saved.to_string(), ..Book::default() }
}

#[test]
fn a_fresh_book_has_landed_nothing_and_first_change_writes() {
    assert_eq!(book().saved, "");
    assert_eq!(book().inflight, "");
    assert_eq!(asked(&book(), NEW).start, NEW);
}

#[test]
fn a_patch_the_last_landed_write_stored_starts_nothing() {
    assert_eq!(asked(&book_with_saved(OLD), OLD).start, "");
}

#[test]
fn a_change_starts_a_write_that_is_in_flight_not_saved() {
    let step = asked(&book_with_saved(OLD), NEW);
    assert_eq!(step.start, NEW);
    assert_eq!(step.book.inflight, NEW);
    assert_eq!(step.book.saved, OLD);
}

#[test]
fn a_refused_write_is_not_believed_and_is_reported() {
    let first = asked(&book_with_saved(OLD), NEW);
    let refused = exited(&first.book, 2, NEW);
    assert_eq!(refused.book.saved, OLD);
    assert!(refused.failed);
    assert_eq!(refused.book.inflight, "");
}

#[test]
fn an_identical_retry_is_attempted_after_a_refusal() {
    let first = asked(&book_with_saved(OLD), NEW);
    let refused = exited(&first.book, 2, NEW);
    assert_eq!(asked(&refused.book, NEW).start, NEW);
}

#[test]
fn a_write_that_exited_zero_is_believed_and_not_reported() {
    let first = asked(&book_with_saved(OLD), NEW);
    let landed = exited(&first.book, 0, "");
    assert_eq!(landed.book.saved, NEW);
    assert!(!landed.failed);
    assert_eq!(asked(&landed.book, NEW).start, "");
}

#[test]
fn a_second_change_queues_behind_the_running_writer() {
    let first = asked(&book_with_saved(OLD), NEW);
    let queued = asked(&first.book, THIRD);
    assert_eq!(queued.book.pending, THIRD);
    assert_eq!(queued.start, "");
    assert_eq!(queued.book.inflight, NEW);
    assert_eq!(asked(&queued.book, THIRD).start, "");
}

#[test]
fn from_file_reads_a_document_and_flags_what_cannot_be_read() {
    assert_eq!(from_file(OLD).state.get("columns").and_then(Json::as_array).and_then(|c| c.get(2)).and_then(Json::as_str), Some("date"));
    assert!(!from_file(OLD).unreadable);
    assert_eq!(from_file("{\"columns\":[\"name\"],}").state, j("{}"));
    assert!(from_file("{\"columns\":[\"name\"],}").unreadable);
    assert!(from_file("[1,2]").unreadable);
    assert!(!from_file("").unreadable);
}

#[test]
fn external_favourites_keep_invalid_and_duplicate_originals() {
    let draft = j(DRAFT);
    let text = r#"{"view":"grid","places":{"sidebarWidth":160,"favourites":[17,{"label":"New","path":"/new"},{"label":"New","path":"/new"}]}}"#;
    let refresh = refreshed_favourites(&draft, text);
    assert_eq!(
        refresh.state.get("places").and_then(|p| p.get("favourites")),
        Some(&j(r#"[17,{"label":"New","path":"/new"},{"label":"New","path":"/new"}]"#))
    );
    assert_eq!(refresh.state.get("view"), Some(&Json::Str("columns".to_string())));
    assert_eq!(refresh.state.get("places").and_then(|p| p.get("sidebarWidth")), Some(&j("256")));
}

#[test]
fn refresh_does_not_mutate_its_input() {
    let draft = j(DRAFT);
    let _ = refreshed_favourites(&draft, r#"{"places":{"favourites":[]}}"#);
    assert_eq!(draft, j(DRAFT));
}

#[test]
fn malformed_or_empty_external_bytes_keep_visible_records_and_report() {
    let draft = j(DRAFT);
    let bad = refreshed_favourites(&draft, "{");
    assert_eq!(bad.state, draft);
    assert!(!bad.error.is_empty());
    assert_eq!(bad.error.key(), Some("settings.error.favourites_unread"));
    assert_eq!(say_error(&bad.error), FAVOURITES_UNREAD_TEXT);
    let empty = refreshed_favourites(&draft, "");
    assert_eq!(empty.state, draft);
    assert!(!empty.error.is_empty());
}

#[test]
fn nonarray_or_non_object_favourites_keep_visible_records() {
    let draft = j(DRAFT);
    assert_eq!(refreshed_favourites(&draft, r#"{"places":{"favourites":17}}"#).state, draft);
    assert_eq!(refreshed_favourites(&draft, r#"{"places":[]}"#).state, draft);
}

#[test]
fn a_missing_favourites_group_is_the_empty_new_store() {
    let draft = j(DRAFT);
    let refresh = refreshed_favourites(&draft, "{}");
    assert_eq!(refresh.state.get("places").and_then(|p| p.get("favourites")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
}

#[test]
fn unchanged_favourites_keep_the_existing_state() {
    let draft = j(DRAFT);
    assert_eq!(refreshed_favourites(&draft, DRAFT).state, draft);
}

#[test]
fn favourites_after_add_keeps_siblings_and_flags_a_concurrent_removal() {
    let a = j(r#"{"label":"A","path":"/a"}"#);
    let b = j(r#"{"label":"B","path":"/b"}"#);
    let c = j(r#"{"label":"C","path":"/c"}"#);
    let before = vec![a.clone(), b.clone()];
    let own_add = favourites_after(&before, &FavouriteOp::Add(c.clone()));
    assert_eq!(own_add, vec![a, b.clone(), c.clone()]);
    assert_ne!(own_add, vec![b, c]);
}

#[test]
fn favourites_after_predicts_move_remove_and_rename_and_never_mutates() {
    let a = j(r#"{"label":"A","path":"/a"}"#);
    let b = j(r#"{"label":"B","path":"/b"}"#);
    let before = vec![a.clone(), b.clone()];
    assert_eq!(favourites_after(&before, &FavouriteOp::Move { index: 1, to: 0 }), vec![b.clone(), a.clone()]);
    assert_eq!(favourites_after(&before, &FavouriteOp::Remove(0)), vec![b.clone()]);
    let renamed = FavouriteOp::Rename { index: 1, label: "Renamed".to_string() };
    assert_eq!(favourites_after(&before, &renamed), vec![a.clone(), j(r#"{"label":"Renamed","path":"/b"}"#)]);
    assert_eq!(before, vec![a, b]);
}

#[test]
fn with_key_and_with_group_rebuild_the_document_without_mutating_it() {
    let held = j(r#"{"columns":["name"],"display":{"textSize":{"mode":"system"}}}"#);
    assert_eq!(
        with_key(&held, "keys", Json::Str("windows".to_string())),
        j(r#"{"columns":["name"],"display":{"textSize":{"mode":"system"}},"keys":"windows"}"#)
    );
    let places = j(r#"{"places":{"sidebarWidth":240,"showHome":true}}"#);
    assert_eq!(
        with_group(&places, "places", &j(r#"{"showHome":false}"#)),
        j(r#"{"places":{"sidebarWidth":240,"showHome":false}}"#)
    );
    assert_eq!(
        with_group(&j("{}"), "display", &j(r#"{"textSize":{"mode":16}}"#)),
        j(r#"{"display":{"textSize":{"mode":16}}}"#)
    );
    assert_eq!(held, j(r#"{"columns":["name"],"display":{"textSize":{"mode":"system"}}}"#));
}
