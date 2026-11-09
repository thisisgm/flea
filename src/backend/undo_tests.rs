use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;

fn entry(op: &str, steps: Vec<Step>) -> Entry {
    Entry { op: op.to_string(), steps }
}

fn legacy_identity(path: &Path) -> Option<ItemIdentity> {
    let current = ItemIdentity::inspect(path).unwrap();
    if current.born().is_none() {
        eprintln!("legacy undo check skipped: filesystem has no birth time");
        return None;
    }
    let (dev, ino, kind, len, mtime, changed, _) = current.to_parts();
    Some(ItemIdentity::from_parts(dev, ino, kind, len, mtime, changed, None))
}

#[test]
fn legacy_entry_rebase_updates_every_recorded_identity_slot() {
    let d = TestDir::new("undolegacyrebase");
    let path = d.file("untouched.txt", "body");
    let Some(legacy) = legacy_identity(&path) else { return };
    let current = ItemIdentity::inspect(&path).unwrap();
    let new_path = d.file("new.txt", "new identity");
    let new = ItemIdentity::inspect(&new_path).unwrap();
    let steps = |identity: &ItemIdentity| vec![
        Step::Moved { from: path.clone(), to: path.clone(), before: identity.clone(), after: identity.clone(), rename_fat: false },
        Step::Copied { from: path.clone(), to: path.clone(), source: identity.clone(), created: identity.clone(), manifest: None, manifest_nonce: None, merge: false, rename_fat: false },
        Step::MadeFile { path: path.clone(), identity: identity.clone() },
        Step::MadeDir { path: path.clone(), identity: identity.clone() },
        Step::Linked { path: path.clone(), identity: identity.clone(), source: path.clone(), kind: super::super::link::LinkKind::Relative },
    ];
    let mut recorded = entry("legacy", steps(&legacy));
    recorded.rebase(&current, &new);
    assert_eq!(recorded.steps, steps(&new), "all legacy slots must follow the identity rebase");
}

#[test]
fn legacy_moved_undo_returns_the_identity_rebase_pair() {
    let d = TestDir::new("undolegacymoved");
    let from = d.join("before.txt");
    let to = d.file("after.txt", "body");
    let Some(legacy) = legacy_identity(&to) else { return };
    let current = ItemIdentity::inspect(&to).unwrap();
    let step = Step::Moved { from: from.clone(), to: to.clone(), before: legacy.clone(), after: legacy, rename_fat: false };
    let pair = reverse(&step).expect("legacy move must undo").expect("untouched legacy move must return its rebase pair");
    assert_eq!(pair.0, current);
    assert_eq!(pair.1, ItemIdentity::inspect(&from).unwrap());
    assert!(!to.exists());
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "body");
}

#[test]
fn legacy_copied_undo_removes_the_untouched_copy() {
    let d = TestDir::new("undolegacycopy");
    let source = d.file("source.txt", "body");
    let copy = d.file("copy.txt", "body");
    let Some(legacy) = legacy_identity(&copy) else { return };
    let step = Step::Copied { from: source.clone(), to: copy.clone(), source: ItemIdentity::inspect(&source).unwrap(), created: legacy, manifest: None, manifest_nonce: None, merge: false, rename_fat: false };
    let mut journal = Journal::new();
    journal.push(entry("copy", vec![step]));
    assert_eq!(journal.undo().expect("untouched legacy copy must undo"), "copy");
    assert!(!copy.exists());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "body");
}

#[test]
fn legacy_new_file_undo_removes_the_untouched_empty_file() {
    let d = TestDir::new("undolegacyfile");
    let path = d.file("empty.txt", "");
    let Some(legacy) = legacy_identity(&path) else { return };
    let mut journal = Journal::new();
    journal.push(entry("new file", vec![Step::MadeFile { path: path.clone(), identity: legacy }]));
    assert_eq!(journal.undo().expect("untouched legacy new file must undo"), "new file");
    assert!(!path.exists());
}

#[test]
fn an_empty_journal_answers_an_error_rather_than_claiming_it_undid_something() {
    let mut j = Journal::new();
    let e = j.undo().expect_err("nothing to undo");
    assert_eq!(e.where_, "undo");
    assert!(e.msg.contains("nothing to undo"));
}

#[test]
fn an_operation_that_changed_nothing_is_not_recorded() {
    let mut j = Journal::new();
    j.push(entry("rename", Vec::new()));
    assert!(j.is_empty(), "an empty step list would undo as a no-op reported as work");
}

#[test]
fn the_ring_is_bounded_at_fifty_and_drops_its_oldest() {
    let mut j = Journal::new();
    for i in 0..DEPTH + 10 {
        j.push(entry(&format!("op {}", i), vec![Step::Created { path: PathBuf::from("/x") }]));
    }
    assert_eq!(j.len(), DEPTH);
}

#[test]
fn undoing_a_rename_puts_the_old_name_back() {
    let d = TestDir::new("undorename");
    let from = d.join("before.txt");
    let to = d.file("after.txt", "body");
    let mut j = Journal::new();
    j.push(entry("rename", vec![moved(&from, &to, ItemIdentity::inspect(&to).unwrap()).unwrap()]));
    assert_eq!(j.undo().expect("undo"), "rename");
    assert!(from.exists(), "the original name is back");
    assert!(!to.exists(), "the new name is gone");
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "body");
}

fn born_identity(born: Option<(u64, u32)>) -> ItemIdentity {
    ItemIdentity::from_parts(1, 2, 0o100000, 3, (4, 5), (6, 7), born)
}

#[test]
fn unequal_birth_times_split_two_items_that_share_a_device_inode_and_kind() {
    let was = born_identity(Some((100, 1)));
    assert!(was.same_item(&born_identity(Some((100, 1)))));
    assert!(!was.same_item(&born_identity(Some((100, 2)))), "a later nanosecond is a recreated file");
    assert!(!was.same_item(&born_identity(Some((101, 1)))), "and so is a later second");
    // A side with no birth time (an old record, or a filesystem that keeps none) leaves dev, inode and kind to decide.
    assert!(was.same_item(&born_identity(None)));
    assert!(born_identity(None).same_item(&was));
    assert!(born_identity(None).same_item(&born_identity(None)));
}

#[test]
fn undoing_a_rename_refuses_a_file_born_after_the_one_it_recorded() {
    let d = TestDir::new("undoborn");
    let from = d.join("before.txt");
    let to = d.file("after.txt", "body");
    let landed = ItemIdentity::inspect(&to).unwrap();
    let Some((sec, nsec)) = landed.born() else { return };
    // Same device, inode and kind, a different birth: what a recreated file looks like beside the recorded one.
    let (dev, ino, kind, len, mtime, changed, _) = landed.to_parts();
    let recorded = ItemIdentity::from_parts(dev, ino, kind, len, mtime, changed, Some((sec.wrapping_add(1), nsec)));
    let mut j = Journal::new();
    j.push(entry("rename", vec![Step::Moved { from: from.clone(), to: to.clone(), before: recorded.clone(), after: recorded, rename_fat: false }]));
    let e = j.undo().expect_err("a file born at another time is not the one that was renamed");
    assert!(e.msg.contains("replaced"), "{}", e.msg);
    assert!(to.exists() && !from.exists(), "and it is left exactly where it is");
}

#[test]
fn undoing_a_rename_refuses_to_clobber_a_file_that_took_the_old_name_since() {
    let d = TestDir::new("undoclobber");
    let from = d.file("before.txt", "something else wrote this");
    let to = d.file("after.txt", "body");
    let mut j = Journal::new();
    j.push(entry("rename", vec![moved(&from, &to, ItemIdentity::inspect(&to).unwrap()).unwrap()]));
    let e = j.undo().expect_err("must refuse rather than destroy the newer file");
    assert_eq!(e.where_, "rename");
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "something else wrote this");
}

// Issue 111: the copied root keeps its own ctime while a file inside it is edited or a new one is
// added, so undo removed a tree the user had gone on working in, permanently and not to the trash.
#[test]
fn undoing_a_copied_folder_refuses_once_anything_inside_it_has_changed() {
    for change in ["edit", "add"] {
        let d = TestDir::new(&format!("undotree{change}"));
        let source = d.dir("source");
        std::fs::create_dir(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/document.txt"), "as copied").unwrap();
        let copy = d.join("copy");
        let flag = std::sync::atomic::AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = crate::backend::copyfile::Progress { cancel: &flag, on_bytes: &mut sink, tree: None, partial: None, manifest: None, durability: None, for_move: false };
        crate::backend::copyfile::copy_any(&source, &copy, &mut p).expect("the copy this undo reverses");
        let step = copied(&source, &copy, ItemIdentity::inspect(&source).unwrap()).unwrap();
        // Measured: /tmp is tmpfs, whose timestamps are coarser than this test is fast, so the copy and
        // the edit landed on the same nanosecond and the change was indistinguishable from the copy.
        std::thread::sleep(std::time::Duration::from_millis(20));
        // A later edit and a later create both leave the root's own ctime exactly where the copy left it.
        match change {
            "edit" => std::fs::write(copy.join("nested/document.txt"), "the user's own work").unwrap(),
            _ => std::fs::write(copy.join("nested/new-work.txt"), "the user's own work").unwrap(),
        }
        let mut j = Journal::new();
        j.push(entry("copy", vec![step]));
        let e = j.undo().expect_err("must refuse rather than delete work done since the copy");
        assert_eq!(e.where_, "undo");
        // The distinctive half: the identity refusal beside it says "the copied item changed", so a
        // substring both carry would not say which guard answered.
        assert!(e.msg.contains("something inside the copied folder"), "{}", e.msg);
        // An edit moves the file's own ctime; a create moves the ctime of the directory it landed in.
        let names = if change == "edit" { "nested/document.txt" } else { "nested" };
        assert!(e.path.ends_with(names), "names what changed: {}", e.path);
        assert!(copy.join("nested").exists(), "the tree the user was working in is still there");
        match change {
            "edit" => assert_eq!(std::fs::read_to_string(copy.join("nested/document.txt")).unwrap(), "the user's own work"),
            _ => assert!(copy.join("nested/new-work.txt").exists(), "the new file survives the refusal"),
        }
    }
}

// A failed copy's tree can hold a file something else wrote into it mid-copy, so undo must refuse rather than remove it.
#[test]
fn undoing_a_failed_copy_keeps_a_file_something_else_put_inside_it() {
    let _in_place = crate::backend::copystage_tests::in_place();
    let d = TestDir::new("undopartialforeign");
    let source = d.dir("source");
    std::fs::create_dir(source.join("nested")).unwrap();
    std::fs::write(source.join("nested/a.bin"), "x".repeat(8)).unwrap();
    std::fs::write(source.join("nested/b.bin"), "y".repeat(16)).unwrap();
    let identity = ItemIdentity::inspect(&source).unwrap();
    let copy = d.join("copy");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut planted = false;
    let mut sink = |_: u64, _: u64| {
        if planted {
            return;
        }
        planted = true;
        // Past a coarse-ctime kernel's tick, so the stray is provably newer than the copy's own folders.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let other = if copy.join("nested/a.bin").exists() { "b.bin" } else { "a.bin" };
        std::fs::write(copy.join("nested").join(other), "stray").unwrap();
    };
    let mut p = crate::backend::copyfile::Progress { cancel: &flag, on_bytes: &mut sink, tree: None, partial: None, manifest: None, durability: None, for_move: false };
    crate::backend::copyfile::copy_any(&source, &copy, &mut p).expect_err("the stray takes the second file's name");
    assert_eq!(p.partial, Some(copy.clone()), "the tree is the partial the journal records");
    let mut j = Journal::new();
    j.push(entry("copy", vec![copied(&source, &copy, identity).unwrap()]));
    j.undo().expect_err("the tree holds a file this operation never created");
    let strays = ["a.bin", "b.bin"].iter().filter(|name| std::fs::read_to_string(copy.join("nested").join(name)).ok().as_deref() == Some("stray")).count();
    assert_eq!(strays, 1, "the file something else wrote survives the undo");
}

// The control: a tree nobody touched is still the copy's own, so undo removes it the way it always did.
#[test]
fn undoing_a_copied_folder_nobody_touched_still_removes_it() {
    let d = TestDir::new("undotreeclean");
    let source = d.dir("source");
    std::fs::create_dir(source.join("nested")).unwrap();
    std::fs::write(source.join("nested/document.txt"), "as copied").unwrap();
    let copy = d.join("copy");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = crate::backend::copyfile::Progress { cancel: &flag, on_bytes: &mut sink, tree: None, partial: None, manifest: None, durability: None, for_move: false };
    crate::backend::copyfile::copy_any(&source, &copy, &mut p).expect("copy");
    let step = copied(&source, &copy, ItemIdentity::inspect(&source).unwrap()).unwrap();
    let mut j = Journal::new();
    j.push(entry("copy", vec![step]));
    j.undo().expect("an untouched copy is still this operation's to remove");
    assert!(!copy.exists(), "the copy is gone and the source is untouched");
    assert!(source.join("nested/document.txt").exists());
}

// The assumption the guard rests on, asserted rather than trusted: the copy writes the root's own mode
// last, so no descendant it wrote can carry a ctime past the one the journal records for that root. A
// filesystem too coarse to tell them apart answers equal, which is what this compares.
#[test]
fn a_copy_leaves_no_descendant_newer_than_the_root_it_records() {
    let d = TestDir::new("undotreeorder");
    let source = d.dir("source");
    std::fs::create_dir(source.join("nested")).unwrap();
    std::fs::write(source.join("nested/document.txt"), "as copied").unwrap();
    std::os::unix::fs::symlink("document.txt", source.join("nested/link")).unwrap();
    let copy = d.join("copy");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = crate::backend::copyfile::Progress { cancel: &flag, on_bytes: &mut sink, tree: None, partial: None, manifest: None, durability: None, for_move: false };
    crate::backend::copyfile::copy_any(&source, &copy, &mut p).expect("copy");
    let root = ItemIdentity::inspect(&copy).unwrap().changed;
    assert_eq!(newer_inside(&copy, root).unwrap(), None, "the copy's own tree is never newer than its root");
}

#[test]
fn undoing_a_duplicate_removes_only_the_copy_it_created() {
    let d = TestDir::new("undodup");
    let original = d.file("doc.txt", "original");
    let copy = d.file("doc copy.txt", "original");
    let mut j = Journal::new();
    j.push(entry("duplicate", vec![Step::Created { path: copy.clone() }]));
    d.assert_contains(&copy);
    j.undo().expect("undo");
    assert!(!copy.exists(), "the copy is gone");
    assert!(original.exists(), "the file it was copied from is untouched");
}

#[test]
fn undoing_a_created_directory_takes_its_contents_with_it() {
    let d = TestDir::new("undodir");
    let made = d.dir("copied-tree");
    std::fs::write(made.join("inside.txt"), "body").unwrap();
    let mut j = Journal::new();
    j.push(entry("copy", vec![Step::Created { path: made.clone() }]));
    d.assert_contains(&made);
    j.undo().expect("undo");
    assert!(!made.exists());
}

#[test]
fn the_steps_of_one_operation_reverse_newest_first() {
    let d = TestDir::new("undoorder");
    // A move recorded as two steps: undoing them out of order would leave b.txt where a.txt belongs.
    let a = d.file("a.txt", "a");
    let mut j = Journal::new();
    j.push(entry(
        "move",
        vec![
            moved(&d.join("first.txt"), &a, ItemIdentity::inspect(&a).unwrap()).unwrap(),
            Step::Created { path: d.file("second.txt", "s") },
        ],
    ));
    d.assert_contains(&d.join("second.txt"));
    j.undo().expect("undo");
    assert!(!d.join("second.txt").exists(), "the newest step reversed");
    assert!(d.join("first.txt").exists(), "the oldest step reversed too");
}

#[test]
fn a_step_that_fails_stops_the_rest_rather_than_half_reversing() {
    let d = TestDir::new("undofail");
    let mut j = Journal::new();
    j.push(entry(
        "copy",
        vec![
            Step::Created { path: d.file("keeper.txt", "k") },
            // Reversed first, and it cannot be: nothing is at this path to remove.
            Step::Created { path: d.join("never-existed.txt") },
        ],
    ));
    d.assert_contains(&d.join("keeper.txt"));
    d.assert_contains(&d.join("never-existed.txt"));
    let e = j.undo().expect_err("the missing path must fail");
    assert_eq!(e.where_, "undo");
    assert!(d.join("keeper.txt").exists(), "the step behind the failure was not reversed");
}

#[test]
fn two_journals_in_one_process_mint_different_nonce_high_halves() {
    let mut a = Journal::new();
    let mut b = Journal::new();
    // Distinct makers keep one backend from reattaching the other's manifest.
    assert_ne!(a.test_nonce() >> 32, b.test_nonce() >> 32);
}

#[test]
fn pushing_past_depth_drops_manifests_whose_entries_are_gone() {
    let d = TestDir::new("undoprune");
    // Manifest handles live off the destination, so each push records one live nonce.
    let runtime = d.path().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut j = Journal::new();
    j.attach_test_shared(runtime);
    for i in 0..DEPTH + 5 {
        let src = d.dir(&format!("s{i}"));
        std::fs::write(src.join("f"), "x").unwrap();
        let dst = d.path().join(format!("d{i}"));
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(dst.join("f"), "x").unwrap();
        let mut w = crate::backend::copymanifest::writer_for(&src, &dst).expect("writer");
        let meta = dst.join("f").symlink_metadata().unwrap();
        w.record(&dst.join("f"), &meta);
        let handle = w.finish().expect("finish").expect("a record");
        let step = copied_partial(&src, &dst, ItemIdentity::inspect(&src).unwrap(), Some(handle)).unwrap();
        j.push(entry(&format!("op{i}"), vec![step]));
    }
    // Only the entries the doc still holds keep their manifests.
    assert!(j.manifests.len() <= DEPTH, "pruned to the live entries, got {}", j.manifests.len());
}

#[test]
fn a_partial_mode_undo_that_restored_nothing_leaves_the_redo_clause_out() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("undopartialnone");
    let a = d.file("a.txt", "a");
    std::fs::set_permissions(&a, std::fs::Permissions::from_mode(0o644)).unwrap();
    let steps = crate::backend::permissions::apply_many(&[(a.clone(), "600".to_string())]).expect("one ordinary file");
    d.replace_file(&a, "replacement");
    std::fs::set_permissions(&a, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut j = Journal::new();
    j.push(entry("permissions", steps));
    let e = j.undo().expect_err("the one replaced path is skipped with a note");
    assert!(e.msg.starts_with("1 item was left in place: "), "{}", e.msg);
    assert!(!e.msg.contains("restored for redo"), "nothing was restored, so no clause says so: {}", e.msg);
}

#[test]
fn a_partial_mode_undo_keeps_the_restored_half_redoable() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("undopartialredo");
    let a = d.file("a.txt", "a");
    let b = d.file("b.txt", "b");
    let c = d.file("c.txt", "c");
    for p in [&a, &b, &c] {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let steps = crate::backend::permissions::apply_many(
        &[(a.clone(), "600".to_string()), (b.clone(), "600".to_string()), (c.clone(), "600".to_string())]).expect("three ordinary files");
    d.replace_file(&a, "replacement");
    std::fs::set_permissions(&a, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut j = Journal::new();
    j.push(entry("permissions", steps));
    let e = j.undo().expect_err("one replaced path is skipped with a note");
    assert!(e.msg.starts_with("1 item was left in place: "), "{}", e.msg);
    assert!(e.msg.ends_with("; 2 items were restored for redo"), "restored half unnamed: {}", e.msg);
    assert_eq!(b.metadata().unwrap().mode() & 0o777, 0o644, "the untouched files still restore");
    assert_eq!(c.metadata().unwrap().mode() & 0o777, 0o644, "the untouched files still restore");
    let (tx, _rx) = std::sync::mpsc::channel();
    assert_eq!(j.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx).unwrap(), "permissions");
    assert_eq!(b.metadata().unwrap().mode() & 0o777, 0o600, "redo reapplies the undone half");
    assert_eq!(c.metadata().unwrap().mode() & 0o777, 0o600, "redo reapplies the undone half");
    assert_eq!(std::fs::metadata(&a).unwrap().mode() & 0o777, 0o600, "the replacement keeps its mode");
}

#[test]
fn undoing_a_new_folder_removes_it_while_it_is_still_empty() {
    let d = TestDir::new("undomkdir");
    let made = d.dir("fresh");
    let mut j = Journal::new();
    j.push(entry("mkdir", vec![Step::MadeDir { path: made.clone(), identity: ItemIdentity::inspect(&made).unwrap() }]));
    d.assert_contains(&made);
    assert_eq!(j.undo().expect("undo"), "mkdir");
    assert!(!made.exists());
}

#[test]
fn undoing_a_new_folder_the_user_has_filled_refuses_and_keeps_what_is_inside() {
    let d = TestDir::new("undomkdirfilled");
    let made = d.dir("fresh");
    std::fs::write(made.join("theirs.txt"), "not ours to remove").unwrap();
    let mut j = Journal::new();
    j.push(entry("mkdir", vec![Step::MadeDir { path: made.clone(), identity: ItemIdentity::inspect(&made).unwrap() }]));
    d.assert_contains(&made);
    let e = j.undo().expect_err("must refuse rather than delete what the operation did not put there");
    assert_eq!(e.where_, "undo");
    assert_eq!(e.msg, "the new folder has been filled since, so undo left it in place");
    assert_eq!(std::fs::read_to_string(made.join("theirs.txt")).unwrap(), "not ours to remove");
    // Spent like every failed reversal, so the next undo reaches the operation before this one.
    assert!(j.is_empty());
}
