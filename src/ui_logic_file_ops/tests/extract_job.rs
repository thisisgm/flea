// #264: an extraction's job block carries the count, the entry in flight, the byte bar and the time left (extracting board).
use crate::ui_logic_file_ops::jobs::{self, Action};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::{joined, parse, Event};

#[test]
fn a_running_extract_block_reads_as_the_extracting_board_draws_it() {
    let cat = Catalog::english();
    let line = r#"{"t":"job","job":{"id":9,"kind":"extract","head":"Extracting 7 of 18","file":"DSC_4907.jpg","done":6,"total":18,"bytes":3700000,"totalBytes":9700000,"itemBytes":586600,"rate":2400000,"left":2,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#;
    let Some(Event::Job { job, .. }) = parse(line) else {
        panic!("not a job line");
    };
    let block = jobs::block(&cat.i18n, &job);
    assert_eq!(block.head, "Extracting 7 of 18");
    assert_eq!(block.file, "DSC_4907.jpg · 586.6 kB");
    assert_eq!(
        joined(&block.line),
        "3.7 MB of 9.7 MB · 2.4 MB/s · 0:02 left"
    );
    assert!((block.bar.unwrap() - 3.7 / 9.7).abs() < 1e-9);
    assert_eq!(block.buttons, vec![Action::Cancel]);
}

// EY-326: an extract the backend ends as encrypted opens the password card, so the Jobs card draws no failure for it.
#[test]
fn an_encrypted_extract_draws_no_failed_block() {
    let row = |id: u64, reason: &str| {
        let line = format!(
            r#"{{"t":"job","job":{{"id":{id},"kind":"extract","head":"Extracting locked.7z","file":"","done":0,"total":1,"state":"failed","reason":"{reason}","output":[],"undo":false,"cancelled":false}},"unfinished":0}}"#
        );
        let Some(Event::Job { job, .. }) = parse(&line) else {
            panic!("not a job line");
        };
        job
    };
    let mut held = jobs::Jobs::default();
    held.update(row(4, "encrypted"));
    assert_eq!(jobs::state(&held.rows[0]), jobs::State::Done);
    assert!(
        !held.visible(),
        "the password card asks; no failed block is drawn"
    );
    held.update(row(5, "no space left on device"));
    assert_eq!(held.blocks().map(|r| r.id).collect::<Vec<_>>(), vec![5]);
    // A wrong password asks again on the card; only an extract is hidden, never another job's failure.
    assert_eq!(jobs::state(&row(6, "wrong-password")), jobs::State::Done);
    let mut other = row(7, "encrypted");
    other.kind = "compress".into();
    assert_eq!(jobs::state(&other), jobs::State::Failed);
}

// FX-ARCHUI follow-up: the card read the backend's lower-case clause where the footer said it as a sentence.
#[test]
fn a_failed_extract_reads_its_reason_as_a_sentence() {
    let cat = Catalog::english();
    let line = r#"{"t":"job","job":{"id":8,"kind":"extract","head":"Extracting dotdot.zip","file":"","done":0,"total":1,"state":"failed","reason":"an entry in this archive would land outside its folder, so nothing was extracted","output":[],"undo":false,"cancelled":false},"unfinished":0}"#;
    let Some(Event::Job { job, .. }) = parse(line) else {
        panic!("not a job line");
    };
    let block = jobs::block(&cat.i18n, &job);
    assert_eq!(block.head, "Extraction failed");
    assert_eq!(
        block.file,
        "An entry in this archive would land outside its folder, so nothing was extracted."
    );
}

#[test]
fn a_failed_copy_keeps_the_item_s_literal_name() {
    let cat = Catalog::english();
    let line = r#"{"t":"job","job":{"id":9,"kind":"copy","state":"failed","reason":"report.txt: permission denied"}}"#;
    let Some(Event::Job { job, .. }) = parse(line) else {
        panic!("not a job line");
    };
    assert_eq!(
        jobs::block(&cat.i18n, &job).file,
        "report.txt: permission denied"
    );
}
