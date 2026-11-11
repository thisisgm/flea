//! Copy As Relative Path: read from the folder the menu captured, kept on the claim while the backend resolves rows, hidden in Recent; backend id 7.
use super::rig::*;

fn pick(rig: &mut Rig, snapshot: &[String], rows: Vec<usize>, listed: &str, recent: bool) {
    let mut landing = Rig::landing(listed);
    landing.recent = recent;
    let said = rig.board.menu_action(
        &rig.en,
        7,
        "copyAs:copyRelative",
        snapshot,
        "",
        rows,
        Rig::take_facts(listed),
        landing,
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(said.is_empty(), "said {said:?}");
}

#[test]
fn a_snapshot_copies_each_item_from_the_listed_folder() {
    let mut rig = Rig::new();
    let snapshot = vec![
        "/home/gm/proj/src/a.rs".to_string(),
        "/home/gm/proj/b.rs".to_string(),
    ];
    pick(&mut rig, &snapshot, Vec::new(), "/home/gm/proj", false);
    assert_eq!(rig.owner.texts.borrow().as_slice(), &["src/a.rs\nb.rs"]);
}

// Ops.copyAs stores pane.path on the claim, so a folder change while the backend resolves rows cannot rewrite the base.
#[test]
fn rows_resolve_against_the_folder_captured_at_the_pick() {
    let mut rig = Rig::new();
    pick(&mut rig, &[], vec![0, 1], "/home/gm/proj", false);
    assert_eq!(rig.set_lines(), vec![r#"{"c":"paths","rows":[0,1]}"#]);
    rig.board.paths_resolved(
        &rig.en,
        7,
        vec!["/home/gm/proj/a.rs".into(), "/home/gm/proj/src/b.rs".into()],
        Rig::take_facts("/home/gm/elsewhere"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.owner.texts.borrow().as_slice(), &["a.rs\nsrc/b.rs"]);
}

// The flyout hides the row in Recent; a pick that still arrives there copies nothing and asks the backend nothing.
#[test]
fn a_pick_in_recent_copies_nothing() {
    let mut rig = Rig::new();
    pick(
        &mut rig,
        &["/home/gm/a.txt".to_string()],
        vec![0],
        "/",
        true,
    );
    assert!(rig.owner.texts.borrow().is_empty());
    assert!(rig.set_lines().is_empty(), "sent {:?}", rig.set_lines());
}
