// Ported from tests/js/opstrash.js: one #[test] per oracle check, each assert marked with its oracle line.
use crate::ui_logic_file_ops::lines;
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::trash::{self, Scope};

#[test]
fn trash_reads_as_the_canvas_draws_it() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:22
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 4, 0, ""),
        "Moved 4 items to Trash · z undoes"
    );
}

#[test]
fn trash_that_failed_outright_offers_no_undo() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:25
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 0, 1, ""),
        "That item could not be moved to Trash."
    );
}

#[test]
fn partly_failed_trash_reports_both_halves() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:28
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 3, 1, ""),
        "Moved 3 items to Trash, 1 failed · z undoes"
    );
}

#[test]
fn undoing_a_trash_says_where_it_came_back_from() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:31
    assert_eq!(lines::undone(&cat.i18n, "trash"), "Put it back from Trash.");
}

#[test]
fn trash_scope_line_is_singular_for_one_and_plural_for_two() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:48
    assert_eq!(
        format!(
            "{} // {}",
            trash::scope_line(&cat.i18n, Scope::TrashItems, 1),
            trash::scope_line(&cat.i18n, Scope::TrashItems, 2)
        ),
        "This Trash item is deleted from disk. This cannot be undone. // These Trash items are deleted from disk. This cannot be undone."
    );
}

#[test]
fn menu_delete_scope_line_names_its_own_scope() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:51
    assert_eq!(
        trash::scope_line(&cat.i18n, Scope::Items, 1),
        "This item is deleted from disk. This cannot be undone."
    );
}

#[test]
fn empty_trash_body_deletes_it_for_one_and_them_otherwise() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:54
    assert_eq!(
        format!("{} // {}", trash::all_line(&cat.i18n, 1, "4 KiB"), trash::all_line(&cat.i18n, 1234, "1.2 GiB")),
        "1 item, 4 KiB. This deletes it from disk. This cannot be undone. // 1,234 items, 1.2 GiB. This deletes them from disk. This cannot be undone."
    );
}

#[test]
fn empty_trash_body_cites_no_undo_key_or_journal() {
    let cat = Catalog::english();
    let body = format!(
        "{}{}",
        trash::all_line(&cat.i18n, 1, "4 KiB"),
        trash::all_line(&cat.i18n, 1234, "1.2 GiB")
    );
    // oracle tests/js/opstrash.js:57
    assert!(!(body.contains("cannot undo ") || body.contains("journal")));
}

#[test]
fn finished_trash_delete_names_its_total() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:59
    assert_eq!(
        format!(
            "{} // {} // {}",
            lines::done_of(&cat.i18n, "deleted", 1, 1),
            lines::done_of(&cat.i18n, "deleted", 1, 2),
            lines::done_of(&cat.i18n, "restored", 0, 1)
        ),
        "Deleted 1 of 1 item // Deleted 1 of 2 items // Restored 0 of 1 item"
    );
}

#[test]
fn trash_failure_without_a_reason_reads_as_before() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:67
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 0, 1, ""),
        "That item could not be moved to Trash."
    );
}

#[test]
fn trash_failure_with_a_reason_names_it() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:69
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 0, 1, "permission denied"),
        "That item could not be moved to Trash: permission denied."
    );
}

#[test]
fn partly_failed_trash_names_the_reason_too() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:71
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 2, 1, "permission denied"),
        "Moved 2 items to Trash, 1 failed: permission denied · z undoes"
    );
}

#[test]
fn successful_trash_never_carries_a_reason() {
    let cat = Catalog::english();
    // oracle tests/js/opstrash.js:73
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 2, 0, "permission denied"),
        "Moved 2 items to Trash · z undoes"
    );
}
