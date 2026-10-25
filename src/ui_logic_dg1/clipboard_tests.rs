//! tests/js/clipboard.js, the single-window half: publish, own echoes, foreign selections, failures, spent cuts and the collide capture rule.
use super::harness::{english, Pane};
use super::{words, PasteAsk};

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

#[test]
fn copy_marks_locally_before_it_reaches_the_wire() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/a/f1"])), vec![]);
    assert_eq!(pane.sent[0], r#"{"c":"clipSet","op":"copy","paths":["/a/f1"]}"#);
    assert_eq!(pane.said[0].text, "Copied 1 item");
    assert!(pane.said[0].paste_hint);
    assert!(!pane.said[0].error);
}

#[test]
fn an_own_watcher_token_changes_nothing() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/a/f1"])), vec![]);
    pane.set_ok("own");
    assert_eq!(pane.current().token, "own");
    let before = pane.current();
    let counts = (pane.session.borrow().generation(), pane.session.borrow().revision());
    pane.changed("copy", paths(&["/a/f1"]), "own");
    assert_eq!(pane.current(), before);
    assert_eq!((pane.session.borrow().generation(), pane.session.borrow().revision()), counts, "an own echo replaces nothing");
}

#[test]
fn a_foreign_selection_replaces_paths_and_token() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/a/f1"])), vec![]);
    pane.set_ok("own");
    pane.changed("copy", paths(&["/other/copy"]), "foreign");
    assert_eq!(pane.current().paths, paths(&["/other/copy"]));
    assert!(!pane.current().moving);
    assert_eq!(pane.current().token, "foreign");
    pane.changed("cut", paths(&["/other/cut"]), "");
    assert!(pane.current().moving);
    pane.changed("none", Vec::new(), "");
    assert!(pane.current() == super::Clip::empty());
    assert_eq!(pane.current().token, "");
}

#[test]
fn a_failed_setter_keeps_the_local_clip_and_says_the_scope_once() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/a/f2"])), vec![]);
    let before = pane.current();
    let (outs, _validated) = pane.session.borrow_mut().on_set(&en, 1, false, "", "no compositor");
    pane.apply(outs);
    assert_eq!(pane.current(), before);
    assert_eq!(pane.said[1].text, "Copied in this window only: no compositor");
    assert!(pane.said[1].error);
    let said = pane.said.len();
    let (outs, _validated) = pane.session.borrow_mut().on_set(&en, 1, false, "", "no compositor");
    pane.apply(outs);
    assert_eq!(pane.said.len(), said);
}

#[test]
fn a_failed_clear_names_the_system_clipboard() {
    let en = english();
    let mut pane = Pane::new();
    let outs = pane.session.borrow_mut().on_clear(&en, false, "no compositor");
    pane.apply(outs);
    assert_eq!(pane.said[0].text, "Could not clear the system clipboard: no compositor");
    assert!(pane.said[0].error);
}

#[test]
fn a_failed_watch_keeps_the_local_copy_and_reads_before_paste() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/a/f1"])), vec![]);
    pane.set_ok("copy-token");
    pane.watch_error();
    assert!(pane.watch_failed());
    assert_eq!(pane.current().paths, paths(&["/a/f1"]), "the failed watch keeps the local copy");
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.sent.last().unwrap(), r#"{"c":"clipGet"}"#);
    assert!(pane.asked.is_empty());
    pane.get_ok(&en, "copy", paths(&["/fallback/file"]), "");
    assert_eq!(pane.pasted(), "copy:/fallback/file");
}

#[test]
fn a_failed_read_still_pastes_the_local_cut() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/a/f2"])), vec![]);
    pane.set_ok("cut-token");
    pane.watch_error();
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.sent.last().unwrap(), r#"{"c":"clipGet"}"#);
    assert!(pane.asked.is_empty());
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.said.last().unwrap().text, "Still reading the clipboard; try again.");
    assert_eq!(pane.sent.len(), 2);
    pane.get_err(&en);
    assert_eq!(pane.pasted(), "move:/a/f2");
}

#[test]
fn a_spent_cut_clears_by_token_and_a_copy_never_clears() {
    let mut pane = Pane::new();
    pane.changed("cut", paths(&["/a/f2"]), "cut-token");
    let (captured, entry) = pane.captured();
    let outs = pane.session.borrow_mut().spent(&captured, &entry, true);
    pane.apply(outs);
    assert_eq!(pane.sent[0], r#"{"c":"clipClear","token":"cut-token"}"#);
    assert!(pane.current().is_empty());
    pane.changed("cut", paths(&["/foreign/f2"]), "");
    let (captured, entry) = pane.captured();
    let outs = pane.session.borrow_mut().spent(&captured, &entry, true);
    pane.apply(outs);
    assert_eq!(pane.sent[1], r#"{"c":"clipClear","cut":["/foreign/f2"]}"#);
    pane.changed("copy", paths(&["/a/f1"]), "copy-token");
    let (held, entry) = pane.captured();
    let sent = pane.sent.len();
    let outs = pane.session.borrow_mut().spent(&held, &entry, true);
    pane.apply(outs);
    assert_eq!(pane.sent.len(), sent);
    assert_eq!(pane.current(), held);
}

#[test]
fn a_card_clears_its_captured_cut_and_keeps_a_newer_one() {
    let mut pane = Pane::new();
    pane.changed("cut", paths(&["/old/f"]), "old");
    let (old, entry) = pane.captured();
    pane.changed("cut", paths(&["/new/f"]), "new");
    let outs = pane.session.borrow_mut().spent(&old, &entry, true);
    pane.apply(outs);
    assert_eq!(pane.sent[0], r#"{"c":"clipClear","token":"old"}"#);
    assert_eq!(pane.current().token, "new");
}

#[test]
fn a_transfer_spends_its_cut_and_a_cancel_keeps_it() {
    let en = english();
    let mut pane = Pane::new();
    pane.changed("cut", paths(&["/a/f2"]), "sent-cut");
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    match &pane.asked[0] {
        PasteAsk::Transfer { op, spends_cut, .. } => {
            assert_eq!(op, "move");
            assert!(spends_cut);
        }
        other => panic!("a cut pastes as a move, got {other:?}"),
    }
    let (captured, entry) = pane.captured();
    let outs = pane.session.borrow_mut().spent(&captured, &entry, true);
    pane.apply(outs);
    assert!(pane.sent.iter().any(|line| line.contains("clipClear")));
    assert!(pane.current().is_empty());
    pane.changed("cut", paths(&["/a/f2"]), "cancel-cut");
    let (captured, entry) = pane.captured();
    let outs = pane.session.borrow_mut().spent(&captured, &entry, false);
    pane.apply(outs);
    assert_eq!(pane.current().token, "cancel-cut");
}

#[test]
fn an_early_owner_echo_cannot_roll_back_a_newer_local_cut() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, false, Some(paths(&["/early/first"])), vec![]);
    pane.clip(&en, true, Some(paths(&["/early/second"])), vec![]);
    let held = pane.current();
    pane.changed("copy", paths(&["/early/first"]), "first-token");
    assert_eq!(pane.current(), held);
    pane.set_ok("first-token");
    assert_eq!(pane.current(), held);
    pane.changed("cut", paths(&["/early/second"]), "second-token");
    pane.set_ok("second-token");
    assert_eq!(pane.current().token, "second-token");
    assert!(pane.current().moving);
}

#[test]
fn words_resolve_through_the_catalog() {
    let en = english();
    assert_eq!(words::landed(&en, false, 1).text, "Copied 1 item");
    assert!(words::landed(&en, false, 1).paste_hint);
    assert_eq!(words::landed(&en, false, 3).text, "Copied 3 items");
    assert_eq!(words::landed(&en, true, 2).text, "Cut 2 items");
    assert_eq!(en.tr("menu.refusal.empty_clipboard"), "There is nothing to paste; y copies and x cuts.");
}

// Clipboard.receive gives each set's token to its own clip object (ui/js/Clipboard.js:85), so the newest of two equal cuts keeps the newest token.
#[test]
fn two_equal_cuts_keep_the_newest_token() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/a/f"])), vec![]);
    pane.clip(&en, true, Some(paths(&["/a/f"])), vec![]);
    pane.set_ok("11111111111111111111111111111111");
    pane.set_ok("22222222222222222222222222222222");
    let (current, entry) = pane.captured();
    assert_eq!(current.token, "22222222222222222222222222222222", "the live owner is the second set");
    let outs = pane.session.borrow_mut().spent(&current, &entry, true);
    pane.apply(outs);
    assert_eq!(
        pane.sent.last().unwrap(),
        r#"{"c":"clipClear","token":"22222222222222222222222222222222"}"#,
        "the second set's token is the one a spent cut clears"
    );
}

// Ops.copied wraps the landed line in Status.withPaste (tests/js/clipboard.js:63), so the status slot appends the paste key without matching prose.
#[test]
fn a_landed_line_carries_the_paste_hint() {
    let en = english();
    let copied = words::landed(&en, false, 1);
    assert_eq!(copied.text, "Copied 1 item");
    assert!(copied.paste_hint);
    assert!(!copied.error);
    let cut = words::landed(&en, true, 2);
    assert_eq!(cut.text, "Cut 2 items");
    assert!(cut.paste_hint);
}

// Clipboard.spent empties the pane without moving the generation (ui/js/Clipboard.js:172-177), so a fallback read out across a spent cut still serves its paste (ui/js/Clipboard.js:106).
#[test]
fn a_read_out_across_a_spent_cut_still_serves_its_paste() {
    let en = english();
    let mut pane = Pane::new();
    pane.watch_error();
    pane.clip(&en, true, Some(paths(&["/a/f"])), vec![]);
    pane.set_ok("tokf");
    let (cut, entry) = pane.captured();
    pane.paste(&en, super::PendingPaste::transfer("/dest"));
    assert_eq!(pane.sent.last().unwrap(), r#"{"c":"clipGet"}"#);
    let outs = pane.session.borrow_mut().spent(&cut, &entry, true);
    pane.apply(outs);
    pane.get_ok(&en, "copy", paths(&["/foreign/y"]), "y");
    assert_eq!(pane.pasted(), "copy:/foreign/y", "sent {:?}", pane.sent);
}

// A clipboard history that re-offers this window's own older bytes after another owner's selection is that owner's choice, so it applies.
#[test]
fn an_own_token_offered_again_after_a_foreign_selection_applies() {
    let en = english();
    let mut pane = Pane::new();
    pane.clip(&en, true, Some(paths(&["/a/f2"])), vec![]);
    pane.set_ok("own-token");
    let counts = (pane.session.borrow().generation(), pane.session.borrow().revision());
    pane.changed("cut", paths(&["/a/f2"]), "own-token");
    assert_eq!(pane.current().token, "own-token", "the own echo is ignored");
    assert_eq!((pane.session.borrow().generation(), pane.session.borrow().revision()), counts, "the own echo replaces nothing");
    pane.changed("copy", paths(&["/other/b"]), "");
    assert_eq!(pane.current().paths, paths(&["/other/b"]));
    pane.changed("cut", paths(&["/a/f2"]), "own-token");
    assert_eq!(pane.current().paths, paths(&["/a/f2"]), "the re-offered own bytes replace the foreign selection");
    assert!(pane.current().moving);
}
