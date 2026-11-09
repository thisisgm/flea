// Ports tests/js/uistate.js (Flea 0.3.12) lines 367-416 and records the QML-driven checks at 418-487 as BLOCKED.
use super::*;
use crate::jsondoc::{parse, Json};

fn j(text: &str) -> Json {
    parse(text).expect("fixture parses")
}

fn code(value: i64) -> Answer {
    Answer { code: Some(value), text: None }
}

fn text(value: &str) -> Answer {
    Answer { code: None, text: Some(value.to_string()) }
}

fn both(value: i64, body: &str) -> Answer {
    Answer { code: Some(value), text: Some(body.to_string()) }
}

const BOGUS_COLUMNS: &str = r#"{"columns":["name","size","bogus"]}"#;
const BOGUS_WITH_DENSITY: &str = r#"{"columns":["name","size","bogus"],"density":"normal"}"#;

#[test]
fn a_refused_key_drops_out_of_what_is_owed_and_the_state_heals() {
    let owed_bogus = j(r#"{"columns":["name","size","bogus"],"density":"compact"}"#);
    let settled_defaults = j(r#"{"columns":["name","size","date"],"density":"compact"}"#);
    let pruned = drop_invalid(&owed_bogus, BOGUS_COLUMNS);
    assert!(pruned.get("columns").is_none());
    assert_eq!(invalid_keys(BOGUS_COLUMNS), vec!["columns"]);
    let reverted = reverted_state(&j(r#"{"columns":["name","size","bogus"]}"#), BOGUS_COLUMNS, &settled_defaults);
    assert_eq!(at_columns(&reverted), j(r#"["name","size","date"]"#));
}

#[test]
fn a_valid_key_beside_a_refusal_survives_and_only_the_refused_key_is_named() {
    let two_key = drop_invalid(&j(r#"{"columns":["name","size","bogus"],"density":"normal"}"#), BOGUS_WITH_DENSITY);
    assert_eq!(two_key.get("density"), Some(&Json::Str("normal".to_string())));
    assert!(two_key.get("columns").is_none());
    assert_eq!(invalid_keys(BOGUS_WITH_DENSITY), vec!["columns"]);
}

#[test]
fn a_newer_owed_value_the_refused_writer_never_carried_survives_the_prune() {
    let newer = drop_invalid(&j(r#"{"columns":["name"],"density":"normal"}"#), BOGUS_WITH_DENSITY);
    assert_eq!(at_columns(&newer), j(r#"["name"]"#));
}

#[test]
fn the_revert_heals_only_the_refused_key_and_leaves_a_valid_key() {
    let settled_defaults = j(r#"{"columns":["name","size","date"],"density":"compact"}"#);
    let revert_two = reverted_state(
        &j(r#"{"columns":["name","size","bogus"],"density":"normal"}"#),
        BOGUS_WITH_DENSITY,
        &settled_defaults,
    );
    assert_eq!(at_columns(&revert_two), j(r#"["name","size","date"]"#));
    assert_eq!(revert_two.get("density"), Some(&Json::Str("normal".to_string())));
}

#[test]
fn a_newer_value_for_the_refused_key_survives_the_revert() {
    let settled_defaults = j(r#"{"columns":["name","size","date"],"density":"compact"}"#);
    let revert_newer = reverted_state(&j(r#"{"columns":["name"],"density":"normal"}"#), BOGUS_WITH_DENSITY, &settled_defaults);
    assert_eq!(at_columns(&revert_newer), j(r#"["name"]"#));
}

#[test]
fn a_failed_settle_still_drops_the_refused_key_and_keeps_the_valid_one() {
    let failed = drop_invalid(&j(r#"{"columns":["name","size","bogus"],"density":"normal"}"#), BOGUS_WITH_DENSITY);
    assert!(failed.get("columns").is_none());
    assert_eq!(failed.get("density"), Some(&Json::Str("normal".to_string())));
}

// Only a whole answer (both halves present) is read. Empty text still counts as an answer.
#[test]
fn half_an_answer_is_not_whole_and_both_halves_are_in_either_order() {
    assert!(!whole(&code(0)));
    assert!(whole(&landed(code(0), text("{}"))));
    assert!(whole(&landed(text("{}"), code(0))));
}

#[test]
fn a_settler_that_never_ran_is_told_from_a_real_exit() {
    assert!(never_ran(&Answer::default(), false));
    assert!(!never_ran(&code(0), false));
}

#[test]
fn a_prune_behind_a_running_settle_queues_and_an_idle_settler_starts_it() {
    assert_eq!(prune_ask(true, r#"{"columns":[]}"#).queue, r#"{"columns":[]}"#);
    assert_eq!(prune_ask(true, r#"{"columns":[]}"#).start, "");
    assert_eq!(prune_ask(false, r#"{"columns":[]}"#).start, r#"{"columns":[]}"#);
}

#[test]
fn a_half_landed_settle_still_counts_as_running_so_a_prune_queues_behind_it() {
    assert!(settle_busy(false, "apply", &code(0)));
    assert!(settle_busy(false, "apply", &text("{}")));
    assert!(!settle_busy(false, "apply", &both(0, "{}")));
    assert!(!settle_busy(false, "", &Answer::default()));
    let half = settle_busy(false, "apply", &code(0));
    assert_eq!(prune_ask(half, r#"{"columns":[]}"#).queue, r#"{"columns":[]}"#);
    let text_only_half = settle_busy(false, "apply", &text("{}"));
    assert_eq!(prune_ask(text_only_half, r#"{"columns":[]}"#).start, "");
}

#[test]
fn a_queued_prune_spends_before_a_dirty_reread() {
    assert_eq!(settle_next(true, r#"{"columns":[]}"#), "prune");
    assert_eq!(settle_next(true, ""), "apply");
    assert_eq!(settle_next(false, ""), "idle");
}

fn at_columns(doc: &Json) -> Json {
    doc.get("columns").cloned().unwrap_or(Json::Null)
}
