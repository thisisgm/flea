// Ported checks from tests/js/collide.js and tests/js/collidemerge.js, continued: done lines and the merge ring.
use super::collide::{folder_move, folder_paste, key};
use crate::ui_logic_op1::collide::{self, Choice};
use crate::ui_logic_op1::host::{Host, Step};
use crate::ui_logic_op1::lines;
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::transfer::Transfer;
use crate::ui_logic_op1::TransferDone;

#[test]
fn done_receipts_split_the_undo_advice_from_the_line() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    let copy = Transfer::started(1, false, 2, false);
    let cases = [
        (
            TransferDone {
                ok: 2,
                ..Default::default()
            },
            "Copied 2 items",
            true,
        ),
        (
            TransferDone {
                failed: 2,
                ..Default::default()
            },
            "Copied 0 of 2 · 2 failed",
            false,
        ),
        (
            TransferDone {
                into: "trip".into(),
                merged: 3,
                ok: 1,
                ..Default::default()
            },
            "Merged 3 items into trip",
            true,
        ),
        (
            TransferDone {
                into: "trip".into(),
                failed: 1,
                ..Default::default()
            },
            "Merged 0 items into trip · 1 failed",
            false,
        ),
        // The merge arm answers from merged, never ok: a kept-both-only merge offers no undo.
        (
            TransferDone {
                into: "trip".into(),
                merged: 2,
                ..Default::default()
            },
            "Merged 2 items into trip",
            true,
        ),
        (
            TransferDone {
                into: "trip".into(),
                kept_both: 2,
                ok: 2,
                ..Default::default()
            },
            "Merged 0 items into trip · 2 kept both",
            false,
        ),
    ];
    for (done, body, undo) in cases {
        assert_eq!(
            lines::done_receipt(i18n, &copy, &done),
            (body.to_owned(), undo)
        );
        let whole = lines::done_line(i18n, "default", &copy, &done);
        assert_eq!(
            whole,
            if undo {
                format!("{body} · z undoes")
            } else {
                body.to_owned()
            }
        );
    }
}

#[test]
fn done_lines_follow_the_drive_verdict_and_the_merge() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    let copy = Transfer::started(1, false, 2, false);
    // oracle tests/js/collidemerge.js:52
    let one = TransferDone {
        into: "trip".into(),
        merged: 212,
        ok: 1,
        ..Default::default()
    };
    assert_eq!(
        lines::done_line(i18n, "default", &copy, &one),
        "Merged 212 items into trip · z undoes"
    );
    // oracle tests/js/collidemerge.js:55
    let several = TransferDone {
        into: "Pictures".into(),
        merged: 40,
        kept_both: 3,
        ok: 2,
        ..Default::default()
    };
    assert_eq!(
        lines::done_line(i18n, "default", &copy, &several),
        "Merged 40 items into Pictures · 3 kept both · z undoes"
    );
    // oracle tests/js/collidemerge.js:58
    let ordinary = TransferDone {
        ok: 2,
        ..Default::default()
    };
    assert_eq!(
        lines::done_line(i18n, "default", &copy, &ordinary),
        "Copied 2 items · z undoes"
    );
    // oracle tests/js/collidemerge.js:61
    let verdict = TransferDone {
        ok: 2,
        note: "rclone uploads them in the background".into(),
        durable: false,
        ..Default::default()
    };
    assert_eq!(
        lines::done_line(i18n, "default", &copy, &verdict),
        "Copied 2 items · rclone uploads them in the background · z undoes"
    );
    // oracle tests/js/collidemerge.js:67
    let rclone = TransferDone {
        into: "trip".into(),
        merged: 212,
        ok: 1,
        note: "rclone uploads them in the background".into(),
        durable: false,
        ..Default::default()
    };
    assert_eq!(
        lines::done_line(i18n, "default", &copy, &rclone),
        "Merged 212 items into trip · rclone uploads them in the background · z undoes"
    );
}

#[test]
fn undo_and_redo_words_are_whole_sentences() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collidemerge.js:73
    assert_eq!(lines::undone(i18n, "merge"), "Undid the merge");
    // oracle tests/js/collidemerge.js:86
    assert_eq!(
        lines::with_undo(i18n, "default", lines::redone(i18n, "copy")),
        "Redid the copy · z undoes"
    );
    // oracle tests/js/collidemerge.js:87
    assert_eq!(
        lines::with_undo(i18n, "default", lines::redone(i18n, "move")),
        "Redid the move · z undoes"
    );
    // oracle tests/js/collidemerge.js:88
    assert_eq!(
        lines::with_undo(i18n, "default", lines::redone(i18n, "merge")),
        "Redid the merge · z undoes"
    );
}

#[test]
fn a_hidden_merge_is_skipped_by_the_ring() {
    // oracle tests/js/collidemerge.js:95
    assert_eq!(
        collide::moved(Choice::Skip, key("l"), false),
        Choice::Replace
    );
    // oracle tests/js/collidemerge.js:96
    assert_eq!(collide::moved(Choice::Skip, key("l"), true), Choice::Merge);
    // oracle tests/js/collidemerge.js:97
    assert_eq!(
        collide::moved(Choice::Skip, key("tab"), false),
        Choice::Replace
    );
}

#[test]
fn merge_holds_the_transfer_across_two_cards() {
    let mut host = Host::default();
    // oracle tests/js/collidemerge.js:105
    assert_eq!(
        host.ask(&folder_paste(), &[], false, 0),
        Step::Send(r#"{"c":"collisions","id":1,"dest":"/d/Pictures","paths":["/s/trip"]}"#.into())
    );
    // oracle tests/js/collidemerge.js:107
    let walk = host.decide(Choice::Merge);
    assert_eq!(
        walk,
        Step::Send(
            r#"{"c":"mergecollisions","id":1,"dest":"/d/Pictures","paths":["/s/trip"]}"#.into()
        )
    );
    // oracle tests/js/collidemerge.js:110
    assert_eq!(
        walk,
        Step::Send(
            r#"{"c":"mergecollisions","id":1,"dest":"/d/Pictures","paths":["/s/trip"]}"#.into()
        )
    );
    // oracle tests/js/collidemerge.js:112
    assert!(host.pending.is_some() && host.merging);
    // oracle tests/js/collidemerge.js:115
    assert_eq!(host.decide_merge(Choice::Keep), Step::Send(r#"{"c":"transfer","op":"copy","paths":["/s/trip"],"dest":"/d/Pictures","collide":"merge","collideId":1,"inner":"keep"}"#.into()));
    // oracle tests/js/collidemerge.js:117
    assert!(host.pending.is_none() && !host.merging);
    // oracle tests/js/collidemerge.js:121
    assert_eq!(
        host.ask(&folder_move(), &[], true, 0),
        Step::Send(r#"{"c":"collisions","id":2,"dest":"/d/Pictures","paths":["/s/trip"]}"#.into())
    );
    assert_eq!(
        host.decide(Choice::Merge),
        Step::Send(
            r#"{"c":"mergecollisions","id":2,"dest":"/d/Pictures","paths":["/s/trip"]}"#.into()
        )
    );
    assert_eq!(host.decide_merge(Choice::Cancel), Step::Nothing);
    assert!(host.pending.is_none());
}
