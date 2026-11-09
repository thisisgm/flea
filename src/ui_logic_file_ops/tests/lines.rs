// Ported from tests/js/ops.js: one #[test] per oracle check, each assert marked with its oracle line.
use crate::ui_logic_file_ops::leaf;
use crate::ui_logic_file_ops::lines::{self, Outcome, Taildrop};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::transfer::Transfer;

// A transfer card with a name, a count and an index, the rest at rest.
pub(super) fn item(moving: bool, n: u64, index: u64, name: &str) -> Transfer {
    Transfer {
        moving,
        n,
        index,
        name: name.into(),
        ..Default::default()
    }
}

// A transfer card with no name yet.
pub(super) fn card(moving: bool, n: u64, index: u64) -> Transfer {
    item(moving, n, index, "")
}

// An outcome with no verdict note, the common shape of the transfer lines.
pub(super) fn outcome(ok: u64, failed: u64, skipped: u64, cancelled: bool) -> Outcome<'static> {
    Outcome {
        ok,
        failed,
        skipped,
        cancelled,
        note: "",
    }
}

// The in-flight photo sample the Transfer checks fold over.
pub(super) fn flight() -> Transfer {
    Transfer::started(12, false, 23, false).sampled(8, "panel-demo.mp4", 24_000_000, 48_000_000, 0)
}

#[test]
fn taildrop_refusal_names_subject_and_cause() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:13
    assert_eq!(
        lines::taildrop_refused(&cat.i18n, "Peer went offline"),
        "Taildrop · Peer went offline"
    );
}

#[test]
fn taildrop_sends_validated_cursor_path() {
    // oracle tests/js/ops.js:18
    assert_eq!(
        lines::taildrop_route(Some("/captured/source"), Some(false)),
        Taildrop::Send
    );
}

#[test]
fn taildrop_dispatch_says_it_is_sending() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:19
    assert_eq!(
        lines::taildrop_sending(&cat.i18n, "/captured/source", "Laptop"),
        "Sending source to Laptop."
    );
}

#[test]
fn taildrop_without_validated_path_says_out_of_date() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:22
    assert_eq!(
        lines::taildrop_stale(&cat.i18n),
        "That menu is out of date. Reopen it."
    );
}

#[test]
fn taildrop_without_validated_path_never_falls_back() {
    // oracle tests/js/ops.js:23
    assert_eq!(lines::taildrop_route(None, Some(false)), Taildrop::Stale);
}

#[test]
fn archive_success_says_what_was_verified() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:25
    assert_eq!(
        lines::archive_done_line(&cat.i18n, true),
        "Archive written."
    );
    // oracle tests/js/ops.js:26
    assert_eq!(
        lines::archive_done_line(&cat.i18n, false),
        "Extracted. The archive index could not be read, so this was not verified."
    );
}

#[test]
fn item_word_is_singular_for_one() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:30
    assert_eq!(
        format!(
            "{} / {} / {}",
            lines::items(&cat.i18n, 1),
            lines::items(&cat.i18n, 2),
            lines::items(&cat.i18n, 0)
        ),
        "1 item / 2 items / 0 items"
    );
}

#[test]
fn progress_line_reads_copying_with_name() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:35
    assert_eq!(
        lines::progress_line(&cat.i18n, &item(false, 5, 1, "photo.heic")),
        "Copying 2 of 5 · photo.heic"
    );
    // oracle tests/js/ops.js:38
    assert_eq!(
        lines::progress_line(&cat.i18n, &item(true, 5, 1, "photo.heic")),
        "Moving 2 of 5 · photo.heic"
    );
    // oracle tests/js/ops.js:42
    assert_eq!(
        lines::progress_line(&cat.i18n, &item(false, 2, 0, "")),
        "Copying 1 of 2"
    );
}

#[test]
fn transfer_done_names_its_undo_when_something_landed() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:46
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            outcome(2, 0, 0, false)
        ),
        "Copied 2 items · z undoes"
    );
    // oracle tests/js/ops.js:49
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(true, 1, 0),
            outcome(1, 0, 0, false)
        ),
        "Moved 1 item · z undoes"
    );
    // oracle tests/js/ops.js:52
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            outcome(1, 1, 0, false)
        ),
        "Copied 1 of 2 · 1 failed · z undoes"
    );
}

#[test]
fn transfer_done_offers_no_undo_when_nothing_landed() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:56
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            outcome(0, 2, 0, false)
        ),
        "Copied 0 of 2 · 2 failed"
    );
    // oracle tests/js/ops.js:59
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            outcome(0, 0, 2, true)
        ),
        "Copied 0 of 2 · 2 skipped · cancelled"
    );
    // oracle tests/js/ops.js:62
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 5, 0),
            outcome(3, 0, 2, true)
        ),
        "Copied 3 of 5 · 2 skipped · cancelled · z undoes"
    );
}

#[test]
fn transfer_done_keeps_the_full_count_inventory() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:65
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 5, 0),
            outcome(2, 1, 2, false)
        ),
        "Copied 2 of 5 · 1 failed · 2 skipped · z undoes"
    );
}

#[test]
fn transfer_failure_keeps_backend_cause() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:68
    assert_eq!(
        lines::transfer_failure(&cat.i18n, true, "photo.heic", "disk full"),
        "Move failed: photo.heic · disk full"
    );
}

#[test]
fn retry_selection_makes_no_claim_without_matches() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:71
    assert_eq!(lines::retry_selection_line(&cat.i18n, &[]), "");
}

#[test]
fn retry_selection_names_one_match_by_filename() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:72
    assert_eq!(
        lines::retry_selection_line(&cat.i18n, &["/source/c.txt"]),
        "c.txt selected for retry"
    );
}

#[test]
fn retry_selection_counts_many_matches() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:74
    assert_eq!(
        lines::retry_selection_line(&cat.i18n, &["/source/c.txt", "/source/d.txt"]),
        "2 items selected for retry"
    );
}

#[test]
fn undone_names_the_operation_it_reversed() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:78
    assert_eq!(
        format!(
            "{} / {}",
            lines::undone(&cat.i18n, "rename"),
            lines::undone(&cat.i18n, "move")
        ),
        "Undid the rename / Undid the move"
    );
    // oracle tests/js/ops.js:83
    assert_eq!(lines::undone(&cat.i18n, "mkdir"), "Removed the new folder.");
}

#[test]
fn copied_line_names_its_paste_hint() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:87
    assert_eq!(
        format!(
            "{} / {}",
            lines::copied(&cat.i18n, "default", 2, true),
            lines::copied(&cat.i18n, "default", 1, false)
        ),
        "Cut 2 items · p pastes / Copied 1 item · p pastes"
    );
}

#[test]
fn leaf_is_the_part_after_the_last_separator() {
    // oracle tests/js/ops.js:91
    assert_eq!(leaf("/home/gm/photo copy.jpg"), "photo copy.jpg");
    // oracle tests/js/ops.js:94
    assert_eq!(leaf("bare.txt"), "bare.txt");
}

#[test]
fn target_paths_keeps_only_rows_the_window_holds() {
    let rows = ["f0", "f1", "f2", "f3", "f4"];
    let indices: Vec<u64> = (0..40).collect();
    // oracle tests/js/ops.js:145
    assert_eq!(
        format!(
            "{} of {}",
            lines::target_paths(0, &rows, &indices).len(),
            indices.len()
        ),
        "5 of 40"
    );
}

#[test]
fn selection_keeps_every_selected_index() {
    let selected: Vec<u64> = (0..40).collect();
    // oracle tests/js/ops.js:156
    assert_eq!(
        lines::target_indices(&selected, 0, true).len().to_string(),
        "40"
    );
    // oracle tests/js/ops.js:167
    assert_eq!(
        lines::target_indices(&selected, 0, true).len().to_string(),
        "40"
    );
}
