// The Jobs registry against the b-jobs contract: keyed rows, stable places, one Cancel per block, the strip.
use crate::ui_logic_op1::jobs::{self, Action, Jobs, State};
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::{joined, parse, trash, Event, JobRow};

fn row(id: u64, kind: &str, state: &str, head: &str) -> JobRow {
    JobRow {
        id,
        kind: kind.into(),
        state: state.into(),
        head: head.into(),
        ..JobRow::default()
    }
}

fn job(line: &str) -> JobRow {
    match parse(line) {
        Some(Event::Job { job, .. }) => job,
        other => panic!("not a job line: {other:?}"),
    }
}

#[test]
fn concurrent_jobs_keep_their_places_and_new_ones_enter_on_top() {
    let mut reg = Jobs::default();
    reg.update(row(12, "copy", "running", "Copying 10 of 18 to NAS"));
    reg.update(row(13, "copy", "waiting", "Copy 8 items to NAS"));
    reg.update(row(14, "action", "running", "Resize to 1600 px · 4 of 9"));
    let ids = |r: &Jobs| r.blocks().map(|b| b.id).collect::<Vec<_>>();
    assert_eq!(ids(&reg), vec![14, 13, 12]);
    // An update replaces its row where it stands, so the first job's Cancel never moves.
    reg.update(row(12, "copy", "running", "Copying 11 of 18 to NAS"));
    assert_eq!(ids(&reg), vec![14, 13, 12]);
    assert_eq!(reg.rows[2].head, "Copying 11 of 18 to NAS");
    // A failed row keeps its position; a done row leaves the card.
    reg.update(row(13, "copy", "failed", ""));
    reg.update(row(14, "action", "done", ""));
    assert_eq!(ids(&reg), vec![13, 12]);
    assert_eq!(reg.unfinished(), 1);
    assert_eq!(reg.oldest().map(|r| r.id), Some(12));
}

#[test]
fn the_strip_names_the_oldest_unfinished_head_and_counts_only_unfinished() {
    let cat = Catalog::english();
    let mut reg = Jobs::default();
    assert_eq!(jobs::strip(&cat.i18n, &reg), None);
    reg.update(row(1, "copy", "running", "Copying 10 of 18 to NAS"));
    assert_eq!(
        jobs::strip(&cat.i18n, &reg),
        Some(("Copying 10 of 18 to NAS".into(), String::new()))
    );
    reg.update(row(2, "copy", "waiting", "Copy 8 items to NAS"));
    reg.update(row(3, "send", "running", "Sending 2 of 3 to gdrive"));
    reg.update(row(4, "send", "failed", ""));
    let strip = jobs::strip(&cat.i18n, &reg);
    assert_eq!(
        strip,
        Some(("Copying 10 of 18 to NAS".into(), " · 3 jobs".into()))
    );
    reg.toggle_fold();
    assert!(reg.folded);
    assert_eq!(jobs::strip(&cat.i18n, &reg), strip);
}

#[test]
fn a_running_copy_block_reads_as_the_board_draws_it() {
    let cat = Catalog::english();
    let r = job(
        r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 10 of 18 to NAS","file":"DSC_4944.jpg","done":9,"total":18,"bytes":5300000,"totalBytes":9700000,"itemBytes":458300,"rate":1100000,"left":4,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":3}"#,
    );
    let b = jobs::block(&cat.i18n, &r);
    assert_eq!(b.head, "Copying 10 of 18 to NAS");
    assert_eq!(b.file, "DSC_4944.jpg · 458.3 kB");
    assert_eq!(joined(&b.line), "5.3 MB of 9.7 MB · 1.1 MB/s · 0:04 left");
    assert_eq!(
        b.line
            .iter()
            .filter(|p| p.figure)
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>(),
        vec!["5.3 MB", "9.7 MB", "1.1 MB/s", "0:04"]
    );
    assert_eq!(b.buttons, vec![Action::Cancel]);
    assert!((b.bar.unwrap() - 5.3 / 9.7).abs() < 1e-9);
}

#[test]
fn a_waiting_block_names_its_predecessor_with_an_empty_bar() {
    let cat = Catalog::english();
    let r = JobRow {
        reason: "waiting for the copy to NAS".into(),
        ..row(13, "copy", "waiting", "Copy 8 items to NAS")
    };
    let b = jobs::block(&cat.i18n, &r);
    assert_eq!(
        (b.file.as_str(), b.bar, b.mark_dim),
        ("waiting for the copy to NAS", Some(0.0), true)
    );
    assert!(b.line.is_empty());
    assert_eq!(b.buttons, vec![Action::Cancel]);
}

#[test]
fn an_action_block_counts_files_and_shows_its_last_two_output_lines() {
    let cat = Catalog::english();
    let r = JobRow {
        done: 4,
        total: 9,
        file: "DSC_4901.jpg".into(),
        output: vec!["a".into(), "b".into(), "c".into()],
        ..row(14, "action", "running", "Resize to 1600 px · 4 of 9")
    };
    let b = jobs::block(&cat.i18n, &r);
    assert_eq!(b.output, vec!["b".to_string(), "c".to_string()]);
    assert_eq!(b.file, "");
    assert!(b.line.is_empty());
    assert!((b.bar.unwrap() - 4.0 / 9.0).abs() < 1e-9);
    let quiet = jobs::block(
        &cat.i18n,
        &JobRow {
            output: Vec::new(),
            ..r
        },
    );
    assert_eq!(quiet.file, "DSC_4901.jpg");
}

#[test]
fn a_failed_block_offers_retry_and_dismiss_with_no_bar() {
    let cat = Catalog::english();
    let r = JobRow {
        reason: "DSC_4930.jpg did not match on b2-photos.".into(),
        ..row(15, "send", "failed", "Sending 2 of 3")
    };
    let b = jobs::block(&cat.i18n, &r);
    assert_eq!(
        (b.head.as_str(), b.file.as_str(), b.state),
        (
            "Send failed",
            "DSC_4930.jpg did not match on b2-photos.",
            State::Failed
        )
    );
    assert_eq!(
        (b.bar, b.buttons.clone()),
        (None, vec![Action::Retry, Action::Dismiss])
    );
    for (kind, head) in [
        ("compress", "Compression failed"),
        ("extract", "Extraction failed"),
        ("emptyTrash", "Emptying Trash failed"),
    ] {
        assert_eq!(
            jobs::block(&cat.i18n, &row(1, kind, "failed", "")).head,
            head
        );
    }
    assert_eq!(
        jobs::block(
            &cat.i18n,
            &row(1, "action", "failed", "Resize to 1600 px failed")
        )
        .head,
        "Resize to 1600 px failed"
    );
}

#[test]
fn unknown_totals_never_invent_a_share_or_estimate() {
    let cat = Catalog::english();
    let r = JobRow {
        bytes: Some(0),
        total_bytes: Some(0),
        ..row(16, "copy", "running", "Copying 1 of 2")
    };
    let b = jobs::block(&cat.i18n, &r);
    assert_eq!(b.bar, Some(0.0));
    assert!(b.line.is_empty());
    let rateless = JobRow {
        bytes: Some(1_500_000),
        total_bytes: Some(9_000_000),
        rate: 0,
        left: None,
        ..r
    };
    assert_eq!(
        joined(&jobs::block(&cat.i18n, &rateless).line),
        "1.5 MB of 9.0 MB · 0 B/s"
    );
}

#[test]
fn emptying_trash_counts_files_and_estimates_from_deletions() {
    let cat = Catalog::english();
    let r = JobRow {
        file: "node_modules".into(),
        item_bytes: 412_600_000,
        done: 41_212,
        total: 104_881,
        left: Some(27),
        bytes: None,
        total_bytes: None,
        ..row(17, "emptyTrash", "running", "")
    };
    let b = jobs::block(&cat.i18n, &r);
    assert_eq!(b.head, "Emptying Trash");
    assert_eq!(b.file, "node_modules · 412.6 MB");
    assert_eq!(joined(&b.line), "41,212 of 104,881 deleted · 0:27 left");
}

#[test]
fn requests_name_only_their_own_job() {
    assert_eq!(
        jobs::request(Action::Cancel.op(), 12),
        r#"{"c":"jobs","op":"cancel","id":12}"#
    );
    assert_eq!(
        jobs::request(Action::Retry.op(), 13),
        r#"{"c":"jobs","op":"retry","id":13}"#
    );
    assert_eq!(
        jobs::request(Action::Dismiss.op(), 14),
        r#"{"c":"jobs","op":"dismiss","id":14}"#
    );
    assert_eq!(jobs::list_request(), r#"{"c":"jobs","op":"list"}"#);
}

#[test]
fn a_stale_retry_and_other_refusals_read_in_the_catalog_words() {
    let cat = Catalog::english();
    let Some(Event::JobError { id, error, code }) = parse(
        r#"{"t":"joberror","id":12,"error":"A failed item changed; it was not retried.","code":"changed"}"#,
    ) else {
        panic!()
    };
    assert_eq!(
        (id, jobs::refusal(&cat.i18n, &error, &code)),
        (12, "A failed item changed; it was not retried.".into())
    );
    // With no code the backend's own sentence stands.
    assert_eq!(
        jobs::refusal(&cat.i18n, "Only a failed job can be retried.", ""),
        "Only a failed job can be retried."
    );
    assert_eq!(
        jobs::refusal(&cat.i18n, "x", "busy"),
        "An operation is already running."
    );
}

#[test]
fn undo_names_only_the_newest_journalled_job_while_nothing_runs() {
    let mut reg = Jobs::default();
    reg.update(JobRow {
        undo: true,
        ..row(1, "copy", "done", "")
    });
    reg.update(JobRow {
        undo: false,
        ..row(2, "send", "done", "")
    });
    assert_eq!(reg.undo_target(), None);
    reg.update(JobRow {
        undo: true,
        ..row(3, "move", "done", "")
    });
    assert_eq!(reg.undo_target(), Some(3));
    reg.update(row(4, "copy", "running", ""));
    assert_eq!(reg.undo_target(), None);
}

#[test]
fn a_malformed_job_line_is_not_a_job() {
    assert_eq!(parse(r#"{"t":"job","job":{"kind":"copy"}}"#), None);
    assert_eq!(parse(r#"{"t":"jobs","jobs":[{"id":"x"}]}"#), None);
    assert_eq!(parse("not json"), None);
    assert_eq!(parse(r#"{"t":"other"}"#), None);
}

#[test]
fn delete_routes_to_trash_to_the_permanent_card_or_to_a_refusal() {
    // A writable place with no Trash asks the Cancel-default permanent card; a read-only one refuses.
    assert_eq!(trash::delete_route(true, true), trash::Route::Trash);
    assert_eq!(trash::delete_route(true, false), trash::Route::Refuse);
    assert_eq!(
        trash::delete_route(false, true),
        trash::Route::ConfirmPermanent
    );
    assert_eq!(trash::delete_route(false, false), trash::Route::Refuse);
    assert_eq!(trash::PRIMARY, trash::Button::Cancel);
}

#[test]
fn checked_delete_counts_validation_then_deletions_and_cancel_ends_once() {
    let cat = Catalog::english();
    let mut e = trash::Emptying::started(40, 0, 0);
    assert!(e.sampled(
        40,
        "checking",
        "node_modules",
        412_600_000,
        0,
        104_881,
        3,
        100
    ));
    assert_eq!(joined(&e.count_parts(&cat.i18n)), "3 of 104,881 checked");
    assert_eq!(e.left(), None);
    assert!(!e.sampled(41, "deleting", "x", 0, 9, 9, 0, 200));
    assert!(e.sampled(
        40,
        "deleting",
        "node_modules",
        412_600_000,
        0,
        104_881,
        0,
        1_000
    ));
    assert!(e.sampled(
        40,
        "deleting",
        "node_modules",
        412_600_000,
        41_212,
        104_881,
        0,
        18_000
    ));
    assert_eq!(
        joined(&e.count_parts(&cat.i18n)),
        "41,212 of 104,881 deleted · 0:26 left"
    );
    assert!(!e.finish(41));
    assert!(e.finish(40));
    assert!(!e.running);
    assert!(!e.sampled(40, "deleting", "x", 0, 50_000, 104_881, 0, 19_000));
    // An unfilled, stale or running review never enables Delete.
    let review = trash::Review {
        count: Some(3),
        token: "\"t\"".into(),
        ..Default::default()
    };
    assert!(trash::delete_enabled(&review, false));
    assert!(!trash::delete_enabled(&review, true));
    assert!(!trash::delete_enabled(
        &trash::Review {
            stale: true,
            ..review.clone()
        },
        false
    ));
    assert!(!trash::delete_enabled(
        &trash::Review {
            count: None,
            ..review.clone()
        },
        false
    ));
    assert!(!trash::delete_enabled(
        &trash::Review {
            token: String::new(),
            ..review
        },
        false
    ));
    assert!(!trash::empty_button_enabled(10, true));
    assert!(trash::empty_button_enabled(10, false));
    assert!(!trash::empty_button_enabled(0, false));
}

// Jobs never rebuild an unchanged block: an identical update or snapshot reports no change.
#[test]
fn an_unchanged_row_reports_no_change() {
    let mut reg = Jobs::default();
    assert!(reg.update(row(12, "copy", "running", "Copying 10 of 18 to NAS")));
    assert!(!reg.update(row(12, "copy", "running", "Copying 10 of 18 to NAS")));
    assert!(reg.update(row(12, "copy", "running", "Copying 11 of 18 to NAS")));
    let rows = reg.rows.clone();
    assert!(!reg.snapshot(rows.clone()));
    // Negative: a refusal on screen is cleared by the snapshot, which is a change.
    reg.refused("busy");
    assert!(reg.snapshot(rows));
}
