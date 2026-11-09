// A written file counts only once its close succeeds and the destination shows all of it (soak S8-12).
use super::*;
use crate::backend::durable::{self, Durability};
use crate::backend::testdir::TestDir;
use std::sync::atomic::AtomicBool;

// The errno a FUSE mount answers close with when its upload failed.
const EIO: i32 = 5;

fn progress<'a>(
    flag: &'a AtomicBool,
    sink: &'a mut dyn FnMut(u64, u64),
    for_move: bool,
) -> Progress<'a> {
    Progress {
        cancel: flag,
        on_bytes: sink,
        partial: None,
        tree: None,
        manifest: None,
        durability: None,
        for_move,
    }
}

fn clear_faults() {
    landed::test_fail_closes(None);
    landed::test_short_looks(None);
    landed::test_no_wait(false);
    test_force_exdev(false);
}

// S8-12: gvfs answered close for a WebDAV upload that never finished, and the move deleted its source.
#[test]
fn a_move_whose_close_fails_keeps_the_source_and_journals_the_copy() {
    let d = TestDir::new("landed-close-move");
    std::fs::create_dir_all(d.join("src/tree/sub")).unwrap();
    d.file("src/f.txt", "f");
    d.file("src/tree/a.txt", "a");
    d.file("src/tree/sub/b.txt", "b");
    let out = d.dir("out");
    test_force_exdev(true);
    landed::test_fail_closes(Some(EIO));
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut outcomes = Vec::new();
    for name in ["f.txt", "tree"] {
        let mut p = progress(&flag, &mut sink, true);
        outcomes.push((
            move_any(&d.join("src").join(name), &out.join(name), &mut p),
            p.partial.take(),
        ));
    }
    clear_faults();
    for ((outcome, partial), name) in outcomes.into_iter().zip(["f.txt", "tree"]) {
        let e = outcome.expect_err("a failed close fails the item");
        assert!(
            partial.is_some() || !out.join(name).exists(),
            "the copy is journaled for undo or went whole: {}",
            e.msg
        );
    }
    assert_eq!(std::fs::read_to_string(d.join("src/f.txt")).unwrap(), "f");
    assert_eq!(
        std::fs::read_to_string(d.join("src/tree/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("src/tree/sub/b.txt")).unwrap(),
        "b"
    );
}

#[test]
fn a_copy_whose_close_fails_reports_it() {
    let d = TestDir::new("landed-close-copy");
    let src = d.file("f.txt", "f");
    let out = d.dir("out");
    landed::test_fail_closes(Some(EIO));
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = progress(&flag, &mut sink, false);
    let outcome = copy_any(&src, &out.join("f.txt"), &mut p);
    let partial = p.partial.take();
    clear_faults();
    let e = outcome.expect_err("the copy says the close failed");
    let eio = crate::error::io_message(&std::io::Error::from_raw_os_error(EIO));
    assert_eq!((e.where_.as_str(), e.msg), ("copy", eio));
    assert_eq!(
        partial,
        Some(out.join("f.txt")),
        "the unconfirmed copy is journaled for undo"
    );
}

#[test]
fn a_landing_never_shown_whole_keeps_the_source() {
    let d = TestDir::new("landed-short");
    let src = d.file("f.txt", "body");
    let out = d.dir("out");
    test_force_exdev(true);
    landed::test_short_looks(Some(usize::MAX));
    landed::test_no_wait(true);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = progress(&flag, &mut sink, true);
    let outcome = move_any(&src, &out.join("f.txt"), &mut p);
    clear_faults();
    let e = outcome.expect_err("the destination never showed every byte");
    assert_eq!(
        (e.where_.as_str(), e.msg.as_str()),
        ("copy", landed::FILE_UNCONFIRMED),
        "the copy's own landing failed it, before the move's check"
    );
    assert_eq!(
        std::fs::read_to_string(&src).unwrap(),
        "body",
        "the source stays"
    );
}

#[test]
fn a_copy_whose_landing_is_never_shown_whole_reports_it() {
    let d = TestDir::new("landed-short-copy");
    let src = d.file("f.txt", "body");
    let out = d.dir("out");
    landed::test_short_looks(Some(usize::MAX));
    landed::test_no_wait(true);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = progress(&flag, &mut sink, false);
    let outcome = copy_any(&src, &out.join("f.txt"), &mut p);
    let partial = p.partial.take();
    clear_faults();
    let e = outcome.expect_err("the copy says the drive never showed every byte");
    assert_eq!(e.msg, landed::FILE_UNCONFIRMED);
    assert_eq!(
        partial,
        Some(out.join("f.txt")),
        "the unconfirmed copy is journaled for undo"
    );
}

// gvfs shows a WebDAV file short until its upload ends, so a short look is waited out rather than failed.
#[test]
fn a_landing_shown_short_while_it_uploads_moves_once_whole() {
    let d = TestDir::new("landed-late");
    let src = d.file("f.txt", "body");
    let out = d.dir("out");
    test_force_exdev(true);
    landed::test_short_looks(Some(2));
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = progress(&flag, &mut sink, true);
    let outcome = move_any(&src, &out.join("f.txt"), &mut p);
    clear_faults();
    outcome.expect("the move finishes once the destination shows every byte");
    assert!(!src.exists(), "the source goes once its copy is whole");
    assert_eq!(std::fs::read_to_string(out.join("f.txt")).unwrap(), "body");
}

#[test]
fn a_copy_that_shrank_before_its_source_goes_keeps_the_source() {
    let d = TestDir::new("landed-shrank");
    let src = d.file("f.txt", "body");
    let dst = d.file("out.txt", "bo");
    let e = finish_move(&src, &dst, None).expect_err("a short copy never lets its source go");
    assert_eq!(
        (e.error.msg.as_str(), e.copy_to_undo),
        (landed::FILE_UNCONFIRMED, true)
    );
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "body");
}

// A held FAT file closes behind its batch's syncfs, so a failed close fails the done line and every later confirm.
#[test]
fn a_held_file_whose_close_fails_leaves_the_result_unconfirmed() {
    use std::sync::mpsc::channel;
    durable::test_reset();
    let d = TestDir::new("landed-wave");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = progress(&flag, &mut sink, false);
    p.durability = Some(&mut durability);
    copy_any(&src, &out.join("a.txt"), &mut p).expect("the copy lands and is held");
    drop(p);
    assert_eq!(durability.held_len(), 1);
    landed::test_fail_closes(Some(EIO));
    durability
        .flush_dirs()
        .expect("the syncfs confirms the batch and its wave starts closing");
    clear_faults();
    let (tx, _rx) = channel();
    let done = durable::finish(7, &tx, &mut durability, &out, 1);
    assert_eq!(
        (done.ok, done.note.as_str()),
        (false, landed::FILE_UNCONFIRMED)
    );
    assert!(
        durability.flush_dirs().is_err(),
        "a failed close fails every later confirm"
    );
    durable::test_reset();
}
