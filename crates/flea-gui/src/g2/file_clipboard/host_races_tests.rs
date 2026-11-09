//! Clipboard ownership, paths and transfer completion races through the host.
use super::rig::*;
use super::seams::Foreign;
use super::{Landing, Take};
use flea::ui_logic_file_clipboard::PendingPaste;

fn take(path: &str) -> Take<'_> {
    Rig::take_facts(path)
}

fn landing(path: &str) -> Landing<'_> {
    Rig::landing(path)
}

// A transfer spent the cut it captured: exactly that cut is cleared on B's owner, never a newer selection; the fork holds no claim while the B prerequisite is not at BASE, so only the wire clear goes out.
#[test]
fn a_spent_transfer_clears_its_capture_on_the_one_claiming_owner() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/dest/B".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"cut-token"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"changed","clip":"cut","paths":["/dest/B"],"token":"cut-token"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let landing_here = landing("/dest");
    let said = rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing_here,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(said.is_empty());
    let (_, captured) = rig.sink.asks.borrow().last().cloned().unwrap();
    assert_eq!(captured.token, "cut-token");
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    assert!(rig
        .wire
        .sent
        .borrow()
        .iter()
        .any(|line| line == r#"{"c":"clipClear","token":"cut-token"}"#));
    assert!(!rig.board.available());
    assert!(
        rig.owner.cleared.borrow().is_empty(),
        "the fork holds no claim to clear"
    );
}

// ClipSelection.resolved drains after a superseded reply, so the newer take asks its own rows (tests/js/clipboard.js orderedPicks).
#[test]
fn a_superseded_paths_reply_still_asks_the_newer_take() {
    for failed in [false, true] {
        let mut rig = Rig::new();
        rig.take(false, vec![0]);
        rig.take(true, vec![1]);
        assert_eq!(rig.set_lines(), vec![r#"{"c":"paths","rows":[0]}"#]);
        if failed {
            rig.board.paths_failed(
                &rig.en,
                7,
                take("/dest"),
                &rig.wire,
                &mut rig.sink,
                &mut rig.owner,
            );
        } else {
            let (claimed, _) = rig.board.line(
                &rig.en,
                7,
                r#"{"t":"paths","paths":["/dest/A"]}"#,
                take("/dest"),
                landing("/dest"),
                &rig.wire,
                &mut rig.sink,
                &mut rig.owner,
            );
            assert!(claimed);
        }
        assert_eq!(
            rig.set_lines(),
            vec![r#"{"c":"paths","rows":[0]}"#, r#"{"c":"paths","rows":[1]}"#],
            "Cut B must ask its rows once A ends (failed={failed})"
        );
        let (claimed, _) = rig.board.line(
            &rig.en,
            7,
            r#"{"t":"paths","paths":["/dest/B"]}"#,
            take("/dest"),
            landing("/dest"),
            &rig.wire,
            &mut rig.sink,
            &mut rig.owner,
        );
        assert!(claimed);
        assert!(rig
            .set_lines()
            .contains(&r#"{"c":"clipSet","op":"cut","paths":["/dest/B"]}"#.to_string()));
    }
}

// A set reply answers its own clip object, never a newer owner (ui/js/Clipboard.js:85).
#[test]
fn a_late_set_reply_never_publishes_a_newer_owner() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        false,
        Some(vec!["/dest/A".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"changed","clip":"cut","paths":["/foreign/X"],"token":"foreign"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"tokenA"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let published: Vec<(Vec<String>, String, bool)> = rig
        .owner
        .published
        .borrow()
        .iter()
        .map(|item| (item.paths.clone(), item.token.clone(), item.cut))
        .collect();
    assert!(
        published
            .iter()
            .all(|(paths, _, _)| paths != &vec!["/foreign/X".to_string()]),
        "published {published:?}"
    );
}

// A cut spent before its set answers leaves nothing to own, so nothing is published.
#[test]
fn a_cut_spent_before_its_token_never_publishes_an_empty_selection() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/dest/B".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let (_, captured) = rig.sink.asks.borrow().last().cloned().unwrap();
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"tokenB"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let published: Vec<(Vec<String>, String, bool)> = rig
        .owner
        .published
        .borrow()
        .iter()
        .map(|item| (item.paths.clone(), item.token.clone(), item.cut))
        .collect();
    assert!(
        published.iter().all(|(paths, _, _)| !paths.is_empty()),
        "published {published:?}"
    );
}

// The same rule at the session: an equal-path foreign copy that lands after the set must not take the old token.
#[test]
fn a_late_set_reply_never_stamps_a_newer_selection() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        false,
        Some(vec!["/dest/A".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/dest/A"],"token":""}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"tokenA"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(
        rig.board.token(),
        "",
        "the set answer never owns a newer selection"
    );
}

// One copy makes exactly one claim on the native selection, B's clipSet; the fork publishes nothing until B mints a token without an owner (g2-fork-dg0 fork/docs/native-mime-transfers.md:20).
#[test]
fn one_copy_claims_the_native_selection_once() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    let said = rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/dest/B".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(said[0].text, "Cut 1 item");
    let (claimed, said) = rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"cut-token"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(claimed);
    assert!(said.is_empty());
    assert_eq!(rig.board.token(), "cut-token");
    let claims = rig
        .set_lines()
        .iter()
        .filter(|line| line.contains(r#""c":"clipSet""#))
        .count();
    assert_eq!(claims, 1, "one clipSet owns the selection");
    assert!(
        rig.owner.published.borrow().is_empty(),
        "the fork holds nothing until the B prerequisite lands"
    );
}

// The fork numbers its own clipboard changes, so one fork report lands the same in every window whatever the local copy count.
#[test]
fn a_fork_report_does_not_depend_on_the_local_copy_count() {
    let report = || Foreign {
        clip: "copy".into(),
        paths: vec!["/foreign/f".into()],
        token: "f".into(),
        generation: 1,
    };
    let mut once = Rig::new();
    once.board.take(
        &once.en,
        7,
        false,
        Some(vec!["/a".into()]),
        Vec::new(),
        take("/dest"),
        &once.wire,
        &mut once.sink,
        &mut once.owner,
    );
    once.owner.foreign.borrow_mut().push(report());
    once.board.foreign(&mut once.owner);
    let mut twice = Rig::new();
    for path in ["/a", "/b"] {
        twice.board.take(
            &twice.en,
            7,
            false,
            Some(vec![path.into()]),
            Vec::new(),
            take("/dest"),
            &twice.wire,
            &mut twice.sink,
            &mut twice.owner,
        );
    }
    twice.owner.foreign.borrow_mut().push(report());
    twice.board.foreign(&mut twice.owner);
    assert_eq!(once.board.token(), "f");
    assert_eq!(
        twice.board.token(),
        once.board.token(),
        "one copy then fork gen 1 vs two copies then fork gen 1"
    );
}

// The collision card holds the pane's own clip, so a cut pasted before its set answers is cleared by the token the answer stamped on it (ui/CollideHost.qml:38, ui/js/Clipboard.js:83-84, :172-177).

#[path = "host_capture_races_tests.rs"]
mod capture;
