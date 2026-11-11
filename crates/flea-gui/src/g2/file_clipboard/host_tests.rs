//! The wire side: clip lines route to the session, the fork publishes validated snapshots, foreign selections apply at their generation, and spent cuts clear exactly their capture; backend id 7, listing id 11.
use super::rig::*;
use super::seams::{payloads, Foreign, MenuAction};
use super::{Landing, Take};
use flea::ui_logic_context_menus::copyas::Format;
use flea::ui_logic_file_clipboard::{PendingPaste, FLEA, GNOME, URILIST};

fn take(path: &str) -> Take<'_> {
    Rig::take_facts(path)
}

fn landing(path: &str) -> Landing<'_> {
    Rig::landing(path)
}

#[test]
fn menu_action_kinds_parse_once() {
    assert_eq!(
        MenuAction::parse("copyAs:copyName"),
        MenuAction::CopyAs("copyName".into())
    );
    assert_eq!(
        MenuAction::parse("copyPath"),
        MenuAction::CopyAs("copyPath".into())
    );
    assert_eq!(MenuAction::parse("copypath"), MenuAction::CopyFirst);
    assert!(matches!(
        MenuAction::parse("pasteAs:pasteLink"),
        MenuAction::PasteAs(_)
    ));
    assert_eq!(
        MenuAction::parse("pasteAs:everything"),
        MenuAction::Unhandled
    );
    assert_eq!(MenuAction::parse("invertSelection"), MenuAction::Unhandled);
}

// One copy makes one claim, B's clipSet; the fork publishes nothing until B mints a token without spawning an owner (g2-fork-dg0 fork/docs/native-mime-transfers.md:20), and payloads_serve_gnome_uri_and_plain_with_kde_only_on_cut pins the shapes it would serve.
#[test]
fn a_set_answer_keeps_the_claim_on_the_wire() {
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
    assert_eq!(said.len(), 1);
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
    assert!(
        rig.set_lines()
            .iter()
            .any(|line| line == r#"{"c":"clipSet","op":"cut","paths":["/dest/B"]}"#),
        "sent {:?}",
        rig.set_lines()
    );
    assert!(
        rig.owner.published.borrow().is_empty(),
        "the fork holds nothing until the B prerequisite lands"
    );
}

#[test]
fn payloads_serve_gnome_uri_and_plain_with_kde_only_on_cut() {
    let copy = payloads(false, &["/a/f1".to_string()], "t");
    assert_eq!(copy.len(), 6);
    let cut = payloads(true, &["/a/f1".to_string()], "t");
    assert_eq!(cut.len(), 7);
    let mimes: Vec<&str> = cut.iter().map(|payload| payload.mime).collect();
    assert!(mimes.contains(&GNOME) && mimes.contains(&URILIST) && mimes.contains(&FLEA));
    assert!(cut
        .iter()
        .any(|payload| payload.mime == "application/x-kde-cutselection"));
    let gnome = cut.iter().find(|payload| payload.mime == GNOME).unwrap();
    // B's src/clip/format.rs writes no trailing newline; the fork doc's newline example is accepted on parse, never written.
    assert_eq!(gnome.bytes, b"cut\nfile:///a/f1");
}

#[test]
fn foreign_selections_apply_at_their_generation() {
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
    let generation = 1;
    rig.owner.foreign.borrow_mut().push(Foreign {
        clip: "cut".into(),
        paths: vec!["/foreign/f".into()],
        token: "foreign".into(),
        generation,
    });
    rig.board.foreign(&mut rig.owner);
    assert_eq!(rig.board.token(), "foreign");
    assert!(rig.board.available());
    rig.owner.foreign.borrow_mut().push(Foreign {
        clip: "copy".into(),
        paths: vec!["/stale/f".into()],
        token: "".into(),
        generation: 0,
    });
    rig.board.foreign(&mut rig.owner);
    assert_eq!(rig.board.token(), "foreign");
}

#[test]
fn other_lines_stay_unclaimed_for_their_owner() {
    let mut rig = Rig::new();
    let (claimed, _) = rig.board.line(
        &rig.en,
        7,
        r#"{"t":"job","op":"update"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(!claimed);
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
    assert!(!claimed);
}

#[test]
fn a_fallback_read_serves_the_waiting_paste_from_the_answer() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    let (_, _) = rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"changed","clip":"none","error":"no data-control"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(rig.board.watch_failed(7));
    let said = rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(said.is_empty());
    assert_eq!(rig.wire.sent.borrow().last().unwrap(), r#"{"c":"clipGet"}"#);
    let (claimed, _) = rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/fallback/file"],"token":""}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(claimed);
    let asks = rig.sink.asks.borrow();
    assert_eq!(asks.len(), 1);
    match &asks[0].0 {
        flea::ui_logic_file_clipboard::PasteAsk::Transfer { op, .. } => assert_eq!(op, "copy"),
        other => panic!("the fallback pastes, got {other:?}"),
    }
}

// Ops.copyAs refuses through refuseWhileBusy while any paths round trip is out, so no asker steals another's reply (ui/js/Ops.js:410, Pane.qml:697).
#[test]
fn copy_as_rows_never_claim_another_askers_reply() {
    let mut rig = Rig::new();
    let busy = Take {
        path: "/dest",
        listing: 11,
        paths_busy: true,
        list_in_flight: false,
    };
    let said = rig.board.take(
        &rig.en,
        7,
        false,
        None,
        vec![0],
        busy,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(
        said[0].text,
        "Still resolving the last selection; try again."
    );
    let said = rig.board.copy_as(
        &rig.en,
        7,
        Format::Path,
        None,
        vec![2],
        busy,
        &rig.wire,
        &mut rig.owner,
    );
    assert_eq!(
        said.len(),
        1,
        "Copy As refuses while another asker holds the slot"
    );
    assert_eq!(
        said[0].text,
        "Still resolving the last selection; try again."
    );
    let (claimed, _) = rig.board.line(
        &rig.en,
        7,
        r#"{"t":"paths","paths":["/dest/other-askers-row"]}"#,
        busy,
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let texts = rig.owner.texts.borrow().clone();
    assert!(
        !claimed && texts.is_empty(),
        "sent {:?}, claimed {claimed}, texts {texts:?}",
        rig.set_lines()
    );
}

// The G1 askers see the same slot through one public read (join seam).
#[test]
fn paths_pending_reports_the_shared_slot() {
    let mut rig = Rig::new();
    assert!(!rig.board.paths_pending(7));
    rig.take(false, vec![0]);
    assert!(rig.board.paths_pending(7));
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/A".into()],
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(!rig.board.paths_pending(7));
    rig.board.copy_as(
        &rig.en,
        7,
        Format::Path,
        None,
        vec![0],
        take("/dest"),
        &rig.wire,
        &mut rig.owner,
    );
    assert!(rig.board.paths_pending(7));
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/A".into()],
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(!rig.board.paths_pending(7));
}

// Pane.pasteLink refuses in Recent for the key and the Paste As menu alike, before any read (ui/Pane.qml:774-777, ui/js/RecentMode.js:13-19).
#[test]
fn a_paste_as_menu_pick_in_recent_says_the_recent_sentence() {
    let mut rig = Rig::new();
    let mut recent = Rig::landing("/dest");
    recent.recent = true;
    let said = rig.board.menu_action(
        &rig.en,
        7,
        "pasteAs:pasteLink",
        &[],
        "",
        Vec::new(),
        take("/dest"),
        recent,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let texts: Vec<String> = said.iter().map(|line| line.text.clone()).collect();
    assert_eq!(
        texts,
        vec!["Recent cannot take a paste. Open a folder first".to_string()]
    );
    assert!(
        !rig.set_lines().iter().any(|line| line.contains("clipGet")),
        "sent {:?}",
        rig.set_lines()
    );
}
