// #264: an extraction's entry samples reach the job line with a rate and a time left, and the Jobs card reads them as the extracting board draws them.
use super::opsdispatch::{report_op, Ops};
use super::opsreq::OpMsg;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_op1::{jobs, joined, parse, Event};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc::channel, Arc};
use std::time::{Duration, Instant};

#[test]
fn an_extraction_sample_carries_its_time_left_onto_the_card() {
    let (tx, _rx) = channel();
    let mut ops = Ops::new(tx);
    let mut out = Vec::new();
    ops.jobs.adopt(
        &mut out,
        9,
        "extract",
        "Extracting",
        18,
        Arc::new(AtomicBool::new(false)),
    );
    // Two seconds in at 3.7 MB of 9.7 MB, the board's figures: 1.85 MB/s leaves 3 seconds.
    let job = &mut ops.jobs.rows[0];
    job.started = Instant::now() - Duration::from_secs(2);
    job.reported = Instant::now() - Duration::from_secs(5);
    out.clear();
    report_op(
        &mut out,
        &mut ops,
        OpMsg::JobEntries {
            id: 9,
            done: 6,
            total: 18,
            file: "DSC_4907.jpg".into(),
            bytes: 3_700_000,
            total_bytes: 9_700_000,
            item_bytes: 586_600,
        },
    );
    let text = String::from_utf8(out).unwrap();
    let line = text
        .lines()
        .rev()
        .find(|line| line.contains(r#""t":"job""#))
        .unwrap_or_else(|| panic!("no job line in {text}"));
    let Some(Event::Job { job, .. }) = parse(line) else {
        panic!("not a job line: {line}");
    };
    assert_eq!(job.kind, "extract");
    // The byte rate, 3.7 MB over two seconds, leaves 6.0 MB for three seconds; an entry rate would leave four.
    assert!((1_700_000..=1_850_000).contains(&job.rate), "{line}");
    assert_eq!(job.left, Some(3), "{line}");
    let catalog = I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::i18n::test_catalogs::english_only()],
    });
    let block = jobs::block(&catalog, &job);
    assert_eq!(block.head, "Extracting 7 of 18");
    assert_eq!(block.file, "DSC_4907.jpg · 586.6 kB");
    let said = joined(&block.line);
    assert!(
        said.starts_with("3.7 MB of 9.7 MB · ") && said.ends_with(" · 0:03 left"),
        "the card's byte line ends in the time left: {said}"
    );
    assert!((block.bar.unwrap() - 3.7 / 9.7).abs() < 1e-9);
}
