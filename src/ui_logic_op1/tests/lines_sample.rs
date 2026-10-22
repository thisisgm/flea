// Ported from tests/js/ops.js, continued: samples, terminal lines, trash and bulk lines.
use super::lines::{card, flight, outcome};
use crate::ui_logic_op1::lines::{self, Outcome};
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::transfer::Transfer;

#[test]
fn sample_leaves_the_finished_count_where_it_was() {
    let f = flight();
    // oracle tests/js/ops.js:459
    assert_eq!(
        format!("{} {} {}", f.done, f.index, f.name),
        "8 8 panel-demo.mp4"
    );
}

#[test]
fn sample_keeps_the_id_the_cancel_button_names() {
    let f = flight();
    // oracle tests/js/ops.js:462
    assert_eq!(format!("{} {}", f.id, f.running), "12 true");
}

#[test]
fn item_terminal_line_counts_whole_and_spends_its_sample() {
    let landed = flight().item_done(8, "panel-demo.mp4");
    // oracle tests/js/ops.js:466
    assert_eq!(
        format!("{} {} {}", landed.done, landed.bytes, landed.total),
        "9 0 0"
    );
}

#[test]
fn idle_transfer_is_not_running_and_draws_nothing() {
    let idle = Transfer::default();
    // oracle tests/js/ops.js:469
    assert_eq!(
        format!("{} {} {}", idle.running, idle.done, idle.total),
        "false 0 0"
    );
}

#[test]
fn copy_onto_rclone_says_it_uploads_in_background() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:475
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 5, 0),
            Outcome {
                ok: 5,
                failed: 0,
                skipped: 0,
                cancelled: false,
                note: "rclone uploads them in the background"
            }
        ),
        "Copied 5 items · rclone uploads them in the background · z undoes"
    );
}

#[test]
fn unconfirmed_folder_says_so_in_the_note() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:478
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            Outcome {
                ok: 2,
                failed: 0,
                skipped: 0,
                cancelled: false,
                note: "copied, but the drive did not confirm the folder"
            }
        ),
        "Copied 2 items · copied, but the drive did not confirm the folder · z undoes"
    );
}

#[test]
fn verdict_note_with_nothing_landed_stays_unsaid() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:481
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            Outcome {
                ok: 0,
                failed: 2,
                skipped: 0,
                cancelled: false,
                note: "rclone uploads them in the background"
            }
        ),
        "Copied 0 of 2 · 2 failed"
    );
}

#[test]
fn no_verdict_note_leaves_the_shipped_line_alone() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:484
    assert_eq!(
        lines::transfer_done(
            &cat.i18n,
            "default",
            &card(false, 2, 0),
            outcome(2, 0, 0, false)
        ),
        "Copied 2 items · z undoes"
    );
}

#[test]
fn stranded_link_is_named_on_the_linked_line() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:505
    assert_eq!(
        lines::linked_line(
            &cat.i18n,
            "default",
            Outcome {
                ok: 1,
                failed: 1,
                skipped: 0,
                cancelled: false,
                note: "the link at /d/b.txt could not be removed: stale; the item it replaced is still in the Trash",
            }
        ),
        "Linked 1 item · 1 failed · the link at /d/b.txt could not be removed: stale; the item it replaced is still in the Trash · z undoes"
    );
}

#[test]
fn no_stranded_note_leaves_the_linked_line_alone() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:508
    assert_eq!(
        lines::linked_line(&cat.i18n, "default", outcome(2, 0, 0, false)),
        "Linked 2 items · z undoes"
    );
}

#[test]
fn one_renamed_item_takes_the_singular() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:512
    assert_eq!(
        lines::bulk_done_line(&cat.i18n, "default", 1),
        "Renamed 1 item · z undoes"
    );
}

#[test]
fn several_renamed_items_take_the_plural() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:513
    assert_eq!(
        lines::bulk_done_line(&cat.i18n, "default", 5),
        "Renamed 5 items · z undoes"
    );
}
