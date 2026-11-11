//! Concurrent pastes keep their own presentation, cancellation and completion receipts.
use super::{model::Operations, tests::catalog};
use flea::ui_logic_file_ops::jobs;

fn started(ops: &mut Operations, id: u64) {
    ops.line(
        &catalog(),
        &format!(r#"{{"t":"transferstarted","id":{id},"n":1,"moving":true}}"#),
        0,
    );
}

fn job(ops: &mut Operations, id: u64, state: &str, undo: bool) {
    ops.line(
        &catalog(),
        &format!(
            r#"{{"t":"job","job":{{"id":{id},"kind":"move","head":"Moving 1 of 1","state":"{state}","undo":{undo}}}}}"#
        ),
        0,
    );
}

fn done(ops: &mut Operations, id: u64, failed: bool) {
    ops.line(
        &catalog(),
        &format!(
            r#"{{"t":"transferdone","id":{id},"ok":{},"failed":{},"skipped":0}}"#,
            u8::from(!failed),
            u8::from(failed)
        ),
        0,
    );
}

#[test]
fn queued_paste_cancels_each_transfer_by_its_own_id() {
    let mut ops = Operations::new("default");
    for id in [1, 2] {
        started(&mut ops, id);
        job(&mut ops, id, "running", false);
    }
    ops.job_action(2, jobs::Action::Cancel);
    assert_eq!(ops.outbox, [r#"{"c":"jobs","op":"cancel","id":2}"#]);
    assert!(!ops.transfer_cancelling);
    ops.outbox.clear();
    ops.cancel_transfer();
    ops.cancel_transfer();
    assert_eq!(ops.outbox, [r#"{"c":"transfercancel","id":1}"#]);
    done(&mut ops, 1, false);
    assert_eq!(ops.wire.transfer.id, 2);
    assert!(!ops.transfer_cancelling);
    ops.outbox.clear();
    ops.cancel_transfer();
    assert_eq!(ops.outbox, [r#"{"c":"transfercancel","id":2}"#]);
}

#[test]
fn queued_paste_survives_first_failure_and_preserves_both_receipts() {
    let mut ops = Operations::new("default");
    started(&mut ops, 1);
    started(&mut ops, 2);
    ops.line(
        &catalog(),
        r#"{"t":"transferitem","id":1,"index":0,"name":"a","ok":false,"err":"denied"}"#,
        0,
    );
    job(&mut ops, 1, "failed", false);
    done(&mut ops, 1, true);
    assert_eq!(ops.wire.transfer.id, 2);
    assert!(ops.wire.transfer.running);
    assert!(ops.sticky.starts_with("Moving 1 of 1"));
    assert!(ops
        .said
        .iter()
        .any(|said| said.error && said.text.contains("denied")));
    assert!(ops
        .said
        .iter()
        .any(|said| said.behind && said.text.contains("failed")));
    job(&mut ops, 2, "done", true);
    done(&mut ops, 2, false);
    assert!(!ops.wire.transfer.running && ops.sticky.is_empty());
    assert!(ops
        .said
        .iter()
        .any(|said| !said.error && said.text == "Moved 1 item · z undoes"));
    assert_eq!(
        ops.jobs.rows.iter().find(|job| job.id == 1).unwrap().state,
        "failed"
    );
    assert_eq!(
        ops.jobs.rows.iter().find(|job| job.id == 2).unwrap().state,
        "done"
    );
}

#[test]
fn queued_paste_releases_first_held_job_and_keeps_its_undo() {
    let mut ops = Operations::new("default");
    started(&mut ops, 1);
    job(&mut ops, 1, "done", true);
    started(&mut ops, 2);
    assert!(
        !ops.jobs.rows[0].undo,
        "the first job waits for its own transferdone"
    );
    done(&mut ops, 1, false);
    assert!(ops.jobs.rows.iter().find(|job| job.id == 1).unwrap().undo);
    assert_eq!(ops.wire.transfer.id, 2);
    assert!(ops
        .said
        .iter()
        .any(|said| said.text == "Moved 1 item · z undoes"));
    job(&mut ops, 2, "done", true);
    done(&mut ops, 2, false);
    assert!(ops
        .jobs
        .rows
        .iter()
        .all(|job| job.state == "done" && job.undo));
}

#[test]
fn queued_extract_completion_never_ends_the_presented_move() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    started(&mut ops, 1);
    ops.line(&cat, r#"{"t":"extractstarted","id":2}"#, 0);
    let sticky = ops.sticky.clone();
    assert!(!ops.extract_done(&cat, 1));
    assert!(!ops.extract_done(&cat, 99));
    assert!(ops.extract_done(&cat, 2));
    assert!(ops.wire.queued.is_empty());
    assert_eq!(ops.wire.transfer.id, 1);
    assert_eq!(ops.sticky, sticky);
    ops.cancel_transfer();
    assert_eq!(ops.outbox, [r#"{"c":"transfercancel","id":1}"#]);
    done(&mut ops, 1, false);
    assert!(!ops.wire.transfer.running);
}

#[test]
fn queued_extract_takes_the_card_and_finishes_after_a_move() {
    let cat = catalog();
    let mut ops = Operations::new("default");
    started(&mut ops, 1);
    ops.line(&cat, r#"{"t":"extractstarted","id":2}"#, 0);
    done(&mut ops, 1, false);
    assert!(ops.wire.transfer.extract && ops.wire.transfer.id == 2);
    assert_eq!(ops.sticky, "Extracting");
    assert!(ops.extract_done(&cat, 2));
    assert!(!ops.wire.transfer.running && ops.sticky.is_empty());
}

#[test]
fn queued_paste_that_ends_first_lands_its_own_row_and_receipt() {
    let mut ops = Operations::new("default");
    let row = |ops: &Operations, id: u64| {
        let job = ops.jobs.rows.iter().find(|job| job.id == id).unwrap();
        (job.state.clone(), job.undo)
    };
    started(&mut ops, 1);
    job(&mut ops, 1, "running", false);
    started(&mut ops, 2);
    job(&mut ops, 2, "done", true);
    done(&mut ops, 2, false);
    assert_eq!(row(&ops, 2), ("done".into(), true));
    assert!(ops.transfer_receipt.is_some());
    assert!(ops
        .said
        .iter()
        .any(|said| said.text == "Moved 1 item · z undoes"));
    assert_eq!(ops.wire.transfer.id, 1);
    assert!(ops.sticky.starts_with("Moving 1 of 1"), "{}", ops.sticky);
    job(&mut ops, 1, "done", true);
    assert_eq!(row(&ops, 1), ("running".into(), false));
    done(&mut ops, 1, false);
    assert_eq!(row(&ops, 1), ("done".into(), true));
    assert!(!ops.wire.transfer.running && ops.sticky.is_empty());
}
