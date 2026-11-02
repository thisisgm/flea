use super::keys::Verb;
use super::rig::*;
use flea::ui_logic_dg1::{LinkKind, PasteAsk};
use flea::ui_logic_mn1::copyas::Format;

#[test]
fn verbs_route_in_every_preset_spelling() {
    assert_eq!(Verb::parse("copy"), Some(Verb::Copy));
    assert_eq!(Verb::parse("cut"), Some(Verb::Cut));
    assert_eq!(Verb::parse("cutArm"), Some(Verb::Cut));
    assert_eq!(Verb::parse("paste"), Some(Verb::Paste));
    assert_eq!(Verb::parse("pasteArm"), Some(Verb::Paste));
    assert_eq!(Verb::parse("movePaste"), Some(Verb::MovePaste));
    assert_eq!(
        Verb::parse("pasteLink"),
        Some(Verb::Link(LinkKind::Relative))
    );
    assert_eq!(
        Verb::parse("pasteAbsoluteLink"),
        Some(Verb::Link(LinkKind::Absolute))
    );
    assert_eq!(
        Verb::parse("pasteHardLink"),
        Some(Verb::Link(LinkKind::Hard))
    );
    assert_eq!(Verb::parse("pasteAs"), None);
    assert_eq!(Verb::parse("copyAs"), None);
}

#[test]
fn copy_and_cut_take_rows_and_paste_reads_the_selection() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    assert_eq!(rig.set_lines(), vec![r#"{"c":"clipWatch"}"#]);
    rig.board.watch(7, &rig.wire);
    assert_eq!(rig.set_lines().len(), 1);
    let facts = Rig::take_facts("/dest");
    let landing = Rig::landing("/dest");
    let said = rig
        .board
        .act(
            &rig.en,
            7,
            Verb::Cut,
            None,
            vec![0],
            facts,
            landing,
            &rig.wire,
            &mut rig.sink,
            &mut rig.owner,
        )
        .unwrap();
    assert!(said.is_empty());
    assert_eq!(rig.set_lines()[1], r#"{"c":"paths","rows":[0]}"#);
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/B".into()],
        Rig::take_facts("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(
        rig.set_lines()[2],
        r#"{"c":"clipSet","op":"cut","paths":["/dest/B"]}"#
    );
    assert!(rig.board.available());
    assert!(
        rig.board.watch_failed(7),
        "the initial watcher offer has not arrived yet"
    );
    let said = rig
        .board
        .act(
            &rig.en,
            7,
            Verb::Paste,
            None,
            Vec::new(),
            facts,
            landing,
            &rig.wire,
            &mut rig.sink,
            &mut rig.owner,
        )
        .unwrap();
    assert!(said.is_empty());
    let asks = rig.sink.asks.borrow();
    assert_eq!(asks.len(), 1);
    match &asks[0].0 {
        PasteAsk::Transfer { op, spends_cut, .. } => {
            assert_eq!(op, "move");
            assert!(spends_cut);
        }
        other => panic!("a cut pastes as a move, got {other:?}"),
    }
    assert_eq!(asks[0].1.token, "");
}

#[test]
fn menu_picks_publish_text_and_paste_links() {
    let mut rig = Rig::new();
    let landing = Rig::landing("/dest");
    rig.board.menu_action(
        &rig.en,
        7,
        "copyAs:copyName",
        &["/dest/IMG_4121.jpg".into()],
        "",
        Vec::new(),
        Rig::take_facts("/dest"),
        landing,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.owner.texts.borrow().as_slice(), &["IMG_4121.jpg"]);
    assert!(rig.set_lines().is_empty());
    rig.board.menu_action(
        &rig.en,
        7,
        "copypath",
        &["/dest/a.txt".into(), "/dest/b.txt".into()],
        "/dest/cursor.txt",
        Vec::new(),
        Rig::take_facts("/dest"),
        landing,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.owner.texts.borrow().len(), 2);
    assert_eq!(rig.owner.texts.borrow()[1], "/dest/a.txt");
    let said = rig.board.menu_action(
        &rig.en,
        7,
        "pasteAs:pasteHardLink",
        &[],
        "",
        Vec::new(),
        Rig::take_facts("/dest"),
        landing,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(
        said[0].text,
        "There is nothing to paste; y copies and x cuts."
    );
    assert_eq!(
        rig.board
            .menu_action(
                &rig.en,
                7,
                "invertSelection",
                &[],
                "",
                Vec::new(),
                Rig::take_facts("/dest"),
                landing,
                &rig.wire,
                &mut rig.sink,
                &mut rig.owner
            )
            .len(),
        0
    );
}

#[test]
fn cut_rows_carry_scissors_and_copy_rows_carry_copy() {
    let mut rig = Rig::new();
    assert_eq!(rig.board.mark_for_row("/dest", "B"), "");
    assert_eq!(rig.board.calls, 1);
    rig.take(true, vec![0]);
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/B".into()],
        Rig::take_facts("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.board.mark_for_row("/dest", "B"), "scissors");
    assert_eq!(rig.board.mark_for_row("/dest", "other"), "");
    assert_eq!(rig.board.builds, 1);
    assert_eq!(rig.board.calls, 3);
    // The same row taken as a copy drives the copy branch.
    rig.take(false, vec![0]);
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/B".into()],
        Rig::take_facts("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.board.mark_for_row("/dest", "B"), "copy");
}

#[test]
fn copy_as_rows_resolve_through_the_backend() {
    let mut rig = Rig::new();
    rig.board.copy_as(
        &rig.en,
        7,
        Format::Stem,
        None,
        vec![3],
        Rig::take_facts("/dest"),
        &rig.wire,
        &mut rig.owner,
    );
    assert_eq!(rig.set_lines(), vec![r#"{"c":"paths","rows":[3]}"#]);
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/archive.tar.gz".into()],
        Rig::take_facts("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.owner.texts.borrow().as_slice(), &["archive.tar"]);
}

// Ops.copyAs with no paths takes the pane's target rows, so an empty menu snapshot resolves through the backend (ui/js/Ops.js:434-441).
#[test]
fn copy_as_with_an_empty_snapshot_falls_back_to_rows() {
    let mut rig = Rig::new();
    let said = rig.board.menu_action(
        &rig.en,
        7,
        "copyPath",
        &[],
        "/dest/cursor",
        vec![3],
        Rig::take_facts("/dest"),
        Rig::landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(said.is_empty());
    assert_eq!(rig.set_lines(), vec![r#"{"c":"paths","rows":[3]}"#]);
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/dest/archive.tar.gz".into()],
        Rig::take_facts("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(
        rig.owner.texts.borrow().as_slice(),
        &["/dest/archive.tar.gz"]
    );
}

// Recent refuses every paste verb before any read (ui/js/Focus.js:217,235,237).
#[test]
fn paste_verbs_refuse_in_recent_before_reading() {
    let mut rig = Rig::new();
    let mut recent = Rig::landing("/dest");
    recent.recent = true;
    for verb in [
        Verb::Paste,
        Verb::MovePaste,
        Verb::Link(LinkKind::Relative),
        Verb::Link(LinkKind::Absolute),
        Verb::Link(LinkKind::Hard),
    ] {
        let said = rig
            .board
            .act(
                &rig.en,
                7,
                verb,
                None,
                Vec::new(),
                Rig::take_facts("/dest"),
                recent,
                &rig.wire,
                &mut rig.sink,
                &mut rig.owner,
            )
            .unwrap();
        assert_eq!(said.len(), 1, "{verb:?}");
        assert_eq!(
            said[0].text, "Recent cannot take a paste. Open a folder first",
            "{verb:?}"
        );
        assert!(!said[0].error, "{verb:?}");
    }
    assert!(rig.sink.asks.borrow().is_empty());
    assert!(rig.set_lines().is_empty());
    assert!(rig.owner.texts.borrow().is_empty());
}
