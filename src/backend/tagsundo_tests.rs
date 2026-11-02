// The settags undo and redo tests, beside the request tests so each file stays under the file budget.
use super::tests::{ops, state_for};
use super::*;
use crate::backend::testdir::TestDir;
use std::sync::mpsc::channel;

#[test]
fn a_redo_reapplies_what_undo_took_back() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsreqredo");
    let _file = d.file("a.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "add", "Green");
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), Vec::<String>::new());
    let (tx, _rx) = channel();
    o.journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx).expect("redo reapplies");
    assert_eq!(crate::backend::tags::read(&d.join("a.txt")).unwrap(), vec!["Green"]);
}

#[test]
fn undoing_a_toggled_removal_puts_the_tag_back() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsundoremoval");
    let file = d.file("plain.txt", "plain");
    let mut st = state_for(d.path());
    st.listing.push("plain.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "toggle", "Yellow");
    apply_tags(&mut out, &mut o, &st, vec![0], "toggle", "Yellow");
    assert_eq!(String::from_utf8(std::mem::take(&mut out)).unwrap(), settagged_line(1, "add") + "\n" + &settagged_line(1, "remove") + "\n");
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    assert!(String::from_utf8(out).unwrap().contains(r#"{"t":"undone","op":"settags","ok":true}"#));
    assert_eq!(crate::backend::tags::read(&file).unwrap(), vec!["Yellow"], "undo restores the tag the toggle removed");
}

#[test]
fn undo_leaves_a_file_whose_tags_changed_since_and_restores_the_rest() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsundochanged");
    let first = d.file("a.txt", "body");
    let second = d.file("b.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    st.listing.push("b.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0, 1], "add", "Red");
    crate::backend::tags::write_names(&first, &["Blue".to_string()]).unwrap();
    let mut out = Vec::new();
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""t":"error""#) && line.contains("left in place"), "{}", line);
    assert_eq!(crate::backend::tags::read(&first).unwrap(), vec!["Blue"], "a newer change survives undo");
    assert_eq!(crate::backend::tags::read(&second).unwrap(), Vec::<String>::new(), "the untouched file is restored");
}

#[test]
fn undo_leaves_a_file_replaced_by_another_carrying_the_same_tags() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsundoreplaced");
    let file = d.file("a.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "add", "Red");
    // Both files exist at once, so the replacement cannot reuse the old inode on any filesystem.
    let other = d.file("a.new", "other");
    crate::backend::tags::write_names(&other, &["Red".to_string()]).unwrap();
    std::fs::rename(&other, &file).unwrap();
    let mut out = Vec::new();
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    let line = String::from_utf8(out).unwrap();
    assert!(line.contains(r#""t":"error""#) && line.contains("left in place"), "{}", line);
    assert_eq!(crate::backend::tags::read(&file).unwrap(), vec!["Red"], "the replacement keeps its own tags");
}

// Longer than one tick of the coarse clock file times are stamped from (10 ms at HZ 100, the slowest common setting).
const COARSE_TICK_MS: u64 = 25;

#[test]
fn redo_skips_a_file_retagged_since_undo_and_refuses_a_replaced_one() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsredochanged");
    let file = d.file("a.txt", "body");
    let mut st = state_for(d.path());
    st.listing.push("a.txt", false);
    let (tx, _rx) = channel();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut o = ops();
    let mut out = Vec::new();
    apply_tags(&mut out, &mut o, &st, vec![0], "add", "Red");
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    // ctime moves on the kernel's coarse tick, so a retag that waits past one is the retag a person makes.
    std::thread::sleep(std::time::Duration::from_millis(COARSE_TICK_MS));
    crate::backend::tags::write_names(&file, &["Blue".to_string()]).unwrap();
    let skipped = o.journal.redo(1, &stop, &tx).expect_err("a retagged file is not overwritten");
    assert!(skipped.msg.starts_with("1 item was left in place: "), "the replay skips the file, never the source check's refusal: {}", skipped.msg);
    assert_eq!(crate::backend::tags::read(&file).unwrap(), vec!["Blue"]);
    // The replaced file is the middle step, so a redo that stopped at it in either order would already have written one.
    let (middle, last) = (d.file("b.txt", "middle"), d.file("c.txt", "last"));
    st.listing.push("b.txt", false);
    st.listing.push("c.txt", false);
    let mut o = ops();
    apply_tags(&mut out, &mut o, &st, vec![0, 1, 2], "add", "Red");
    crate::backend::opsdispatch::do_undo(&mut out, &mut o);
    let other = d.file("b.new", "other body");
    std::fs::rename(&other, &middle).unwrap();
    let refused = o.journal.redo(1, &stop, &tx).expect_err("a replaced file refuses the redo");
    assert!(refused.msg.contains("replaced"), "{}", refused.msg);
    assert_eq!(crate::backend::tags::read(&file).unwrap(), vec!["Blue"], "the refusal leaves every file of the entry as undo left it");
    assert_eq!(crate::backend::tags::read(&middle).unwrap(), Vec::<String>::new());
    assert_eq!(crate::backend::tags::read(&last).unwrap(), Vec::<String>::new(), "the refusal leaves every file of the entry as undo left it");
}

#[test]
fn redo_refuses_a_file_recreated_on_the_freed_inode() {
    use std::os::unix::fs::MetadataExt;
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for hold_original in [false, true] {
        let d = crate::backend::inodereuse::sandbox("tagsredorecreated");
        let file = d.file("a.txt", "body");
        let mut st = state_for(d.path());
        st.listing.push("a.txt", false);
        let (tx, _rx) = channel();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut o = ops();
        let mut out = Vec::new();
        apply_tags(&mut out, &mut o, &st, vec![0], "add", "Red");
        // Birth-time refusal is checked where available; recreated-file refusal below is unconditional.
        let meta = file.symlink_metadata().unwrap();
        if let Some(born) = crate::backend::permissions::born_of(&meta) {
            let (dev, ino) = (meta.dev(), meta.ino());
            let before = None;
            let after = Some(crate::backend::tags::join(&["Red".to_string()]));
            let different_birth = Some((born.0.wrapping_add(1), born.1));
            let saved = crate::backend::undo::Step::Tags { path: file.clone(), before: before.clone(), after: after.clone(), dev, ino, born: different_birth, generation: None };
            let mut steps = Vec::new();
            let err = crate::backend::tagsundo::replay(&file, &before, &after, (dev, ino), (different_birth, None), &saved, &mut steps)
                .expect_err("a step with a different birth time refuses");
            assert!(err.msg.contains("replaced"), "{}", err.msg);
            assert!(steps.is_empty(), "the refusal records nothing");
        }
        crate::backend::opsdispatch::do_undo(&mut out, &mut o);
        let mut held = None;
        let recreated = crate::backend::inodereuse::recreate(&d, &file, "other body", || {
            held = hold_original.then(|| std::fs::File::open(&file).unwrap());
            let mut o = ops();
            apply_tags(&mut out, &mut o, &st, vec![0], "add", "Red");
            crate::backend::opsdispatch::do_undo(&mut out, &mut o);
            let meta = file.symlink_metadata().unwrap();
            (o, (meta.dev(), meta.ino()), (crate::backend::permissions::born_of(&meta), crate::backend::filegen::of_path(&file)))
        });
        if hold_original {
            assert!(!recreated.reused_inode, "a held original forces the different-inode refusal arm");
        }
        let (mut o, old_id, old_birth) = recreated.saved;
        let meta = file.symlink_metadata().unwrap();
        if recreated.reused_inode {
            assert_eq!((meta.dev(), meta.ino()), old_id, "the new file really reuses the recorded inode");
            assert!(crate::backend::undo::born_differs(old_birth.0, crate::backend::permissions::born_of(&meta))
                || crate::backend::filegen::differs(old_birth.1, crate::backend::filegen::of_path(&file)), "the reused inode has a distinct birth or generation");
        } else {
            assert_ne!((meta.dev(), meta.ino()), old_id, "the fallback has a distinct inode");
        }
        assert_eq!(crate::backend::tags::read(&file).unwrap(), Vec::<String>::new(), "tags still match what undo left");
        let refused = o.journal.redo(1, &stop, &tx).expect_err("redo onto a recreated file refuses");
        assert!(refused.msg.contains("replaced"), "{}", refused.msg);
        assert_eq!(crate::backend::tags::read(&file).unwrap(), Vec::<String>::new(), "the new file stays untagged");
        drop(held);
    }
}

// The generation check itself never depends on the filesystem handing the inode back.
#[test]
fn redo_refuses_a_file_whose_inode_generation_differs_though_inode_and_birth_match() {
    use std::os::unix::fs::MetadataExt;
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = TestDir::new("tagsredogeneration");
    let file = d.file("a.txt", "body");
    let meta = file.symlink_metadata().unwrap();
    let (dev, ino, born) = (meta.dev(), meta.ino(), crate::backend::permissions::born_of(&meta));
    let Some(generation) = crate::backend::filegen::of_path(&file) else {
        eprintln!("skipped: the filesystem or sandbox gives no file handle");
        return;
    };
    let before = None;
    let after = Some(crate::backend::tags::join(&["Red".to_string()]));
    let saved = crate::backend::undo::Step::Tags { path: file.clone(), before: before.clone(), after: after.clone(), dev, ino, born, generation: Some(generation.wrapping_add(1)) };
    let mut steps = Vec::new();
    let err = crate::backend::tagsundo::replay(&file, &before, &after, (dev, ino), (born, Some(generation.wrapping_add(1))), &saved, &mut steps)
        .expect_err("a step with another inode generation refuses");
    assert_eq!(err.msg, "the file was replaced, so redo left its tags in place");
    assert!(steps.is_empty(), "the refusal records nothing");
    assert_eq!(crate::backend::tags::read(&file).unwrap(), Vec::<String>::new(), "the file stays untagged");
    let reverse = crate::backend::tagsundo::reverse(&file, &before, &after, (dev, ino), (born, Some(generation.wrapping_add(1))))
        .expect_err("undo refuses the same way");
    assert_eq!(reverse.msg, "the file was replaced, so undo left its tags in place");
    let same = crate::backend::undo::Step::Tags { path: file.clone(), before: before.clone(), after: after.clone(), dev, ino, born, generation: Some(generation) };
    crate::backend::tagsundo::replay(&file, &before, &after, (dev, ino), (born, Some(generation)), &same, &mut steps)
        .expect("the same generation still redoes");
    assert_eq!(crate::backend::tags::read(&file).unwrap(), vec!["Red"]);
}
