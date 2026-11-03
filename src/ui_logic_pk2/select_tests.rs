//! Double-click and activation rules, ported from Picker.doubleAction/activates.
use super::select::*;
use super::testkit::english;

#[test]
fn same_tap_names_the_file_never_another_row() {
    assert!(same_tap("/a/b.txt", "/a/b.txt"));
    assert!(!same_tap("/a/b.txt", "/a/c.txt"));
    assert!(!same_tap("/a/b.txt", ""));
}

#[test]
fn double_clicks_keep_their_mode_rules() {
    assert_eq!(double_action("open", false, false, true, false, true), DoubleAction::Open);
    assert_eq!(double_action("open", false, false, false, false, true), DoubleAction::MarkAccept);
    assert_eq!(double_action("open", false, false, false, true, true), DoubleAction::Accept);
    assert_eq!(double_action("open", true, false, false, false, true), DoubleAction::Mark);
    assert_eq!(double_action("open", true, false, false, true, true), DoubleAction::Mark);
    // A rebuilt list between taps never retargets.
    assert_eq!(double_action("open", false, false, false, false, false), DoubleAction::None);
    assert_eq!(double_action("save", false, false, false, false, true), DoubleAction::None);
    assert_eq!(double_action("open", false, true, false, false, true), DoubleAction::None);
    assert_eq!(double_action("savefiles", false, false, false, false, true), DoubleAction::None);
}

#[test]
fn return_still_activates_where_rename_does_not_own_it() {
    assert!(activates("open", 0, 1, 2));
    assert!(activates("pageForward", 0, 1, 2));
    assert!(!activates("rename", 3, 1, 2));
    assert!(activates("rename", 1, 1, 2));
    assert!(activates("rename", 2, 1, 2));
}

#[test]
fn the_skipped_selection_uses_the_singular_and_plural_templates() {
    let i18n = english();
    assert_eq!(
        skipped_note(&i18n, 2, 3, &[("/a/gone.txt", "deleted")]),
        "Selected 2 of 3. 1 was left out: /a/gone.txt (deleted)."
    );
    assert_eq!(
        skipped_note(&i18n, 2, 5, &[("/a/gone.txt", "deleted"), ("/a/other.txt", "moved"), ("/a/more.txt", "gone")]),
        "Selected 2 of 5. 3 were left out: /a/gone.txt (deleted) and 2 more."
    );
    assert_eq!(skipped_note(&i18n, 2, 2, &[]), "");
}

#[test]
fn the_skipped_note_is_one_cardinal_plural_key_not_two_keys() {
    let i18n = english();
    assert_eq!(
        i18n.tr("picker.browser.skipped_one"),
        "picker.browser.skipped_one",
        "no second singular key exists"
    );
    assert_ne!(i18n.trn("picker.browser.skipped", 1), i18n.trn("picker.browser.skipped", 2));
}
