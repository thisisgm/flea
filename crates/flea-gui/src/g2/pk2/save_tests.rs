//! Save field tests: empty checks, review stem, footer sentences.
use super::route::{located, take_row, Located};
use super::save::SaveField;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_pk2::{self as core, SaveCheck};

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pk2-gui");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

#[test]
fn an_empty_accept_asks_for_a_name_and_checks_nothing_else() {
    let i18n = english();
    let mut field = SaveField::new("");
    assert_eq!(field.accept(), SaveCheck::NeedName);
    assert_eq!(
        field.words(&i18n).error,
        "",
        "the field stays quiet: the footer carries the ask"
    );
    assert_eq!(core::need_name(&i18n), "Name the file before saving it");
}

#[test]
fn the_request_name_is_checked_until_something_is_typed() {
    let i18n = english();
    let mut field = SaveField::new("bad/name");
    assert!(
        !field.refused(),
        "an empty field has no typed name to refuse"
    );
    assert_eq!(
        field.accept(),
        SaveCheck::NeedName,
        "an empty field asks for a name"
    );
    assert_eq!(
        field.words(&i18n).error,
        "",
        "no error line before anything is typed"
    );
    field.edit("ok.txt");
    assert!(!field.refused(), "typing replaces the request's name");
    assert_eq!(field.asked_name(), "ok.txt");
}

#[test]
fn a_refused_name_wins_over_existence_and_error_line_names_it() {
    let i18n = english();
    let mut field = SaveField::new("");
    field.edit("a/b");
    let key = field.probe_key();
    field.set_exists(&key, true);
    assert_eq!(field.check(), SaveCheck::Refused);
    let words = field.words(&i18n);
    assert_eq!(
        words.error,
        "a/b cannot be used: a name cannot be empty, . or .., or contain /"
    );
    assert!(!words.review);
}

#[test]
fn a_collision_opens_the_review_and_asks_for_the_stem() {
    let i18n = english();
    let mut field = SaveField::new("");
    field.edit("report.txt");
    let key = field.probe_key();
    field.set_exists(&key, true);
    assert_eq!(field.accept(), SaveCheck::Exists);
    assert_eq!(field.stem_requests, 1);
    assert_eq!(field.stem_end(), 6);
    let words = field.words(&i18n);
    assert!(words.review);
    assert_eq!(words.error, "report.\u{200b}txt already exists here.");
    // Editing closes the review again.
    field.edit("report2.txt");
    assert!(!field.words(&i18n).review);
}

#[test]
fn backend_death_says_so_through_the_field_with_the_field_sentence() {
    let i18n = english();
    let mut field = SaveField::new("");
    field.edit("a.txt");
    field.note_backend_gone();
    let error = field.words(&i18n).error;
    assert_eq!(error, "Flea stopped responding.");
    assert_ne!(
        error, "Flea stopped responding. Cancel and reopen this request.",
        "the footer sentence stays in the footer"
    );
}

#[test]
fn a_moved_location_asks_for_the_review_again() {
    let i18n = english();
    let mut field = SaveField::new("");
    field.edit("a.txt");
    field.set_location_changed(true);
    assert_eq!(field.check(), SaveCheck::LocationChanged);
    assert_eq!(
        field.words(&i18n).error,
        "The save location changed; review it again."
    );
}

#[test]
fn a_save_reply_refusal_is_shown_verbatim_and_a_new_edit_clears_it() {
    let i18n = english();
    let mut field = SaveField::new("");
    field.edit("a.txt");
    field.set_save_error("Permission denied");
    assert_eq!(field.words(&i18n).error, "Permission denied");
    field.edit("b.txt");
    assert_eq!(field.words(&i18n).error, "");
}

#[test]
fn a_long_collision_name_carries_wrap_joints() {
    let mut field = SaveField::new("");
    field.edit("very-long_release.candidate_v2-final.zip");
    assert!(field.jointed().contains('\u{200b}'));
}

// PickerWindow.qml:338: a name with no probe answer probes; it never submits.
#[test]
fn an_unprobed_name_is_never_ready() {
    let mut field = SaveField::new("");
    field.edit("taken.txt");
    assert_ne!(
        field.accept(),
        SaveCheck::Ready,
        "no probe answered, yet the name may be written"
    );
}

// A stale answer reports untaken so the host probes again.
#[test]
fn a_stale_probe_answer_reports_untaken() {
    let mut field = SaveField::new("");
    field.set_folder("/d");
    field.edit("a.txt");
    let key = field.probe_key();
    field.edit("taken.txt");
    assert!(!field.set_exists(&key, false));
    assert_eq!(field.accept(), SaveCheck::Probe);
    let fresh = field.probe_key();
    assert!(field.set_exists(&fresh, false));
    assert_eq!(field.accept(), SaveCheck::Ready);
}

// A probe for one folder never answers another folder's review.
#[test]
fn a_probe_for_one_folder_never_answers_another() {
    let mut field = SaveField::new("");
    field.set_folder("/a");
    field.edit("taken.txt");
    let key = field.probe_key();
    field.set_folder("/b");
    assert!(!field.set_exists(&key, false));
    assert_eq!(field.accept(), SaveCheck::Probe);
}

// PickerWindow.qml:129-131: the review keys on [path, name].
#[test]
fn a_stale_existence_never_reviews_a_new_name() {
    let mut field = SaveField::new("");
    field.edit("a.txt");
    let key = field.probe_key();
    field.edit("b.txt");
    field.set_exists(&key, false);
    assert_eq!(
        field.accept(),
        SaveCheck::Probe,
        "b.txt was never probed, yet it opens the collision review"
    );
}

// A valid request name seeds the field; Return probes it.
#[test]
fn the_request_name_seeds_the_field() {
    let mut field = SaveField::new("report.txt");
    assert_eq!(field.asked_name(), "report.txt");
    assert_eq!(field.name(), "report.txt");
    assert_ne!(field.accept(), SaveCheck::NeedName);
}

// Review rev-g2-pk2-r3 proofs: keyed probe, saveName as text.
#[test]
fn rev_r3_a_late_probe_answer_never_readies_a_newer_name() {
    let mut field = SaveField::new("");
    field.set_folder("/d");
    field.edit("a.txt");
    // The window probes a.txt; then the user types a taken name.
    let key = field.probe_key();
    field.edit("taken.txt");
    // The a.txt answer (absent) lands now.
    field.set_exists(&key, false);
    assert_eq!(
        field.accept(),
        SaveCheck::Probe,
        "the answer for a.txt let taken.txt be written without its own probe"
    );
}

#[test]
fn rev_r3_an_emptied_field_asks_for_a_name() {
    let mut field = SaveField::new("report.txt");
    field.edit("");
    assert_eq!(
        field.name(),
        "",
        "the emptied field still holds the request name"
    );
    assert_eq!(
        field.accept(),
        SaveCheck::NeedName,
        "Return on an emptied field goes on to save the request name"
    );
}

// Helper coverage after a name transfer; rule 10's Enter transfer needs the pk1 host.
#[test]
fn a_transferred_file_name_probes_and_reviews_its_collision() {
    assert_eq!(
        located("/d/a.txt", "/d", "/d", "/d/a.txt", 3),
        Located::Row(3)
    );
    assert_eq!(
        take_row("/d/a.txt", 3, true),
        Some(3),
        "a held file row may be taken"
    );
    let mut field = SaveField::new("untitled");
    field.set_folder("/d");
    field.edit("a.txt");
    let key = field.probe_key();
    assert_eq!((key.folder.as_str(), key.name.as_str()), ("/d", "a.txt"));
    assert!(
        field.set_exists(&key, true),
        "the probe finds the name taken"
    );
    assert_eq!(
        field.accept(),
        SaveCheck::Exists,
        "the transferred name opens the review"
    );
    assert_eq!(field.stem_requests, 1, "the stem is selected again");
    assert_eq!(field.stem_end(), 1, "a.txt selects a");
    assert!(
        field.words(&english()).review,
        "the review carries the taken line"
    );
}
