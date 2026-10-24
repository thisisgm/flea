//! Save-name validity, stem selection, joint wrap and the save check order.
use super::save::*;
use super::testkit::english;

#[test]
fn names_take_the_product_rule_character_for_character() {
    assert!(valid_name("a.txt"));
    assert!(!valid_name(""));
    assert!(!valid_name("."));
    assert!(!valid_name(".."));
    assert!(!valid_name("a/b"));
    assert!(!valid_name("a\0b"));
}

#[test]
fn joints_give_the_wrapper_a_break_after_every_segment_mark() {
    assert_eq!(joint_name("a-b_c.d"), "a-\u{200b}b_\u{200b}c.\u{200b}d");
    assert_eq!(joint_name("plain"), "plain");
    // P56: a long already-exists name carries a joint per segment mark.
    let long = "very-long_release.candidate_v2-final.zip";
    assert_eq!(
        joint_name(long).matches('\u{200b}').count(),
        long.chars().filter(|c| *c == '.' || *c == '_' || *c == '-').count()
    );
}

#[test]
fn the_save_check_runs_empty_then_refused_then_location_then_probe() {
    assert_eq!(check_save_name("", None, false), SaveCheck::NeedName);
    assert_eq!(check_save_name("a/b", None, false), SaveCheck::Refused);
    assert_eq!(check_save_name("a.txt", Some(false), true), SaveCheck::LocationChanged);
    assert_eq!(check_save_name("a.txt", Some(true), false), SaveCheck::Exists);
    assert_eq!(check_save_name("a.txt", Some(false), false), SaveCheck::Ready);
    // An unprobed name never writes.
    assert_eq!(check_save_name("a.txt", None, false), SaveCheck::Probe);
    // A moved location outranks a taken name.
    assert_eq!(check_save_name("a.txt", Some(true), true), SaveCheck::LocationChanged);
    // A refused name stays refused even where it also exists or moved.
    assert_eq!(check_save_name("..", Some(true), true), SaveCheck::Refused);
}

#[test]
fn an_empty_name_is_asked_for_before_a_location_or_a_name_is_checked() {
    assert_eq!(check_save_name("", Some(true), true), SaveCheck::NeedName);
}

#[test]
fn the_refused_line_keeps_the_name_and_the_rule_whole() {
    let i18n = english();
    assert_eq!(save_refused(&i18n, "a/b"), "a/b cannot be used: a name cannot be empty, . or .., or contain /");
    // A placeholder inside the name is an argument, never a template.
    assert_eq!(
        save_refused(&i18n, "a%1b"),
        "a%1b cannot be used: a name cannot be empty, . or .., or contain /"
    );
}

#[test]
fn collision_sentences_fill_the_jointed_name_as_one_argument() {
    let i18n = english();
    let jointed = joint_name("report.txt");
    assert_eq!(collision_short(&i18n, &jointed), "report.\u{200b}txt already exists here.");
    assert_eq!(
        collision_review(&i18n, &jointed),
        "report.\u{200b}txt already exists here. Review it before you continue."
    );
}

#[test]
fn failure_and_location_sentences_are_whole_catalog_strings() {
    let i18n = english();
    assert_eq!(list_failed(&i18n, "failed"), "This folder could not be listed.");
    assert_eq!(list_failed(&i18n, "dead"), "Flea stopped responding, so this folder could not be listed.");
    assert_eq!(list_failed(&i18n, "unstarted"), "This folder could not be listed. Restart Flea.");
    assert_eq!(save_stopped(&i18n), "Flea stopped responding. Cancel and reopen this request.");
    assert_eq!(save_moved(&i18n), "The save location changed; review it again.");
}
