//! Window boundaries, producer resets and timer lifetime without a wall-clock or a filesystem.
use super::*;

fn row(id: u64, bytes: u64) -> JobRow {
    JobRow {
        id,
        kind: "copy".into(),
        state: "running".into(),
        bytes: Some(bytes),
        total_bytes: Some(10_000),
        rate: 99_000,
        left: Some(7),
        ..Default::default()
    }
}

fn publish(rates: &mut Rates, source: &JobRow, now: u64) -> JobRow {
    let mut rows = [source.clone()];
    rates.publish(&mut rows, now);
    assert_eq!(source.rate, 99_000);
    assert_eq!(source.left, Some(7));
    rows[0].clone()
}

#[test]
fn card_rate_retains_edge_sample_and_does_not_invent_late_beats() {
    let mut rates = Rates::default();
    assert_eq!(publish(&mut rates, &row(1, 100), 0).rate, 0);
    assert_eq!(publish(&mut rates, &row(1, 500), 250).rate, 1600);
    assert_eq!(publish(&mut rates, &row(1, 1000), 500).rate, 1800);
    assert_eq!(publish(&mut rates, &row(1, 1000), 2250).rate, 400);
    assert_eq!(publish(&mut rates, &row(1, 1000), 2500).rate, 222);
    assert_eq!(publish(&mut rates, &row(1, 1000), 2750).rate, 0);
    assert_eq!(publish(&mut rates, &row(1, 2000), 10_000).rate, 137);
    assert_eq!(publish(&mut rates, &row(1, 2000), 10_250).rate, 133);
    assert_eq!(publish(&mut rates, &row(1, 2000), 12_250).rate, 0);
}

#[test]
fn card_rate_counter_and_item_count_rollback_start_fresh_even_between_beats() {
    let mut rates = Rates::default();
    publish(&mut rates, &row(1, 4000), 0);
    publish(&mut rates, &row(1, 6000), 250);
    let reset = publish(&mut rates, &row(1, 1000), 260);
    assert_eq!((reset.bytes, reset.rate, reset.left), (Some(1000), 0, None));
    assert_eq!(publish(&mut rates, &row(1, 2000), 510).rate, 4000);
    let mut input = row(1, 3000);
    input.done = 2;
    publish(&mut rates, &input, 760);
    input.done = 0;
    let reset = publish(&mut rates, &input, 770);
    assert_eq!((reset.bytes, reset.rate, reset.left), (Some(3000), 0, None));
}

#[test]
fn card_rate_clock_rollback_and_duplicate_frames_do_not_cross_clock_epochs() {
    let mut rates = Rates::default();
    publish(&mut rates, &row(1, 100), 1000);
    assert_eq!(publish(&mut rates, &row(1, 200), 1200).bytes, Some(100));
    let reset = publish(&mut rates, &row(1, 300), 1100);
    assert_eq!((reset.bytes, reset.rate), (Some(300), 0));
    assert_eq!(rates.due(1100), Some(1350));
    assert_eq!(publish(&mut rates, &row(1, 400), 1100).bytes, Some(300));
    assert_eq!(
        rates.due(1350),
        None,
        "an expired beat cannot spin before a frame"
    );
    assert_eq!(publish(&mut rates, &row(1, 800), 1350).rate, 2000);
    assert_eq!(rates.due(1350), Some(1600));
}

#[test]
fn card_rate_job_sessions_removal_retry_and_owner_instances_are_independent() {
    let mut a = Rates::default();
    let mut b = Rates::default();
    let mut rows = [row(1, 100), row(2, 200)];
    a.publish(&mut rows, 0);
    rows = [row(1, 1100), row(2, 2200)];
    a.publish(&mut rows, 250);
    assert_eq!((rows[0].rate, rows[1].rate), (4000, 8000));
    assert_eq!(publish(&mut b, &row(1, 1100), 250).rate, 0);
    a.publish(&mut [], 300);
    assert_eq!(a.due(300), None);
    assert_eq!(publish(&mut a, &row(1, 1100), 310).rate, 0);
    let mut input = row(1, 1200);
    input.state = "done".into();
    assert_eq!(publish(&mut a, &input, 320), input);
    assert_eq!(a.due(320), None);
    assert_eq!(publish(&mut a, &row(1, 1200), 330).rate, 0);
    input = row(1, 1300);
    input.kind = "move".into();
    assert_eq!(publish(&mut a, &input, 340).rate, 0);
    input.to = "another destination".into();
    assert_eq!(publish(&mut a, &input, 350).rate, 0);
}

#[test]
fn card_rate_preserves_non_transfer_and_missing_byte_contracts() {
    let mut rates = Rates::default();
    for kind in ["send", "compress", "emptyTrash", "action"] {
        let mut input = row(1, 500);
        input.kind = kind.into();
        assert_eq!(publish(&mut rates, &input, 0), input);
    }
    let mut input = row(1, 500);
    input.total_bytes = None;
    assert_eq!(publish(&mut rates, &input, 0), input);
    input.total_bytes = Some(0);
    let sample = publish(&mut rates, &input, 0);
    assert_eq!((sample.rate, sample.left), (0, None));
    input.bytes = Some(1500);
    let sample = publish(&mut rates, &input, 250);
    assert_eq!((sample.rate, sample.left), (4000, None));
    input.bytes = None;
    assert_eq!(publish(&mut rates, &input, 260), input);
    assert_eq!(rates.due(260), None);
}

#[test]
fn card_rate_uses_wide_byte_arithmetic_and_stops_at_the_clock_limit() {
    let mut rates = Rates::default();
    publish(&mut rates, &row(1, 0), 0);
    let sample = publish(&mut rates, &row(1, u64::MAX), 250);
    assert_eq!((sample.rate, sample.left), (u64::MAX, Some(0)));
    publish(&mut rates, &row(1, 100), u64::MAX);
    assert_eq!(rates.due(u64::MAX), None);
}

#[test]
fn card_rate_real_presentation_restarts_after_fold_and_stops_after_completion() {
    let mut model = super::super::Operations::new("default");
    model.jobs.update(row(1, 100));
    let mut presentation = super::super::Presentation::default();
    presentation.observe(&model, 0);
    presentation.commit(150);
    assert_eq!(presentation.jobs.rows[0].rate, 0);
    assert_eq!(presentation.due(150), Some(400));
    model.jobs.update(row(1, 1100));
    presentation.observe(&model, 400);
    presentation.commit(400);
    assert_eq!(presentation.jobs.rows[0].rate, 4000);
    model.jobs.toggle_fold();
    presentation.toggled(&model, 410);
    presentation.commit(410);
    assert!(presentation.jobs.folded);
    assert_eq!(presentation.due(410), None);
    model.jobs.toggle_fold();
    presentation.toggled(&model, 420);
    presentation.commit(420);
    assert_eq!(presentation.jobs.rows[0].rate, 0);
    model.jobs.rows[0].state = "done".into();
    presentation.observe(&model, 530);
    presentation.commit(530);
    assert_eq!(presentation.due(530), None);
    assert_eq!(model.jobs.rows[0].rate, 99_000);
}

#[test]
fn eta_warmup_first_three_bytes_keep_legacy_zero_rate_without_eta() {
    let catalog = crate::test_catalogs::english();
    for kind in ["extract", "copy", "move"] {
        let mut rates = Rates::default();
        let mut input = row(1, 3);
        input.kind = kind.into();
        input.total_bytes = Some(60_800_000);
        let sample = publish(&mut rates, &input, 5);
        let block = jobs::block(&catalog, &sample);
        let line = flea::ui_logic_file_ops::joined(&block.line);
        assert_eq!(line, "3 B of 60.8 MB · 0 B/s", "{kind}");
        assert_eq!((sample.rate, sample.left), (0, None));
        input.bytes = Some(4);
        let before_beat = publish(&mut rates, &input, 254);
        assert_eq!(
            (before_beat.bytes, before_beat.rate, before_beat.left),
            (Some(3), 0, None)
        );
        let on_beat = publish(&mut rates, &input, 255);
        assert_eq!(
            (on_beat.bytes, on_beat.rate, on_beat.left),
            (Some(4), 4, Some(15_199_999))
        );
    }
}

#[test]
fn eta_warmup_zero_sample_hides_the_byte_line_until_the_publish_beat() {
    let catalog = crate::test_catalogs::english();
    let mut rates = Rates::default();
    let mut input = row(1, 0);
    input.kind = "extract".into();
    input.total_bytes = Some(60_817_408);
    let sample = publish(&mut rates, &input, 0);
    assert!(jobs::block(&catalog, &sample).line.is_empty());
    input.bytes = Some(3);
    let sample = publish(&mut rates, &input, 5);
    assert!(jobs::block(&catalog, &sample).line.is_empty());
    let sample = publish(&mut rates, &input, 250);
    assert_eq!((sample.bytes, sample.rate), (Some(3), 12));
}

#[test]
fn eta_warmup_sparse_sample_can_reproduce_the_reported_eta() {
    let catalog = crate::test_catalogs::english();
    let mut rates = Rates::default();
    let mut input = row(1, 0);
    input.kind = "extract".into();
    input.total_bytes = Some(60_817_408);
    publish(&mut rates, &input, 0);
    input.bytes = Some(3);
    let sample = publish(&mut rates, &input, 3000);
    assert_eq!(
        flea::ui_logic_file_ops::joined(&jobs::block(&catalog, &sample).line),
        "3 B of 60.8 MB · 1 B/s · 16893:43:25 left"
    );
}

#[test]
fn eta_warmup_steady_sample_keeps_the_extracting_board_line() {
    let catalog = crate::test_catalogs::english();
    let mut rates = Rates::default();
    let mut input = row(1, 100_000);
    input.kind = "extract".into();
    input.total_bytes = Some(9_700_000);
    publish(&mut rates, &input, 0);
    input.bytes = Some(3_700_000);
    let sample = publish(&mut rates, &input, 1500);
    let block = jobs::block(&catalog, &sample);
    assert_eq!(
        flea::ui_logic_file_ops::joined(&block.line),
        "3.7 MB of 9.7 MB · 2.4 MB/s · 0:02 left"
    );
    assert_eq!(
        block
            .line
            .iter()
            .filter(|part| part.figure)
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>(),
        ["3.7 MB", "9.7 MB", "2.4 MB/s", "0:02"]
    );
}
