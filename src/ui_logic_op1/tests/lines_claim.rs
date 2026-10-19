// Ported from tests/js/ops.js, continued: the claim lines.
use crate::ui_logic_op1::lines::{self, Claim};
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::transfer::Transfer;

#[test]
fn paths_reply_with_no_claim_lands_on_clipboard() {
    // oracle tests/js/ops.js:177
    assert_eq!(lines::claim_of(None, ""), Claim::Clipboard);
}

#[test]
fn compress_reply_builds_the_archive_claim() {
    // oracle tests/js/ops.js:191
    assert_eq!(
        lines::claim_of(Some("compress"), "zip"),
        Claim::Compress("zip".into())
    );
}

#[test]
fn copy_as_claim_routes_the_format() {
    // oracle tests/js/ops.js:216
    assert_eq!(
        lines::claim_of(Some("copyAs"), "quoted"),
        Claim::CopyAs("quoted".into())
    );
    // oracle tests/js/ops.js:237
    assert_eq!(
        lines::claim_of(Some("copyAs"), "quoted"),
        Claim::CopyAs("quoted".into())
    );
}

#[test]
fn copy_as_refuses_while_a_drag_claim_waits() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:223
    assert_eq!(
        (
            lines::refuse_while_busy(true, false),
            lines::paths_busy(&cat.i18n)
        ),
        (
            true,
            "Still resolving the last selection; try again.".to_string()
        )
    );
}

#[test]
fn compress_refuses_while_a_cut_waits() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:231
    assert_eq!(
        (
            lines::refuse_while_busy(false, true),
            lines::paths_busy(&cat.i18n)
        ),
        (
            true,
            "Still resolving the last selection; try again.".to_string()
        )
    );
}

#[test]
fn permissions_claim_opens_over_the_resolved_list() {
    // oracle tests/js/ops.js:243
    assert_eq!(lines::claim_of(Some("permissions"), ""), Claim::Permissions);
}

#[test]
fn started_transfer_carries_id_direction_and_count() {
    let t = Transfer::started(12, true, 3, false);
    // oracle tests/js/ops.js:267
    assert_eq!(
        format!("{}/{}/{}/{}", t.id, t.moving, t.n, t.running),
        "12/true/3/true"
    );
}

#[test]
fn redo_progress_keeps_its_operation_identity() {
    let cat = Catalog::english();
    let redo = Transfer {
        redo: "move".into(),
        ..Transfer::started(12, true, 3, false)
    };
    let sample = redo.sampled(1, "photo.heic", 50, 100, 0);
    let done = sample.item_done(1, "photo.heic");
    // oracle tests/js/ops.js:273
    assert_eq!(
        format!("{}/{}/{}", sample.head(&cat.i18n), done.redo, done.done),
        "Redoing move 2 of 3/move/2"
    );
}

#[test]
fn progress_sample_leaves_the_prior_state_unchanged() {
    let redo = Transfer {
        redo: "move".into(),
        ..Transfer::started(12, true, 3, false)
    };
    let _sample = redo.sampled(1, "photo.heic", 50, 100, 0);
    // oracle tests/js/ops.js:276
    assert_eq!(redo.index, 0);
}

#[test]
fn rename_destination_is_the_renamed_sibling() {
    // oracle tests/js/ops.js:309
    assert_eq!(lines::rename_destination("/d/f3", "g3"), "/d/g3");
}
