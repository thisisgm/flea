// Ports tests/js/uistate.js (Flea 0.3.12) lines 140-277: what a window owes, what a landed write clears, and the interleave.
use super::*;
use crate::jsondoc::{parse, render, Json};

const OLD: &str = r#"{"columns":["name","size","date"]}"#;
const NEW: &str = r#"{"columns":["name","date"]}"#;
const THIRD: &str = r#"{"columns":["name"]}"#;

fn j(text: &str) -> Json {
    parse(text).expect("fixture parses")
}

#[test]
fn a_text_size_change_owes_the_text_size_alone() {
    let owed = with_group(&j("{}"), "display", &j(r#"{"textSize":{"mode":16}}"#));
    assert_eq!(owed, j(r#"{"display":{"textSize":{"mode":16}}}"#));
    assert!(!render(&owed).contains("keys"));
    assert!(!render(&owed).contains("columns"));
    assert!(!render(&owed).contains("hidden"));
    assert_eq!(render(&j("{}")).trim_end(), "{}");
}

#[test]
fn a_second_change_joins_the_patch_already_owed() {
    let owed = with_group(&j("{}"), "display", &j(r#"{"textSize":{"mode":16}}"#));
    assert_eq!(
        with_key(&owed, "keys", Json::Str("windows".to_string())),
        j(r#"{"display":{"textSize":{"mode":16}},"keys":"windows"}"#)
    );
    assert_eq!(
        with_group(&owed, "display", &j(r#"{"hidden":["paste"]}"#)),
        j(r#"{"display":{"textSize":{"mode":16},"hidden":["paste"]}}"#)
    );
}

#[test]
fn the_document_keeps_a_newer_flea_sub_key_that_the_patch_never_carries() {
    let read = j(r#"{"display":{"textSize":{"mode":"system"},"aKeyThisBuildHasNeverHeardOf":true}}"#);
    assert_eq!(
        with_group(&read, "display", &j(r#"{"textSize":{"mode":16}}"#)),
        j(r#"{"display":{"textSize":{"mode":16},"aKeyThisBuildHasNeverHeardOf":true}}"#)
    );
    let owed = with_group(&j("{}"), "display", &j(r#"{"textSize":{"mode":16}}"#));
    assert!(!render(&owed).contains("NeverHeardOf"));
}

#[test]
fn a_folder_sort_owes_its_entry_alone_and_a_landed_entry_clears() {
    let entry_owed = with_group(&j("{}"), "folderSorts", &j(r#"{"/a":{"key":"size","reverse":true}}"#));
    assert_eq!(entry_owed, j(r#"{"folderSorts":{"/a":{"key":"size","reverse":true}}}"#));
    let two_owed = with_group(&entry_owed, "folderSorts", &j(r#"{"/b":{"key":"name","reverse":false}}"#));
    assert_eq!(
        two_owed,
        j(r#"{"folderSorts":{"/a":{"key":"size","reverse":true},"/b":{"key":"name","reverse":false}}}"#)
    );
    assert_eq!(
        acknowledged(&two_owed, r#"{"folderSorts":{"/a":{"key":"size","reverse":true}}}"#),
        j(r#"{"folderSorts":{"/b":{"key":"name","reverse":false}}}"#)
    );
}

#[test]
fn a_forget_owes_its_null_entry_and_a_landed_forget_clears() {
    let forget_owed = with_group(&j("{}"), "folderSorts", &j(r#"{"/a":null}"#));
    assert_eq!(forget_owed, j(r#"{"folderSorts":{"/a":null}}"#));
    assert_eq!(acknowledged(&forget_owed, r#"{"folderSorts":{"/a":null}}"#), j("{}"));
    assert_eq!(
        with_group(&j("{}"), "columnWidths", &j(r#"{"size":120}"#)),
        j(r#"{"columnWidths":{"size":120}}"#)
    );
}

#[test]
fn a_half_landed_last_tabs_keeps_the_whole_owed_strip() {
    let owed = j(r#"{"lastTabs":{"paths":["/a","/b"],"index":0}}"#);
    assert_eq!(
        acknowledged(&owed, r#"{"lastTabs":{"paths":["/a","/a"],"index":0}}"#),
        j(r#"{"lastTabs":{"paths":["/a","/b"],"index":0}}"#)
    );
}

#[test]
fn a_fully_landed_last_tabs_clears() {
    let owed = j(r#"{"lastTabs":{"paths":["/a","/b"],"index":0}}"#);
    assert_eq!(acknowledged(&owed, r#"{"lastTabs":{"paths":["/a","/b"],"index":0}}"#), j("{}"));
}

#[test]
fn a_queued_writer_starts_when_the_writer_exits_and_a_refusal_still_runs_it() {
    let held = Book { saved: OLD.to_string(), ..Book::default() };
    let first = asked(&held, NEW);
    let queued = asked(&first.book, THIRD);
    let drained = exited(&queued.book, 0, THIRD);
    assert_eq!(drained.start, THIRD);
    assert_eq!(drained.book.saved, NEW);
    let refused = exited(&queued.book, 2, THIRD);
    assert_eq!(refused.start, THIRD);
    assert!(refused.failed);
}

#[test]
fn a_writer_with_nothing_behind_or_nothing_owed_starts_nothing() {
    let held = Book { saved: OLD.to_string(), ..Book::default() };
    let first = asked(&held, NEW);
    assert_eq!(exited(&first.book, 0, "{}").start, "");
    let queued = asked(&first.book, THIRD);
    assert_eq!(exited(&queued.book, 0, "{}").start, "");
    assert_eq!(exited(&queued.book, 0, "{}").book.inflight, "");
}

#[test]
fn a_landed_setting_is_no_longer_owed_and_an_unsent_one_stays() {
    assert_eq!(acknowledged(&j(r#"{"keys":"windows"}"#), r#"{"keys":"windows"}"#), j("{}"));
    assert_eq!(
        acknowledged(&j(r#"{"keys":"windows","columns":["name"]}"#), r#"{"keys":"windows"}"#),
        j(r#"{"columns":["name"]}"#)
    );
}

#[test]
fn an_array_setting_is_compared_whole() {
    assert_eq!(acknowledged(&j(r#"{"columns":["name","size"]}"#), r#"{"columns":["name","size"]}"#), j("{}"));
    assert_eq!(acknowledged(&j(r#"{"columns":["name"]}"#), r#"{"columns":["name","size"]}"#), j(r#"{"columns":["name"]}"#));
}

#[test]
fn a_landed_leaf_clears_out_of_its_group_and_the_group_goes_with_its_last_leaf() {
    let two_leaves = j(r#"{"display":{"textSize":{"mode":16},"hidden":["paste"]}}"#);
    assert_eq!(
        acknowledged(&two_leaves, r#"{"display":{"textSize":{"mode":16}}}"#),
        j(r#"{"display":{"hidden":["paste"]}}"#)
    );
    assert_eq!(acknowledged(&j(r#"{"display":{"textSize":{"mode":16}}}"#), r#"{"display":{"textSize":{"mode":16}}}"#), j("{}"));
    assert_eq!(two_leaves, j(r#"{"display":{"textSize":{"mode":16},"hidden":["paste"]}}"#));
}

#[test]
fn bytes_that_are_not_a_patch_clear_nothing() {
    assert_eq!(acknowledged(&j(r#"{"keys":"windows"}"#), "not json"), j(r#"{"keys":"windows"}"#));
}

#[test]
fn a_newer_value_for_a_landed_setting_or_leaf_is_still_owed() {
    assert_eq!(acknowledged(&j(r#"{"keys":"mac"}"#), r#"{"keys":"windows"}"#), j(r#"{"keys":"mac"}"#));
    assert_eq!(
        acknowledged(&j(r#"{"display":{"textSize":{"mode":20}}}"#), r#"{"display":{"textSize":{"mode":16}}}"#),
        j(r#"{"display":{"textSize":{"mode":20}}}"#)
    );
}

// The interleave the book exists for: the preset lands while the text size waits behind it.
#[test]
fn the_queued_writer_starts_with_the_text_size_once_the_preset_landed() {
    let keys_text = r#"{"keys":"windows"}"#;
    let also_size_text = r#"{"keys":"windows","display":{"textSize":{"mode":16}}}"#;
    let own_keys = with_key(&j("{}"), "keys", Json::Str("windows".to_string()));
    let also_size = with_group(&own_keys, "display", &j(r#"{"textSize":{"mode":16}}"#));
    let first = asked(&book(), keys_text);
    assert_eq!(first.start, keys_text);
    let behind = asked(&first.book, also_size_text);
    assert_eq!(behind.book.pending, also_size_text);
    let still_owes = acknowledged(&also_size, &behind.book.inflight);
    assert_eq!(still_owes, j(r#"{"display":{"textSize":{"mode":16}}}"#));
    let drained_after = exited(&behind.book, 0, r#"{"display":{"textSize":{"mode":16}}}"#);
    assert_eq!(drained_after.start, r#"{"display":{"textSize":{"mode":16}}}"#);
    assert!(!drained_after.start.contains("keys"));
}

// A refused writer keeps its own setting in the patch behind it, and the refusal is still reported.
#[test]
fn a_refused_writer_leaves_its_setting_in_the_patch_behind_it() {
    let keys_text = r#"{"keys":"windows"}"#;
    let also_size_text = r#"{"keys":"windows","display":{"textSize":{"mode":16}}}"#;
    let own_keys = with_key(&j("{}"), "keys", Json::Str("windows".to_string()));
    let also_size = with_group(&own_keys, "display", &j(r#"{"textSize":{"mode":16}}"#));
    let first = asked(&book(), keys_text);
    let behind = asked(&first.book, also_size_text);
    let refused_under = exited(&behind.book, 2, also_size_text);
    assert_eq!(refused_under.start, also_size_text);
    assert!(refused_under.failed);
    assert_eq!(also_size, j(r#"{"keys":"windows","display":{"textSize":{"mode":16}}}"#));
}
