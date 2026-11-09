// A FAT answer on the policy remaps or skips each leaf before the collision rules see it, driven against real files in a sandbox.
use super::{text, transfer};
use crate::backend::undo::Journal;
use crate::backend::collide::*;
use crate::backend::fatnames::Choice;
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};

const EEXIST: i32 = 17;

fn fat_policy(choice: Choice, on_fat: bool) -> Policy {
    Policy { fat: Some(choice), on_fat, ..Policy::default() }
}

fn sources(d: &TestDir) -> Vec<PathBuf> {
    d.dir("from");
    ["clip: final.mp4", "notes?.md", "draft.", "final.mp4"].iter().map(|name| d.file(&format!("from/{}", name), "yours")).collect()
}

#[test]
fn rename_and_copy_lands_each_bad_name_under_its_new_one() {
    let d = TestDir::new("fat-rename");
    let to = d.dir("to");
    let from = sources(&d);
    let paths: Vec<&Path> = from.iter().map(|p| p.as_path()).collect();
    let (ok, failed, skipped, errors, _) = transfer(false, &paths, &to, fat_policy(Choice::Rename, true));
    assert_eq!((ok, failed, skipped, errors.len()), (4, 0, 0, 0));
    for landed in ["clip_ final.mp4", "notes_.md", "draft", "final.mp4"] {
        assert_eq!(text(&to.join(landed)), "yours", "{} landed", landed);
    }
    assert!(from.iter().all(|p| p.exists()), "a copy leaves every source in place");
}

#[test]
fn skip_them_copies_the_rest_and_writes_nothing_for_a_bad_name() {
    let d = TestDir::new("fat-skip");
    let to = d.dir("to");
    let from = sources(&d);
    let paths: Vec<&Path> = from.iter().map(|p| p.as_path()).collect();
    let (ok, failed, skipped, _, entry) = transfer(false, &paths, &to, fat_policy(Choice::Skip, true));
    assert_eq!((ok, failed, skipped), (1, 0, 3), "only the clean name is copied");
    assert_eq!(std::fs::read_dir(&to).unwrap().count(), 1);
    assert_eq!(entry.steps.len(), 1, "undo has exactly the one file to take back");
}

#[test]
fn a_destination_off_the_fat_family_keeps_every_name_as_asked() {
    let d = TestDir::new("fat-ext4");
    let to = d.dir("to");
    let from = sources(&d);
    let paths: Vec<&Path> = from.iter().map(|p| p.as_path()).collect();
    let (ok, _, skipped, _, _) = transfer(false, &paths, &to, fat_policy(Choice::Rename, false));
    assert_eq!((ok, skipped), (4, 0));
    assert!(to.join("clip: final.mp4").exists() && to.join("draft.").exists(), "nothing is renamed on a guess");
}

// Stands vfat's refusal on the sandbox: the create of each raw name answers EINVAL, as a FAT mount does and a tmpfs does not.
struct RefusesFatNames;
impl RefusesFatNames {
    fn arm(raw: &[&str]) -> RefusesFatNames {
        let faults: Vec<(&str, i32)> = raw.iter().map(|name| (*name, crate::backend::durable::EINVAL)).collect();
        RefusesFatNames::arm_with(&faults)
    }
    fn arm_with(faults: &[(&str, i32)]) -> RefusesFatNames {
        crate::backend::unheld::test_refuse(faults);
        RefusesFatNames
    }
}
impl Drop for RefusesFatNames {
    fn drop(&mut self) {
        crate::backend::unheld::test_refuse(&[]);
        crate::backend::unheld::test_force_linkless(false);
    }
}

// Sample tree: from/trip holds ok.txt, "clip: x.mp4" and a folder "sub: a" holding "b?.txt".
fn trip_with_bad_names(d: &TestDir) -> PathBuf {
    let trip = d.dir("from/trip");
    d.file("from/trip/ok.txt", "plain");
    d.file("from/trip/clip: x.mp4", "inside");
    d.dir("from/trip/sub: a");
    d.file("from/trip/sub: a/b?.txt", "deeper");
    trip
}

#[test]
fn rename_and_copy_renames_the_names_inside_a_folder_too() {
    let d = TestDir::new("fat-rename-inside");
    let to = d.dir("to");
    let trip = trip_with_bad_names(&d);
    let _fat = RefusesFatNames::arm(&["clip: x.mp4", "sub: a", "b?.txt"]);
    let (ok, failed, skipped, errors, _) = transfer(false, &[&trip], &to, fat_policy(Choice::Rename, true));
    assert_eq!((ok, failed, skipped, errors.len()), (1, 0, 0, 0), "{errors:?}");
    assert_eq!(text(&to.join("trip/clip_ x.mp4")), "inside", "the file inside lands under the name Rename gives a top-level file");
    assert_eq!(text(&to.join("trip/sub_ a/b_.txt")), "deeper", "a folder and the file in it are renamed alike");
    assert_eq!(text(&to.join("trip/ok.txt")), "plain");
    assert!(to.join("trip/clip: x.mp4").symlink_metadata().is_err() && to.join("trip/sub: a").symlink_metadata().is_err(), "no raw name landed");
    assert!(trip.join("clip: x.mp4").exists(), "a copy leaves the source names as they were");
}

#[test]
fn skip_them_leaves_the_names_inside_a_folder_as_they_are() {
    let d = TestDir::new("fat-skip-inside");
    let to = d.dir("to");
    let trip = trip_with_bad_names(&d);
    let _fat = RefusesFatNames::arm(&["clip: x.mp4", "sub: a"]);
    crate::backend::unheld::test_force_linkless(true);
    let (ok, failed, _, errors, _) = transfer(false, &[&trip], &to, fat_policy(Choice::Skip, true));
    assert_eq!((ok, failed), (0, 1), "Skip skips the listed top-level items only, so the folder still lands and names what it could not hold");
    assert!(errors[0].contains("2 items inside could not be copied"), "{errors:?}");
    assert_eq!(text(&to.join("trip/ok.txt")), "plain");
    assert!(to.join("trip/clip_ x.mp4").symlink_metadata().is_err(), "Skip never renames");
}

#[test]
fn a_destination_off_the_fat_family_keeps_the_names_inside_a_folder() {
    let d = TestDir::new("fat-ext4-inside");
    let to = d.dir("to");
    let trip = trip_with_bad_names(&d);
    let (ok, failed, _, _, _) = transfer(false, &[&trip], &to, fat_policy(Choice::Rename, false));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(text(&to.join("trip/clip: x.mp4")), "inside");
    assert_eq!(text(&to.join("trip/sub: a/b?.txt")), "deeper", "nothing is renamed on a guess");
}

#[test]
fn two_names_inside_a_folder_that_sanitize_alike_land_one_and_name_the_other() {
    let d = TestDir::new("fat-collide-inside");
    let to = d.dir("to");
    let trip = d.dir("from/trip");
    d.file("from/trip/a:b.txt", "colon");
    d.file("from/trip/a_b.txt", "underscore");
    let _fat = RefusesFatNames::arm(&["a:b.txt"]);
    crate::backend::unheld::test_force_linkless(true);
    let (ok, failed, _, errors, _) = transfer(false, &[&trip], &to, fat_policy(Choice::Rename, true));
    assert_eq!((ok, failed), (0, 1), "{errors:?}");
    assert!(errors[0].contains("same as another"), "the drive's own taken-name sentence, not a bare already-exists: {errors:?}");
    assert!(matches!(text(&to.join("trip/a_b.txt")).as_str(), "colon" | "underscore"), "exactly one of the two landed under the shared name");
}

// Sample input: a:b.txt is refused and a_b.txt answers EEXIST as if another writer planted it; without Rename the first never lands as the second.
#[test]
fn a_taken_name_is_a_fold_only_where_rename_wrote_the_sanitized_name() {
    for answer in [None, Some(Choice::Skip)] {
        let d = TestDir::new("fat-planted-inside");
        let to = d.dir("to");
        let trip = d.dir("from/trip");
        d.file("from/trip/a:b.txt", "colon");
        d.file("from/trip/a_b.txt", "underscore");
        let _fat = RefusesFatNames::arm_with(&[("a:b.txt", crate::backend::durable::EINVAL), ("a_b.txt", EEXIST)]);
        crate::backend::unheld::test_force_linkless(true);
        let policy = answer.map_or_else(Policy::default, |choice| fat_policy(choice, true));
        let (ok, failed, _, errors, _) = transfer(false, &[&trip], &to, policy);
        assert_eq!((ok, failed), (0, 1), "{answer:?}: {errors:?}");
        assert!(!errors[0].contains("items inside could not be copied"), "{answer:?}: a planted name fails the item and is not a fold: {errors:?}");
    }
}

// Sample input: a copy made under Rename is undone and redone twice; each redo writes the renamed names inside the folder.
#[test]
fn redo_of_a_copy_writes_the_names_the_copy_was_made_with() {
    let d = TestDir::new("fat-redo-inside");
    let to = d.dir("to");
    let trip = trip_with_bad_names(&d);
    let _fat = RefusesFatNames::arm(&["clip: x.mp4", "sub: a", "b?.txt"]);
    let (ok, failed, _, errors, entry) = transfer(false, &[&trip], &to, fat_policy(Choice::Rename, true));
    assert_eq!((ok, failed), (1, 0), "{errors:?}");
    // The journal file is the other window's redo source, so the entry goes through its codec first.
    let doc = crate::backend::undoshare::Doc {
        undo: vec![entry],
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    let written = crate::jsondoc::render(&crate::backend::undocodec::encode(&doc));
    let mut back = crate::backend::undocodec::decode(&written).expect("the journal reads back");
    let entry = back.undo.remove(0);
    assert!(entry.steps.iter().any(|s| matches!(s, crate::backend::undo::Step::Copied { rename_fat: true, .. })), "the step keeps the answer it was made with");
    let mut journal = Journal::new();
    journal.push(entry);
    for round in 0..2 {
        assert_eq!(journal.undo().unwrap(), "copy", "round {round}");
        assert!(to.join("trip").symlink_metadata().is_err(), "round {round}: undo takes the copy back");
        let (tx, _rx) = std::sync::mpsc::channel();
        let redone = journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx);
        assert_eq!(redone.map_err(|e| e.msg).unwrap(), "copy", "round {round}");
        assert_eq!(text(&to.join("trip/clip_ x.mp4")), "inside", "round {round}: redo writes the renamed name");
        assert_eq!(text(&to.join("trip/sub_ a/b_.txt")), "deeper", "round {round}");
        assert!(to.join("trip/clip: x.mp4").symlink_metadata().is_err(), "round {round}: no raw name landed");
    }
}

// Sample input: a cross-device move made under Rename is undone and redone; a name that reaches the folder after the undo still lands renamed.
#[test]
fn redo_of_a_cross_device_move_writes_the_names_the_move_was_made_with() {
    let d = TestDir::new("fat-redo-move-inside");
    let to = d.dir("to");
    let trip = trip_with_bad_names(&d);
    let _fat = RefusesFatNames::arm(&["clip: x.mp4", "sub: a", "b?.txt", "late: y.txt"]);
    crate::backend::copyfile::test_force_exdev(true);
    let (ok, failed, _, errors, entry) = transfer(true, &[&trip], &to, fat_policy(Choice::Rename, true));
    assert_eq!((ok, failed), (1, 0), "{errors:?}");
    assert_eq!(
        text(&to.join("trip/clip_ x.mp4")),
        "inside",
        "the move itself wrote the renamed name"
    );
    let doc = crate::backend::undoshare::Doc {
        undo: vec![entry],
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    let written = crate::jsondoc::render(&crate::backend::undocodec::encode(&doc));
    let mut back = crate::backend::undocodec::decode(&written).expect("the journal reads back");
    let entry = back.undo.remove(0);
    assert!(entry.steps.iter().any(|s| matches!(s, crate::backend::undo::Step::Moved { rename_fat: true, .. })), "the move step keeps the answer it was made with");
    let mut journal = Journal::new();
    journal.push(entry);
    for round in 0..2 {
        assert_eq!(journal.undo().unwrap(), "move", "round {round}");
        assert!(to.join("trip").symlink_metadata().is_err(), "round {round}: undo takes the move back");
        if round == 1 {
            // Below the top, so the folder still reads as the one the step recorded.
            d.file("from/trip/sub_ a/late: y.txt", "late");
        }
        let (tx, _rx) = std::sync::mpsc::channel();
        let redone = journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx);
        assert_eq!(redone.map_err(|e| e.msg).unwrap(), "move", "round {round}");
        assert_eq!(text(&to.join("trip/clip_ x.mp4")), "inside", "round {round}: redo writes the renamed name");
        assert_eq!(text(&to.join("trip/sub_ a/b_.txt")), "deeper", "round {round}");
        assert!(trip.symlink_metadata().is_err(), "round {round}: the source went");
    }
    assert_eq!(text(&to.join("trip/sub_ a/late_ y.txt")), "late", "a name added after the undo is renamed too");
    assert!(to.join("trip/sub_ a/late: y.txt").symlink_metadata().is_err(), "no raw name landed");
    crate::backend::copyfile::test_force_exdev(false);
}

// A name that is not text is asked about under its own bytes, never skipped and never matched to its U+FFFD twin.
#[test]
fn a_raw_named_source_asks_about_its_own_name_and_shows_its_display_spelling() {
    use flea::bytename::ExactText as _;
    use std::os::unix::ffi::OsStrExt;
    let d = TestDir::new("collide-raw-name");
    let to = d.dir("to");
    let raw = std::ffi::OsStr::from_bytes(b"clip-\xff.txt");
    let src = d.dir("from").join(raw);
    std::fs::write(&src, "yours").unwrap();
    std::fs::write(to.join(raw), "there").unwrap();
    d.file("to/clip-\u{fffd}.txt", "the twin");
    let (question, shown) = crate::backend::collide::ask(
        7,
        &[src.exact_text().into_owned()],
        &to.to_string_lossy(),
        None,
    );
    assert_eq!(
        shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["clip-\u{fffd}.txt"]
    );
    let there = crate::backend::undo::ItemIdentity::inspect(&to.join(raw)).unwrap();
    assert!(
        question.seen[&src].same_item(&there),
        "the raw file is the one asked about, never its twin"
    );
}
