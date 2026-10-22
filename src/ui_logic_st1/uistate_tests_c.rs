// Ports tests/js/uistate.js (Flea 0.3.12) lines 279-365: window keys, applying another window's save, and the parse guard.
use super::*;
use crate::jsondoc::{parse, render, Json};

fn j(text: &str) -> Json {
    parse(text).expect("fixture parses")
}

// The value at a key path, or None where any step is missing.
fn at<'a>(doc: &'a Json, path: &[&str]) -> Option<&'a Json> {
    path.iter().try_fold(doc, |node, key| node.get(key))
}

#[test]
fn window_keys_are_never_taken_from_the_file() {
    for key in ["view", "pickerView", "columnWidths", "dual", "lastPath", "lastTabs"] {
        assert!(is_window_key(key), "{key} is a window key");
    }
    assert!(!is_window_key("hidden"));
    assert!(!is_window_key("display"));
}

#[test]
fn a_changed_preference_applies_while_window_keys_stay_put() {
    let other = j(r#"{"hidden":false,"density":"compact","view":"list","lastPath":"/a","display":{"textSize":{"mode":"system"}},"sort":{"key":"name","reverse":false},"places":{"showUnmounted":true,"favourites":[{"label":"Old","path":"/old"}]}}"#);
    let changed_file = r#"{"hidden":true,"density":"normal","view":"grid","lastPath":"/b","display":{"textSize":{"mode":16}},"sort":{"key":"size","reverse":false},"places":{"showUnmounted":false,"favourites":[]},"stateVersion":3}"#;
    let applied = apply_external(&other, &j("{}"), changed_file);
    assert_eq!(at(&applied.state, &["hidden"]), Some(&Json::Bool(true)));
    assert_eq!(at(&applied.state, &["display", "textSize", "mode"]), Some(&j("16")));
    assert_eq!(at(&applied.state, &["sort", "key"]), Some(&Json::Str("name".to_string())));
    assert_eq!(at(&applied.state, &["places", "showUnmounted"]), Some(&Json::Bool(false)));
    assert_eq!(at(&applied.state, &["view"]), Some(&Json::Str("list".to_string())));
    assert_eq!(at(&applied.state, &["lastPath"]), Some(&Json::Str("/a".to_string())));
    assert_eq!(at(&applied.state, &["places", "favourites"]), Some(&j(r#"[{"label":"Old","path":"/old"}]"#)));
}

#[test]
fn an_owed_key_keeps_the_window_value_and_a_key_beside_it_applies() {
    let own_state = j(r#"{"hidden":false,"density":"compact"}"#);
    let kept = apply_external(&own_state, &j(r#"{"hidden":false}"#), r#"{"hidden":true,"density":"normal"}"#);
    assert_eq!(at(&kept.state, &["hidden"]), Some(&Json::Bool(false)));
    assert_eq!(at(&kept.state, &["density"]), Some(&Json::Str("normal".to_string())));
}

#[test]
fn a_file_holding_nothing_new_or_unreadable_moves_nothing() {
    let other = j(r#"{"hidden":false,"density":"compact","view":"list","lastPath":"/a","display":{"textSize":{"mode":"system"}},"sort":{"key":"name","reverse":false},"places":{"showUnmounted":true,"favourites":[{"label":"Old","path":"/old"}]}}"#);
    let same_file = render(&other);
    assert!(!apply_external(&other, &j("{}"), &same_file).changed);
    assert!(!apply_external(&other, &j("{}"), "{").changed);
    assert!(!apply_external(&other, &j("{}"), "[1,2]").changed);
    assert!(!apply_external(&other, &j("{}"), "").changed);
}

#[test]
fn an_owed_leaf_keeps_its_value_while_its_siblings_apply() {
    let racing = j(r#"{"hidden":false,"display":{"textSize":{"mode":16},"hyprlandIcons":false}}"#);
    let raced = apply_external(
        &racing,
        &j(r#"{"display":{"textSize":{"mode":16}}}"#),
        r#"{"hidden":true,"display":{"textSize":{"mode":20},"hyprlandIcons":true}}"#,
    );
    assert_eq!(at(&raced.state, &["display", "textSize", "mode"]), Some(&j("16")));
    assert_eq!(at(&raced.state, &["display", "hyprlandIcons"]), Some(&Json::Bool(true)));
    assert_eq!(at(&raced.state, &["hidden"]), Some(&Json::Bool(true)));
}

#[test]
fn an_owed_map_entry_keeps_the_window_value_and_another_entry_applies() {
    let maps = j(r#"{"folderSorts":{"/a":{"key":"size","reverse":true}}}"#);
    let mapped = apply_external(
        &maps,
        &j(r#"{"folderSorts":{"/a":{"key":"size","reverse":true}}}"#),
        r#"{"folderSorts":{"/a":{"key":"name","reverse":false},"/b":{"key":"name","reverse":false}}}"#,
    );
    assert_eq!(at(&mapped.state, &["folderSorts", "/a", "key"]), Some(&Json::Str("size".to_string())));
    assert_eq!(at(&mapped.state, &["folderSorts", "/b", "key"]), Some(&Json::Str("name".to_string())));
}

#[test]
fn a_key_a_newer_flea_wrote_is_kept_verbatim() {
    let applied = apply_external(&j("{}"), &j("{}"), r#"{"aKeyThisBuildHasNeverHeardOf":true}"#);
    assert_eq!(at(&applied.state, &["aKeyThisBuildHasNeverHeardOf"]), Some(&Json::Bool(true)));
}

#[test]
fn sort_is_a_window_key_so_another_windows_sort_never_lands() {
    assert!(is_window_key("sort"));
    let held = j(r#"{"sort":{"key":"name","reverse":false}}"#);
    let applied = apply_external(&held, &j("{}"), r#"{"sort":{"key":"size","reverse":true},"hidden":true}"#);
    assert_eq!(at(&applied.state, &["sort"]), Some(&j(r#"{"key":"name","reverse":false}"#)));
    assert_eq!(at(&applied.state, &["hidden"]), Some(&Json::Bool(true)));
}

#[test]
fn previewfacts_is_a_window_key_so_another_windows_facts_never_land() {
    assert!(is_window_key("previewFacts"));
    let applied = apply_external(&j(r#"{"previewFacts":"compact"}"#), &j("{}"), r#"{"previewFacts":"full","hidden":true}"#);
    assert_eq!(at(&applied.state, &["previewFacts"]), Some(&Json::Str("compact".to_string())));
    assert_eq!(at(&applied.state, &["hidden"]), Some(&Json::Bool(true)));
}

#[test]
fn rail_column_and_grid_zoom_are_window_leaves_and_a_places_switch_is_not() {
    assert!(is_window_leaf("places", "rail"));
    assert!(is_window_leaf("preview", "column"));
    assert!(is_window_leaf("preview", "thumbSize"));
    assert!(!is_window_leaf("places", "showUnmounted"));
}

#[test]
fn another_windows_rail_never_lands_while_its_places_switch_does() {
    let held = j(r#"{"places":{"rail":"hidden","showUnmounted":true,"favourites":[]}}"#);
    let applied = apply_external(&held, &j("{}"), r#"{"places":{"rail":"shown","showUnmounted":false,"favourites":[]}}"#);
    assert_eq!(at(&applied.state, &["places", "rail"]), Some(&Json::Str("hidden".to_string())));
    assert_eq!(at(&applied.state, &["places", "showUnmounted"]), Some(&Json::Bool(false)));
}

#[test]
fn another_windows_preview_column_and_zoom_never_land_while_its_load_switch_does() {
    let held = j(r#"{"preview":{"column":false,"thumbSize":"large","loadOn":"automatic"}}"#);
    let applied = apply_external(&held, &j("{}"), r#"{"preview":{"column":true,"thumbSize":"small","loadOn":"manual"}}"#);
    assert_eq!(at(&applied.state, &["preview", "column"]), Some(&Json::Bool(false)));
    assert_eq!(at(&applied.state, &["preview", "thumbSize"]), Some(&Json::Str("large".to_string())));
    assert_eq!(at(&applied.state, &["preview", "loadOn"]), Some(&Json::Str("manual".to_string())));
}

#[test]
fn only_a_parsed_object_counts_as_a_settled_document() {
    assert!(!parses_as_object(""));
    assert!(!parses_as_object("{"));
    assert!(parses_as_object(r#"{"hidden":true}"#));
}

#[test]
fn only_a_schema_invalid_patch_is_pruned() {
    assert!(is_patch_invalid(r#"{"columns":["name","size","bogus"]}"#));
    assert!(!is_patch_invalid(r#"{"columns":["name","size","date"]}"#));
    assert!(is_patch_invalid(r#"{"places":null}"#));
}
