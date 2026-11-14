// Merge folders on paste: nested trees, keep-both naming, replace via trash, undo, partial failure, move.
use super::*;
use crate::backend::collide::{Ask, Policy, Question};
use crate::backend::opsreq::{run_transfer_checked, OpMsg};
use crate::backend::testdir::TestDir;
use crate::backend::trash;
use crate::backend::undo::{Entry, ItemIdentity, Journal};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;

const EXIT_CODE_SHIFT: i32 = 8;

fn exited(code: i32, stdout: String) -> Output {
    Output { status: ExitStatus::from_raw(code << EXIT_CODE_SHIFT), stdout: stdout.into_bytes(), stderr: Vec::new() }
}

// gio's trash, played by a folder inside the sandbox: entry n sits at can/n with its original path beside it.
fn gio_trash(can: &Path, args: &[&str]) -> Output {
    match args {
        ["trash", "--list"] => {
            let mut listed = String::new();
            for item in std::fs::read_dir(can).unwrap().flatten() {
                let name = item.file_name().to_string_lossy().to_string();
                if let Some(n) = name.strip_suffix(".original") {
                    listed.push_str(&format!("trash:///{}\t{}\n", n, std::fs::read_to_string(item.path()).unwrap()));
                }
            }
            exited(0, listed)
        }
        ["trash", "--restore", uri] => {
            let n = uri.strip_prefix("trash:///").unwrap();
            let original = PathBuf::from(std::fs::read_to_string(can.join(format!("{}.original", n))).unwrap());
            if original.symlink_metadata().is_ok() {
                return exited(1, String::new());
            }
            std::fs::rename(can.join(n), &original).unwrap();
            std::fs::remove_file(can.join(format!("{}.original", n))).unwrap();
            exited(0, String::new())
        }
        ["trash", "--", paths @ ..] => {
            for (i, path) in paths.iter().enumerate() {
                let n = format!("{}-{}", std::process::id(), i);
                std::fs::rename(path, can.join(&n)).unwrap();
                std::fs::write(can.join(format!("{}.original", n)), path).unwrap();
            }
            exited(0, String::new())
        }
        _ => exited(1, String::new()),
    }
}

struct StandIn;
impl Drop for StandIn {
    fn drop(&mut self) {
        trash::STAND_IN.with(|slot| *slot.borrow_mut() = None);
    }
}

fn working_trash(d: &TestDir) -> StandIn {
    let can = d.dir("can");
    let at = can.clone();
    trash::STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| Some(gio_trash(&at, args)))));
    StandIn
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn owned(paths: &[&Path]) -> Vec<String> {
    paths.iter().map(|p| p.to_string_lossy().to_string()).collect()
}

// The combined question as a merge transfer sees it: top-level names plus one pre-write walk.
fn question_for(id: usize, paths: &[&Path], dest: &Path) -> Question {
    let mut seen = HashMap::new();
    for src in paths {
        let name = src.file_name().unwrap().to_str().unwrap();
        let dst_top = dest.join(name);
        if let Ok(there) = ItemIdentity::inspect(&dst_top) {
            seen.insert(src.to_path_buf(), there);
        }
    }
    let outer = Question { id, dest: dest.to_path_buf(), seen, twins: Default::default(), menu: None, fat: None };
    combine(outer, walk(&owned(paths), dest, None))
}

fn policy(word: &str, inner: &str, question: Question, dest: &Path) -> Policy {
    let id = question.id;
    Ask { word: Some(word.to_string()), id, inner: Some(inner.to_string()), fat: None }.policy(Some(question), dest)
}

fn merged_policy(id: usize, paths: &[&Path], dest: &Path, inner: &str) -> Policy {
    policy("merge", inner, question_for(id, paths, dest), dest)
}

// Runs one merge transfer on this thread and answers its counts, entry, totals and folder.
fn transfer(moving: bool, paths: &[&Path], dest: &Path, policy: Policy) -> (usize, usize, usize, Vec<String>, Entry, usize, usize, String) {
    let (tx, rx) = channel();
    run_transfer_checked(1, moving, owned(paths), dest.to_path_buf(), AtomicBool::new(false).into(), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: policy });
    let mut errors = Vec::new();
    let mut out = (0, 0, 0, Vec::new(), Entry { op: String::new(), steps: Vec::new() }, 0, 0, String::new());
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: false, err, .. } => errors.push(err),
            OpMsg::TransferDone { ok, failed, skipped, entry, .. } => out = (ok, failed, skipped, errors.clone(), entry, 0, 0, String::new()),
            OpMsg::MergeDone { ok, failed, skipped, entry, merged, kept_both, into, .. } => {
                out = (ok, failed, skipped, errors.clone(), entry, merged, kept_both, into)
            }
            _ => {}
        }
    }
    out
}

#[test]
fn move_merge_closes_an_open_batch_first_so_items_report_in_index_order() {
    let _force = crate::backend::movebatch::ForceCopyGuard::hold();
    let d = TestDir::new("merge-batch-order");
    let to = d.dir("to");
    d.dir("from");
    let a = d.file("from/a.txt", "a");
    let b = d.file("from/b.txt", "b");
    let trip = d.dir("from/trip");
    d.file("from/trip/x.txt", "x");
    d.dir("to/trip");
    let (tx, rx) = channel();
    run_transfer_checked(10, true, owned(&[&a, &b, &trip]), to.clone(), AtomicBool::new(false).into(), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: merged_policy(7, &[&a, &b, &trip], &to, "skip") });
    let mut order = Vec::new();
    for msg in rx.iter() {
        if let OpMsg::Item { index, ok, .. } = msg {
            order.push((index, ok));
        }
    }
    assert_eq!(order, vec![(0, true), (1, true), (2, true)], "items report in index order: {:?}", order);
}

#[test]
fn merge_of_nested_trees_copies_missing_and_asks_inner_files_once() {
    let d = TestDir::new("merge-nested");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/a.txt", "yours");
    d.dir("from/trip/sub");
    d.file("from/trip/sub/b.txt", "yours-b");
    d.dir("to/trip");
    d.file("to/trip/a.txt", "there");
    d.file("to/trip/c.txt", "stays");
    let (ok, failed, _, _, _, merged, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    assert_eq!((ok, failed), (1, 0), "the folder lands");
    assert_eq!(text(&to.join("trip/sub/b.txt")), "yours-b", "everything missing is copied in");
    assert_eq!(text(&to.join("trip/c.txt")), "stays", "files already there are left alone");
    assert_eq!(text(&to.join("trip/a.txt")), "there", "no answer ever overwrites without it");
    assert_eq!(merged, 3, "the status counts what landed: {}", merged);
}

#[test]
fn merge_keep_both_names_the_inner_copy_as_duplicate_does() {
    let d = TestDir::new("merge-keep");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/photo.png", "yours");
    d.dir("to/trip");
    d.file("to/trip/photo.png", "there");
    let (ok, _, _, _, _, _, kept, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    assert_eq!(ok, 1);
    assert_eq!(text(&to.join("trip/photo copy.png")), "yours", "Keep both names the copy as Duplicate does");
    assert_eq!(text(&to.join("trip/photo.png")), "there");
    assert_eq!(kept, 1, "the status counts what was kept both");
}

#[test]
fn merge_replace_trashes_the_old_file_first_and_undo_restores_it() {
    let d = TestDir::new("merge-replace");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/photo.png", "yours");
    d.dir("to/trip");
    let there = d.file("to/trip/photo.png", "there");
    let _trash = working_trash(&d);
    let (ok, failed, _, _, entry, _, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "replace"));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(text(&there), "yours");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    journal.undo().unwrap();
    assert_eq!(text(&there), "there", "one undo takes the new file away and restores the old one");
}

#[test]
fn merge_undo_removes_only_what_the_merge_created() {
    let d = TestDir::new("merge-undo");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/new.txt", "yours");
    d.file("from/trip/a.txt", "yours-a");
    d.dir("to/trip");
    d.file("to/trip/a.txt", "there");
    d.file("to/trip/old.txt", "stays");
    let (_, _, _, _, entry, _, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&to.join("trip/new.txt"));
    journal.undo().unwrap();
    assert!(!to.join("trip/new.txt").exists(), "what the merge created goes");
    assert!(!to.join("trip/a copy.txt").exists(), "and the kept-both copy with it");
    assert_eq!(text(&to.join("trip/a.txt")), "there", "files already in the destination are never touched by undo");
    assert_eq!(text(&to.join("trip/old.txt")), "stays");
}

#[test]
fn merge_that_fails_part_way_keeps_what_landed_and_reports_the_rest_by_name() {
    let d = TestDir::new("merge-partial");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/good.txt", "yours");
    let shut = d.file("from/trip/shut.txt", "unreadable");
    d.dir("to/trip");
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (ok, failed, _, errors, _, merged, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(text(&to.join("trip/good.txt")), "yours", "a merge that fails part way keeps what landed");
    assert_eq!(failed, 1, "ok={}, failed={}, errors={:?}, merged={}", ok, failed, errors, merged);
    assert!(errors.iter().any(|e| e.contains("shut.txt")), "the rest is reported by name: {:?}", errors);
}

#[test]
fn move_merge_then_undo_restores_emptied_folders_with_their_modes() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("merge-undo-folders");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/a.txt", "yours-a");
    let sub = d.dir("from/trip/sub");
    d.file("from/trip/sub/b.txt", "yours-b");
    d.dir("to/trip");
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o750)).unwrap();
    std::fs::set_permissions(&album, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (ok, failed, _, _, entry, _, _, _) = transfer(true, &[&album], &to, merged_policy(7, &[&album], &to, "skip"));
    assert_eq!((ok, failed), (1, 0));
    assert!(!album.exists(), "the merge emptied the source folder");
    let mut journal = Journal::new();
    journal.push(entry);
    journal.undo().expect("undo brings the moved entries and their folders back");
    assert_eq!(text(&d.join("from/trip/a.txt")), "yours-a", "every source entry is restored");
    assert_eq!(text(&d.join("from/trip/sub/b.txt")), "yours-b");
    assert_eq!(sub.symlink_metadata().unwrap().permissions().mode() & 0o777, 0o750, "an emptied folder keeps its mode");
    assert_eq!(album.symlink_metadata().unwrap().permissions().mode() & 0o777, 0o700);
    let (tx, _rx) = channel();
    journal.redo(1, &AtomicBool::new(false), &tx).expect("redo repeats the merge move");
    assert!(!album.exists(), "redo takes the emptied folders away again");
    assert_eq!(text(&to.join("trip/sub/b.txt")), "yours-b", "and the entries are back where the merge put them");
}

#[test]
fn plain_move_undo_refuses_when_the_source_folder_is_gone() {
    let d = TestDir::new("merge-undo-gone-parent");
    let to = d.dir("to");
    d.dir("from");
    let lone = d.file("from/lone.txt", "yours");
    let from = d.join("from");
    let plain = Ask { word: None, id: 0, inner: None, fat: None }.policy(None, &to);
    let (ok, failed, _, _, entry, _, _, _) = transfer(true, &[&lone], &to, plain);
    assert_eq!((ok, failed), (1, 0));
    std::fs::remove_dir_all(&from).unwrap();
    let mut journal = Journal::new();
    journal.push(entry);
    let _err = journal.undo().expect_err("undo must not recreate a deleted source folder on the wrong disk");
    assert!(!from.exists(), "the refusal creates nothing");
    assert_eq!(text(&to.join("lone.txt")), "yours", "the moved file stays where the move put it");
}

#[test]
fn plain_move_undo_goes_back_through_a_symlinked_folder_name() {
    let d = TestDir::new("merge-undo-link-parent");
    let to = d.dir("to");
    d.dir("real");
    let link = d.join("link");
    std::os::unix::fs::symlink(d.join("real"), &link).unwrap();
    std::fs::write(d.join("real/lone.txt"), "yours").unwrap();
    let lone = link.join("lone.txt");
    let plain = Ask { word: None, id: 0, inner: None, fat: None }.policy(None, &to);
    let (ok, failed, _, _, entry, _, _, _) = transfer(true, &[&lone], &to, plain);
    assert_eq!((ok, failed), (1, 0));
    let mut journal = Journal::new();
    journal.push(entry);
    journal.undo().expect("a folder reached through a symlinked name is still there");
    assert_eq!(text(&d.join("real/lone.txt")), "yours", "the file is back where it came from");
}

#[test]
fn a_move_merge_touches_no_source_folder_of_its_own() {
    // Each rename touches what its own leg needs; the merge layer adds no touch of its own.
    crate::backend::durable::test_reset();
    let d = TestDir::new("merge-no-src-touch");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/a.txt", "yours-a");
    d.file("from/trip/b.txt", "yours-b");
    d.dir("to/trip");
    crate::backend::durable::test_mark_durable(&to);
    let (ok, failed, _, _, _, _, _, _) = transfer(true, &[&album], &to, merged_policy(7, &[&album], &to, "skip"));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(crate::backend::durable::test_touch_calls(), 4, "two renames and no merge-layer touch");
    crate::backend::durable::test_reset();
}

#[test]
fn a_move_merge_onto_a_durable_destination_still_confirms_both_parents() {
    // The removal above must not take the destination confirm with it: a rename rewrote two entries.
    crate::backend::durable::test_reset();
    let d = TestDir::new("merge-dest-confirm");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/a.txt", "yours-a");
    d.dir("to/trip");
    crate::backend::durable::test_mark_durable(&to);
    let (ok, failed, _, _, _, _, _, _) = transfer(true, &[&album], &to, merged_policy(7, &[&album], &to, "skip"));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(crate::backend::durable::test_counts(), (0, 2), "no file bytes to fsync, both parents flushed");
    crate::backend::durable::test_reset();
}

#[test]
fn a_single_merging_folder_names_itself_on_the_terminal_line() {
    let d = TestDir::new("merge-into-one");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    d.file("from/trip/a.txt", "yours");
    d.dir("to/trip");
    let (ok, failed, _, _, _, _, _, into) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(into, "trip", "the status names the folder that merged: {}", into);
}

#[test]
fn several_merging_folders_name_the_destination_on_the_terminal_line() {
    let d = TestDir::new("merge-into-many");
    let to = d.dir("Picts");
    let one = d.dir("from/a");
    let two = d.dir("from/b");
    d.file("from/a/x.txt", "x");
    d.file("from/b/y.txt", "y");
    d.dir("Picts/a");
    d.dir("Picts/b");
    let policy = merged_policy(7, &[&one, &two], &to, "keep");
    let (ok, failed, _, _, _, _, _, into) = transfer(false, &[&one, &two], &to, policy);
    assert_eq!((ok, failed), (2, 0));
    assert_eq!(into, "Picts", "several folders name the destination they folded into: {}", into);
}

#[test]
fn merge_never_descends_into_a_symlinked_folder() {
    // d_type calls a symlink not-a-dir, the way the lstat did: the link lands as a link and counts once.
    let d = TestDir::new("merge-symlink");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    let real = d.dir("real");
    d.file("real/inner.txt", "deep");
    std::os::unix::fs::symlink(&real, album.join("link")).unwrap();
    d.dir("to/trip");
    let (ok, failed, _, _, _, merged, _, _) = transfer(false, &[&album], &to, merged_policy(7, &[&album], &to, "keep"));
    assert_eq!((ok, failed), (1, 0));
    let landed = to.join("trip/link");
    assert!(landed.symlink_metadata().unwrap().file_type().is_symlink(), "a symlinked folder lands as a link");
    // The link lands as one link: the status counts what landed, not its target.
    assert_eq!(merged, 1, "one landed link counts once: {}", merged);
}

#[test]
fn move_merge_removes_only_sources_that_landed() {
    let d = TestDir::new("merge-move");
    let to = d.dir("to");
    let album = d.dir("from/trip");
    let gone = d.file("from/trip/gone.txt", "yours");
    d.file("from/trip/a.txt", "yours-a");
    d.dir("to/trip");
    d.file("to/trip/a.txt", "there");
    let (ok, _, _, _, _, _, _, _) = transfer(true, &[&album], &to, merged_policy(7, &[&album], &to, "skip"));
    assert_eq!(ok, 1);
    assert!(!gone.exists(), "a source entry is removed only after its destination landed");
    assert_eq!(text(&to.join("trip/gone.txt")), "yours");
    assert!(d.join("from/trip/a.txt").exists(), "an entry the answer left in place stays where it was");
    assert_eq!(text(&to.join("trip/a.txt")), "there");
}

#[path = "mergefix_tests.rs"]
mod fixes;

#[path = "mergefat_tests.rs"]
mod fat_names;
