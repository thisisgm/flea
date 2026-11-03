// Ported checks from tests/js/collide.js and tests/js/collidemerge.js, continued.
use super::collide::{
    by_path, by_rows, copy_to, dropbox, folder, folder_paste, folder_rows, lifted, link_paths,
    move_paths, paste_copy, paste_move, row_zero, shelf, transfer_copy_paths,
};
use crate::ui_logic_op1::collide::{self, Choice, Merged};
use crate::ui_logic_op1::host::{Host, Step};
use crate::ui_logic_op1::testcat::Catalog;

#[test]
fn questions_name_the_sources_their_transfer_will() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collide.js:82
    assert_eq!(
        collide::question(&by_path(), &[], 4),
        r#"{"c":"collisions","id":4,"dest":"/b","paths":["/a/x.png"]}"#
    );
    // oracle tests/js/collide.js:85
    assert_eq!(
        collide::question(&by_rows(), &[], 5),
        r#"{"c":"collisions","id":5,"dest":"/d/omarchy","rows":[2,3]}"#
    );
    // oracle tests/js/collide.js:88
    assert_eq!(
        collide::question(&shelf(), &["/s/a.txt"], 6),
        r#"{"c":"collisions","id":6,"dest":"/e","paths":["/s/a.txt"]}"#
    );
    // oracle tests/js/collide.js:91
    assert_eq!(
        collide::question(&copy_to(), &[], 7),
        r#"{"c":"collisions","id":7,"dest":"/f","menuId":31}"#
    );
    // oracle tests/js/collide.js:94
    assert_eq!(
        collide::question(&dropbox(), &[], 8),
        r#"{"c":"collisions","id":8,"dest":"/dropbox","menuId":32}"#
    );
    // oracle tests/js/collide.js:96
    assert_eq!(
        collide::transfer(&by_path(), Choice::Replace, 4),
        r#"{"c":"transfer","op":"copy","paths":["/a/x.png"],"dest":"/b","collide":"replace","collideId":4}"#
    );
    // oracle tests/js/collide.js:98
    let waiting = by_path();
    let _ = collide::transfer(&waiting, Choice::Replace, 4);
    assert_eq!(waiting.get("collide"), None);
    // oracle tests/js/collide.js:99
    assert_eq!(Choice::Refuse.wire(), "refuse");
    // oracle tests/js/collide.js:100
    assert!(collide::refused(false, true));
    assert_eq!(
        collide::waiting_refusal(i18n),
        "A transfer is already waiting on its question; try again once it is answered."
    );
    // oracle tests/js/collide.js:101
    assert!(collide::refused(true, true));
    // oracle tests/js/collide.js:102
    assert!(collide::refused(true, false));
    // oracle tests/js/collide.js:103
    assert!(!collide::refused(false, false));
    // oracle tests/js/collide.js:105
    assert!(collide::dropped_pending("stale", "collisions"));
    // oracle tests/js/collide.js:107
    assert!(
        !collide::dropped_pending("stale", "transfer")
            && !collide::dropped_pending("scan", "collisions")
    );
    // oracle tests/js/collide.js:109
    assert!(!collide::dropped_pending("stale", ""));
}

#[test]
fn rows_keep_the_listing_they_were_read_in() {
    // oracle tests/js/collide.js:113
    let captured = by_rows().waiting(4);
    assert_eq!(captured.get("listing"), Some("4"));
    // oracle tests/js/collide.js:114
    assert_eq!(
        collide::question(&captured, &[], 9),
        r#"{"c":"collisions","id":9,"dest":"/d/omarchy","rows":[2,3],"listing":4}"#
    );
    // oracle tests/js/collide.js:115
    assert_eq!(
        collide::transfer(&captured, Choice::Replace, 9),
        r#"{"c":"transfer","op":"move","rows":[2,3],"dest":"/d/omarchy","listing":4,"collide":"replace","collideId":9}"#
    );
    assert_eq!(captured.waiting(5).get("listing"), Some("4"));
    // oracle tests/js/collide.js:117
    assert_eq!(by_rows().waiting(5).get("listing"), Some("5"));
    // oracle tests/js/collide.js:118
    assert_eq!(by_path().waiting(4).get("listing"), None);
    assert_eq!(by_rows().waiting(0).get("listing"), None);
    // oracle tests/js/collide.js:120
    let handed = by_rows();
    let _ = handed.waiting(4);
    assert_eq!(handed.get("listing"), None);
}

#[test]
fn a_menu_transfer_takes_its_listing_only_at_send() {
    // oracle tests/js/collide.js:121
    assert_eq!(dropbox().waiting(4).get("listing"), None);
    assert_eq!(
        dropbox().waiting(4).stamp_at_send(5).get("listing"),
        Some("5")
    );
}

#[test]
fn paste_spends_the_cut_only_when_it_was_cut() {
    // oracle tests/js/collide.js:132
    let mut cut = Host::default();
    cut.ask(&paste_move(), &[], true, 0);
    assert!(cut.spends_cut);
    // oracle tests/js/collide.js:136
    let mut copy = Host::default();
    copy.ask(&paste_copy(), &[], false, 0);
    assert_eq!(
        copy.pending.as_ref().and_then(|r| r.get("op")),
        Some("\"copy\"")
    );
    assert!(!copy.spends_cut);
}

#[test]
fn a_stamped_drag_keeps_its_listing_while_the_pane_holds_another() {
    // oracle tests/js/collide.js:155
    let stamped = lifted().waiting(8);
    assert_eq!(stamped.get("listing"), Some("7"));
    assert_eq!(
        collide::question(&stamped, &[], 10),
        r#"{"c":"collisions","id":10,"dest":"/d/omarchy","rows":[2],"listing":7}"#
    );
}

#[test]
fn paste_as_links_answers_on_its_own_line() {
    let mut host = Host::default();
    // oracle tests/js/collide.js:172
    assert_eq!(
        host.ask(&link_paths(false), &[], false, 0),
        Step::Send(r#"{"c":"collisions","id":1,"dest":"/d","paths":["/s/a.txt"]}"#.into())
    );
    // oracle tests/js/collide.js:175
    assert_eq!(host.decide(Choice::Keep), Step::Send(r#"{"c":"link","op":"relative","paths":["/s/a.txt"],"dest":"/d","collide":"keep","collideId":1}"#.into()));
    // oracle tests/js/collide.js:179
    host.ask(&link_paths(true), &[], false, 7);
    assert_eq!(host.decide(Choice::Replace), Step::Send(r#"{"c":"link","op":"relative","paths":["/s/a.txt"],"dest":"/d","collide":"replace","collideId":2,"rows":[2],"listing":7}"#.into()));
}

#[test]
fn a_refused_question_ends_only_the_wait_it_refuses() {
    let mut host = Host::default();
    host.ask(&transfer_copy_paths(&["/s/a.txt"], "/d"), &[], false, 0);
    // oracle tests/js/collide.js:275
    assert!(host.pending.is_some());
    // oracle tests/js/collide.js:277
    assert!(host.failed("stale", "collisions"));
    assert!(host.pending.is_none());
    host.ask(&transfer_copy_paths(&["/s/a.txt"], "/d"), &[], false, 0);
    // oracle tests/js/collide.js:281
    assert!(!host.failed("stale", "transfer"));
    assert!(host.pending.is_some());
}

#[test]
fn replace_sends_the_row_in_its_listing_and_cancel_sends_nothing() {
    let mut host = Host::default();
    // oracle tests/js/collide.js:311
    host.ask(&row_zero(), &[], false, 7);
    assert_eq!(host.decide(Choice::Replace), Step::Send(r#"{"c":"transfer","op":"move","rows":[0],"dest":"/d/omarchy","listing":7,"collide":"replace","collideId":1}"#.into()));
    // oracle tests/js/collide.js:316
    let mut cut = Host::default();
    cut.ask(&move_paths(), &[], true, 0);
    assert_eq!(cut.decide(Choice::Cancel), Step::Nothing);
    assert!(cut.pending.is_none());
}

#[test]
fn merge_questions_and_transfers_keep_their_shape() {
    // oracle tests/js/collidemerge.js:12
    assert_eq!(
        collide::merge_question(&folder_paste(), &[], 7),
        r#"{"c":"mergecollisions","id":7,"dest":"/d/Pictures","paths":["/s/trip"]}"#
    );
    // oracle tests/js/collidemerge.js:16
    assert_eq!(
        collide::merge_question(&folder_rows().waiting(4), &[], 7),
        r#"{"c":"mergecollisions","id":7,"dest":"/d/Pictures","rows":[2],"listing":4}"#
    );
    // oracle tests/js/collidemerge.js:19
    assert_eq!(
        collide::merge_transfer(&folder_paste(), Choice::Keep, 7),
        r#"{"c":"transfer","op":"copy","paths":["/s/trip"],"dest":"/d/Pictures","collide":"merge","collideId":7,"inner":"keep"}"#
    );
    // oracle tests/js/collidemerge.js:22
    assert_eq!(
        collide::merge_dest("/d/Pictures", &[folder("trip")]),
        "/d/Pictures/trip"
    );
    // oracle tests/js/collidemerge.js:24
    assert_eq!(
        collide::merge_dest("/d/Pictures", &[folder("a"), folder("b")]),
        "/d/Pictures"
    );
    // oracle tests/js/collidemerge.js:26
    assert_eq!(collide::merge_dest("/d/", &[folder("trip")]), "/d/trip");
    // oracle tests/js/collidemerge.js:27
    assert!(collide::dropped_pending("stale", "mergecollisions"));
}

#[test]
fn merged_lines_say_what_landed_and_where() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collidemerge.js:31
    assert_eq!(
        collide::merged_line(
            i18n,
            "default",
            Merged {
                merged: 212,
                into: "trip",
                ..Default::default()
            }
        ),
        "Merged 212 items into trip · z undoes"
    );
    // oracle tests/js/collidemerge.js:34
    assert_eq!(
        collide::merged_line(
            i18n,
            "default",
            Merged {
                merged: 1,
                into: "trip",
                ..Default::default()
            }
        ),
        "Merged 1 item into trip · z undoes"
    );
    // oracle tests/js/collidemerge.js:37
    assert_eq!(
        collide::merged_line(
            i18n,
            "default",
            Merged {
                merged: 212,
                kept_both: 3,
                into: "trip",
                ..Default::default()
            }
        ),
        "Merged 212 items into trip · 3 kept both · z undoes"
    );
    // oracle tests/js/collidemerge.js:40
    assert_eq!(
        collide::merged_line(
            i18n,
            "default",
            Merged {
                merged: 210,
                kept_both: 3,
                failed: 2,
                into: "trip",
                ..Default::default()
            }
        ),
        "Merged 210 items into trip · 3 kept both · 2 failed · z undoes"
    );
    // oracle tests/js/collidemerge.js:43
    assert_eq!(
        collide::merged_line(
            i18n,
            "default",
            Merged {
                merged: 0,
                failed: 1,
                into: "trip",
                ..Default::default()
            }
        ),
        "Merged 0 items into trip · 1 failed"
    );
    // oracle tests/js/collidemerge.js:46
    assert_eq!(
        collide::merged_line(
            i18n,
            "default",
            Merged {
                merged: 40,
                skipped: 5,
                cancelled: true,
                into: "trip",
                ..Default::default()
            }
        ),
        "Merged 40 items into trip · 5 skipped · cancelled · z undoes"
    );
}
