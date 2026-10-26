// #264: an extraction's job block carries the count, the entry in flight, the byte bar and the time left (extracting board).
use crate::ui_logic_op1::jobs::{self, Action};
use crate::ui_logic_op1::testcat::Catalog;
use crate::ui_logic_op1::{joined, parse, Event};

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
