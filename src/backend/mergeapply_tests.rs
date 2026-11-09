// merge_folder's tests, beside it so mergeapply.rs keeps under the file budget.
use super::*;
use crate::backend::collide::{Ask, Question};
use crate::backend::testdir::TestDir;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn a_cancel_inside_the_last_nested_folder_reports_cancelled() {
    let d = TestDir::new("merge-cancel-nested");
    let src_trip = d.dir("from/trip");
    d.dir("from/trip/sub");
    d.file("from/trip/sub/a.txt", "a-body-with-bytes-to-report");
    d.file("from/trip/sub/b.txt", "b-body-with-bytes-to-report");
    let dest = d.dir("to");
    d.dir("to/trip/sub");
    // A forced-durable destination confirms each copy after its report, so the trip lands before the cancel stops the fold.
    crate::backend::durable::test_mark_durable(&dest);
    let question = Question { id: 7, dest: dest.clone(), seen: HashMap::new(), twins: Default::default(), menu: None, fat: None };
    let inner = Ask { word: Some("merge".to_string()), id: 7, inner: Some("keep".to_string()), fat: None }
        .policy(Some(question), &dest).inner();
    let cancel = AtomicBool::new(false);
    let mut carried = 0u64;
    let mut sink = |_done: u64, _total: u64| {
        cancel.store(true, Ordering::Relaxed);
    };
    let mut manifest = None;
    let mut ctx = crate::backend::durable::Durability::begin(&dest);
    let mut counts = Counts::default();
    let mut deferred = Deferred::begin();
    let mut steps = Vec::new();
    let r = merge_into(&src_trip, &dest.join("trip"), &dest, false, &inner, &mut steps, &cancel,
        &mut sink, &mut carried, &mut manifest, &mut ctx, &mut counts, &mut deferred);
    let err = r.expect_err("a cancel inside the last nested folder reports cancelled, not done");
    assert!(err.msg.contains("cancelled"), "the error names the cancel: {}", err.msg);
}

#[test]
fn a_move_whose_journal_step_fails_is_a_failure_never_a_silent_gap() {
    let d = TestDir::new("merge-journal-gap");
    let landed = d.file("landed.txt", "landed");
    let mut steps = Vec::new();
    journal_moved(&mut steps, &d.join("src.txt"), &landed).expect("a landed move journals");
    assert_eq!(steps.len(), 1, "the moved file is journaled");
    let before = steps.len();
    let missing = d.join("gone.txt");
    let err = journal_moved(&mut steps, &d.join("src.txt"), &missing)
        .expect_err("a journal step the drive will not take fails the land");
    assert_eq!(steps.len(), before, "nothing is journaled for the failed step: {err:?}");
}

#[test]
fn land_one_fails_a_landed_move_whose_journal_step_fails() {
    let d = TestDir::new("merge-land-journal");
    let dest = d.dir("to");
    let cancel = AtomicBool::new(false);
    let (mut carried, mut manifest, mut steps) = (0u64, None, Vec::new());
    let mut ctx = crate::backend::durable::Durability::begin(&dest);
    let mut sink = |_done: u64, _total: u64| {};
    let mut land_move = |src: &Path, to: &Path, steps: &mut Vec<Step>| {
        let mut land = LandCtx { moving: true, cancel: &cancel, sink: &mut sink, carried: &mut carried, manifest: &mut manifest, ctx: &mut ctx, steps };
        land_one(src, to, &mut land)
    };
    let (first, second) = (d.file("first.txt", "one"), d.file("second.txt", "two"));
    land_move(&first, &dest.join("first.txt"), &mut steps).expect("a move whose journal takes the step lands");
    assert_eq!(steps.len(), 1, "the control move is journaled");
    FAIL_JOURNAL.with(|fail| fail.set(true));
    let err = land_move(&second, &dest.join("second.txt"), &mut steps).expect_err("a landed move that cannot be journaled is a failure");
    assert_eq!(err.where_, "journal", "the failure names the journal: {err:?}");
    assert!(dest.join("second.txt").is_file() && !second.exists(), "the move itself landed");
    assert_eq!(steps.len(), 1, "no step was journaled for it");
}
