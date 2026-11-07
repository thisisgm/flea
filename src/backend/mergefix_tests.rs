// Review fixes for merge folders, and the pins that guard them.
use super::*;
use crate::backend::undo::Step;

#[test]
fn a_failure_in_one_subfolder_leaves_the_next_sibling_to_finish_and_be_journalled() {
    let d = TestDir::new("merge-sibling");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    for sub in ["sub1", "sub2"] {
        d.dir(&format!("from/trip/{sub}"));
        d.file(&format!("from/trip/{sub}/new.txt"), sub);
        d.dir(&format!("to/trip/{sub}"));
    }
    // The listing order decides which sibling is walked first, so the shut one is whichever comes first.
    let order: Vec<String> = std::fs::read_dir(&trip).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    let (bad, fine) = (order[0].clone(), order[1].clone());
    let shut = d.join(&format!("to/trip/{bad}"));
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o500)).unwrap();
    let (ok, failed, _, _, entry, _, _, _) = transfer(true, &[&trip], &to, merged_policy(7, &[&trip], &to, "keep"));
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!((ok, failed), (0, 1), "the fold reports its one failure");
    assert!(trip.join(&bad).join("new.txt").exists(), "the entry that failed stays in its source folder");
    assert!(!trip.join(&fine).exists(), "the sibling that landed completely is removed from the source");
    let removed = |step: &Step| matches!(step, Step::RemovedEmptyDir { path, .. } if *path == trip.join(&fine));
    assert!(entry.steps.iter().any(removed), "and its removal is journalled so undo recreates it");
}

#[test]
fn walk_names_inner_files_for_the_one_folder_that_merges() {
    let d = TestDir::new("merge-walk-single");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.file("from/trip/a.txt", "x");
    let loose = d.file("from/x.txt", "y");
    d.dir("to/trip");
    d.file("to/trip/a.txt", "there");
    d.file("to/x.txt", "z");
    let rels: Vec<String> = walk(&owned(&[&trip, &loose]), &to, None).iter().map(|f| f.rel.clone()).collect();
    assert_eq!(rels, vec!["a.txt".to_string(), "x.txt".to_string()], "one merging folder names its own inners: {:?}", rels);
}

#[test]
fn copy_and_move_merge_count_the_same_entries() {
    for moving in [false, true] {
        let _force = crate::backend::movebatch::ForceCopyGuard::hold();
        let d = TestDir::new(if moving { "merge-counts-move" } else { "merge-counts-copy" });
        let to = d.dir("to");
        let trip = d.dir("from/trip");
        d.file("from/trip/x.txt", "x");
        let photo = d.file("from/photo.png", "new");
        d.dir("to/trip");
        let (ok, failed, _, _, _, merged, kept, _) = transfer(moving, &[&trip, &photo], &to,
            merged_policy(7, &[&trip, &photo], &to, "keep"));
        assert_eq!((ok, failed), (2, 0), "moving={}", moving);
        assert_eq!((merged, kept), (2, 0), "copy and move count alike, moving={}", moving);
    }
}

#[test]
fn top_level_keep_counts_on_copy_and_on_move() {
    for moving in [false, true] {
        let _force = crate::backend::movebatch::ForceCopyGuard::hold();
        let d = TestDir::new(if moving { "merge-keep-move" } else { "merge-keep-copy" });
        let to = d.dir("to");
        let trip = d.dir("from/trip");
        d.file("from/trip/x.txt", "x");
        let photo = d.file("from/photo.png", "yours");
        d.dir("to/trip");
        d.file("to/photo.png", "there");
        let (ok, failed, _, _, _, merged, kept, _) = transfer(moving, &[&trip, &photo], &to,
            merged_policy(7, &[&trip, &photo], &to, "keep"));
        assert_eq!((ok, failed), (2, 0), "moving={}", moving);
        assert_eq!((merged, kept), (2, 1), "a top-level keep counts, moving={}", moving);
    }
}

#[test]
fn merge_undo_from_another_window_refuses_rather_than_removing_the_destination() {
    let d = TestDir::new("merge-undo-foreign");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/new.txt", "yours");
    d.dir("to/trip");
    let there = d.file("to/trip/old.txt", "stays");
    let (_, _, _, _, entry, _, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    // A restart leaves only the nonce in the shared doc; no in-memory manifest survives it.
    let mut foreign = entry.clone();
    for step in &mut foreign.steps {
        if let Step::Copied { manifest, .. } = step {
            *manifest = None;
        }
    }
    let mut journal = Journal::new();
    journal.push(foreign);
    let err = journal.undo().expect_err("undo without the manifest must refuse a merge");
    assert!(err.msg.contains("only in the window that made it"), "{}", err.msg);
    assert_eq!(text(&there), "stays", "the pre-existing destination folder survives");
    assert!(to.join("trip/new.txt").exists(), "what the merge created stays too");
}

#[test]
fn a_merge_step_keeps_its_flag_through_the_shared_doc_and_a_plain_copy_carries_none() {
    let d = TestDir::new("merge-codec");
    let id = ItemIdentity::inspect(d.path()).unwrap();
    let step = |merge| Step::Copied { from: d.join("a"), to: d.join("b"), source: id.clone(), created: id.clone(), manifest: None, manifest_nonce: Some(7), merge, rename_fat: false };
    let doc = crate::backend::undoshare::Doc {
        undo: vec![Entry {
            op: "copy".into(),
            steps: vec![step(true), step(false)],
        }],
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    let text = crate::jsondoc::render(&crate::backend::undocodec::encode(&doc));
    assert_eq!(text.matches("\"merge\"").count(), 1, "only the merge top carries the flag: {text}");
    let back = crate::backend::undocodec::decode(&text).expect("the doc decodes");
    let flags: Vec<bool> = back.undo[0].steps.iter().map(|s| matches!(s, Step::Copied { merge: true, .. })).collect();
    assert_eq!(flags, [true, false], "an older journal without the key decodes as a plain copy");
}

#[test]
fn redo_of_a_copy_merge_refuses_and_leaves_the_destination_folder() {
    let d = TestDir::new("merge-redo");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/new.txt", "yours");
    d.dir("to/trip");
    let there = d.file("to/trip/old.txt", "stays");
    let (_, _, _, _, entry, _, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    let mut journal = Journal::new();
    journal.push(entry);
    journal.undo().expect("undo takes away what the merge made");
    let (tx, _rx) = channel();
    let err = journal.redo(1, &AtomicBool::new(false), &tx).expect_err("redo never replays a merge over its folder");
    assert_eq!(err.msg, "an item with that name is already there, so redo left it in place");
    assert_eq!(text(&there), "stays", "the pre-existing destination file survives the refusal");
}

#[test]
fn merge_undo_with_an_unverified_manifest_refuses_rather_than_removing_the_destination() {
    let d = TestDir::new("merge-undo-unverified");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/new.txt", "yours");
    d.dir("to/trip");
    let there = d.file("to/trip/old.txt", "stays");
    // A damaged manifest stream, which remove_owned answers with Fallback instead of a report.
    let handle = crate::backend::copymanifest::test_unverified(&to.join("trip"));
    let source = ItemIdentity::inspect(&album).unwrap();
    let step = crate::backend::undo::merged_partial(&album, &to.join("trip"), source, Some(handle)).expect("a merge top step");
    let mut journal = Journal::new();
    journal.push(Entry { op: "merge".into(), steps: vec![step] });
    let err = journal.undo().expect_err("undo with an unverified manifest must refuse a merge");
    assert!(err.msg.contains("only in the window that made it"), "{}", err.msg);
    assert_eq!(text(&there), "stays", "the pre-existing destination folder survives");
}

#[test]
fn an_unreadable_source_subfolder_fails_the_merge_and_names_it() {
    let d = TestDir::new("merge-unreadable-src");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    let sub = d.dir("from/trip/sub");
    d.file("from/trip/sub/b.txt", "yours-b");
    d.file("from/trip/top.txt", "top");
    d.dir("to/trip");
    d.dir("to/trip/sub");
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (ok, failed, _, errors, _, merged, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!((ok, failed), (0, 1), "a subfolder that cannot be listed fails its folder: errors={:?} merged={}", errors, merged);
    assert_eq!(text(&to.join("trip/top.txt")), "top", "what landed before the failure stays");
    assert!(errors.iter().any(|e| e.contains("sub")), "the subfolder is reported by name: {:?}", errors);
}

#[test]
fn a_failed_batch_confirm_keeps_deferred_items_out_of_the_merge_count() {
    use crate::backend::durable;
    let _force = crate::backend::movebatch::ForceCopyGuard::hold();
    let d = TestDir::new("merge-confirm-fail");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.file("from/trip/x.txt", "x");
    let loose = d.file("from/loose.txt", "yours");
    d.dir("to/trip");
    d.file("to/loose.txt", "there");
    let body = format!("21 1 8:1 / {} rw,flush - vfat /dev/test rw,flush\n", to.display());
    durable::test_mark_durable(&to);
    durable::test_set_fake_mountinfo(Some(&body));
    durable::test_set_syncfs_errno(5);
    let (ok, failed, _, errors, _, merged, kept, _) = transfer(true, &[&trip, &loose], &to, merged_policy(7, &[&trip, &loose], &to, "keep"));
    durable::test_reset();
    assert_eq!((ok, failed), (1, 1), "the folder lands, the deferred file fails its confirm: errors={:?} merged={}", errors, merged);
    assert_eq!((merged, kept), (1, 0), "only the landed inner entry counts: errors={:?}", errors);
    assert!(loose.exists(), "the unconfirmed source stays whole");
}

#[path = "mergereplace_tests.rs"]
mod replace;

#[path = "mergestage_tests.rs"]
mod staged;
