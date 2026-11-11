// Moves out of folders that will not give their items up (soak S4-7).
use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::AtomicBool;

fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
    Progress {
        cancel: flag,
        on_bytes: sink,
        partial: None,
        tree: None,
        manifest: None,
        durability: None,
        for_move: true,
    }
}

// S4-7: a move out of a read-only folder copied the item, emptied the source and journaled nothing.
#[test]
fn a_move_out_of_a_read_only_folder_fails_before_it_copies_and_keeps_the_source_whole() {
    let d = TestDir::new("romove");
    let src = d.dir("src");
    std::fs::create_dir_all(src.join("tree/sub")).unwrap();
    std::fs::write(src.join("tree/a.txt"), "a").unwrap();
    std::fs::write(src.join("tree/sub/b.txt"), "b").unwrap();
    std::fs::write(src.join("f.txt"), "f").unwrap();
    let out = d.dir("out");
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o555)).unwrap();
    test_take_buffers();
    test_force_exdev(true);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut outcomes = Vec::new();
    for name in ["tree", "f.txt"] {
        let mut p = quiet(&flag, &mut sink);
        outcomes.push((
            move_any(&src.join(name), &out.join(name), &mut p),
            p.partial.take(),
        ));
    }
    test_force_exdev(false);
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (outcome, partial) in outcomes {
        let e = outcome.expect_err("the folder refuses to give the item up");
        assert_eq!(
            (e.where_.as_str(), e.msg.as_str()),
            ("move", "permission denied")
        );
        assert_eq!(
            partial, None,
            "nothing was copied, so nothing is left for undo"
        );
    }
    assert_eq!(
        std::fs::read_to_string(src.join("tree/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(
        std::fs::read_to_string(src.join("tree/sub/b.txt")).unwrap(),
        "b"
    );
    assert_eq!(std::fs::read_to_string(src.join("f.txt")).unwrap(), "f");
    assert_eq!(
        std::fs::read_dir(&out).unwrap().count(),
        0,
        "no copy landed"
    );
    assert!(
        test_take_buffers().is_empty(),
        "no byte was read: the folder refused before the copy began"
    );
}

// S4-7, inside the tree: a read-only folder in the moved folder fails the move before its contents copy.
#[test]
fn a_move_whose_tree_holds_a_read_only_folder_keeps_every_source_file() {
    let d = TestDir::new("romoveinner");
    let tree = d.dir("tree");
    std::fs::write(tree.join("a.txt"), "a").unwrap();
    std::fs::create_dir(tree.join("shut")).unwrap();
    std::fs::write(tree.join("shut/b.txt"), "b").unwrap();
    std::fs::set_permissions(tree.join("shut"), std::fs::Permissions::from_mode(0o555)).unwrap();
    test_force_exdev(true);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink);
    let moved = d.join("moved");
    let outcome = move_any(&tree, &moved, &mut p);
    test_force_exdev(false);
    std::fs::set_permissions(tree.join("shut"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        outcome
            .expect_err("the inner folder cannot give up b.txt")
            .msg,
        "permission denied"
    );
    // A copy that ran to the end would reach the removal, which unlinks a.txt and says part was removed.
    assert_eq!(
        std::fs::read_to_string(tree.join("a.txt")).unwrap(),
        "a",
        "nothing of the source was removed"
    );
    assert_eq!(
        std::fs::read_to_string(tree.join("shut/b.txt")).unwrap(),
        "b"
    );
    assert!(
        !moved.exists() || p.partial.as_deref() == Some(moved.as_path()),
        "no copy is left outside the journal"
    );
}

// S4-7, at the last step: a folder that turns read-only while the copy runs keeps the source, and the copy goes.
#[test]
fn a_source_its_folder_will_not_give_up_takes_its_landed_copy_back() {
    let d = TestDir::new("romovelate");
    let src = d.dir("src");
    std::fs::create_dir(src.join("tree")).unwrap();
    std::fs::write(src.join("tree/a.txt"), "a").unwrap();
    let copy = d.dir("copy");
    std::fs::write(copy.join("a.txt"), "a").unwrap();
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o555)).unwrap();
    let outcome = finish_move(&src.join("tree"), &copy, None);
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o755)).unwrap();
    let unmoved = outcome.expect_err("the folder refuses");
    assert!(
        !unmoved.copy_to_undo,
        "the copy went back, so there is nothing for undo"
    );
    assert_eq!(unmoved.error.msg, "permission denied");
    assert!(!copy.exists(), "the landed copy went");
    assert_eq!(
        std::fs::read_to_string(src.join("tree/a.txt")).unwrap(),
        "a"
    );
}

// A source emptied partway, its inner folder refusing, keeps the copy as the only whole one and says so.
#[test]
fn a_source_removed_partway_keeps_its_copy_and_says_so() {
    let d = TestDir::new("partmove");
    let (tree, copy) = (d.dir("tree"), d.dir("copy"));
    for root in [&tree, &copy] {
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::create_dir(root.join("shut")).unwrap();
        std::fs::write(root.join("shut/b.txt"), "b").unwrap();
    }
    std::fs::set_permissions(tree.join("shut"), std::fs::Permissions::from_mode(0o555)).unwrap();
    // The removal unlinks a folder's files before it enters a subfolder, so a.txt goes first in any listing order.
    let outcome = finish_move(&tree, &copy, None);
    std::fs::set_permissions(tree.join("shut"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let unmoved = outcome.expect_err("the inner folder keeps b.txt");
    assert!(
        !unmoved.copy_to_undo,
        "the copy is the only whole one, so it is not taken back"
    );
    assert_eq!(
        unmoved.error.msg,
        "permission denied; part of it was removed, and the copy holds all of it"
    );
    assert!(!tree.join("a.txt").exists(), "the removal had begun");
    assert_eq!(
        std::fs::read_to_string(copy.join("shut/b.txt")).unwrap(),
        "b",
        "the copy stays whole"
    );
}

// S4-10 through undo: taking back a copied tree 3,000 folders deep removes all of it.
#[test]
fn undo_takes_back_a_tree_three_thousand_folders_deep() {
    use crate::backend::undo::{Entry, Journal, Step};
    let d = TestDir::new("deepundo");
    let root = d.join("deep");
    super::deep_tests::deep_tree(&root, 3000);
    let mut journal = Journal::new();
    let steps = vec![Step::Created { path: root.clone() }];
    journal.push(Entry {
        op: "copy".to_string(),
        steps,
    });
    assert_eq!(journal.undo().map(|_| ()).map_err(|e| e.msg), Ok(()));
    assert!(!root.exists(), "the whole tree went back");
}
