use super::*;
use crate::backend::{collide::Table, durable::Durability, movebatch, testdir::TestDir};
use std::sync::mpsc::channel;

#[test]
fn transfer_counts_keep_distinct_wire_fields() {
    assert_eq!(
        transferdone_line(
            71,
            TransferCounts {
                ok: 2,
                failed: 3,
                skipped: 5
            },
            false,
            &[],
            false,
            "",
            &[],
        ),
        r#"{"t":"transferdone","id":71,"ok":2,"failed":3,"skipped":5,"cancelled":false,"retryPaths":[],"durable":false,"note":""}"#,
    );
}

#[test]
fn transfer_item_keeps_request_index_and_path_direction() {
    let root = TestDir::new("transfer-item-fields");
    let src = root.file("source", "payload");
    let dst = root.join("destination");
    let source = ItemIdentity::inspect(&src).unwrap();
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let pace = Pace::new();
    let table = Table::read(root.path());
    let mut steps = Vec::new();
    let mut durability = Durability::begin(root.path());
    let (tx, rx) = channel();
    transferitem::one_item(
        TransferItem {
            id: 71,
            index: 3,
            name: "source",
            src: &src,
            dst: &dst,
            source,
        },
        false,
        &flag,
        &tx,
        (&settled, &pace, &table),
        &mut steps,
        &mut durability,
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "payload");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "payload");
    assert!(matches!(&steps[..], [Step::Copied { from, to, .. }] if from == &src && to == &dst));
    drop(tx);
    assert!(rx.iter().any(|message| matches!(message,
        OpMsg::Progress { id: 71, index: 3, name, .. } if name == "source")));
}

#[test]
fn staged_move_keeps_request_index_and_path_direction() {
    let root = TestDir::new("move-item-fields");
    let src = root.file("source", "payload");
    let out = root.dir("out");
    let dst = out.join("destination");
    let source = ItemIdentity::inspect(&src).unwrap();
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut steps = Vec::new();
    let mut durability = Durability::begin(&out);
    let mut batch = movebatch::MoveBatch::new();
    let (tx, rx) = channel();
    assert!(matches!(
        movebatch::stage_copy(
            TransferItem {
                id: 91,
                index: 7,
                name: "source",
                src: &src,
                dst: &dst,
                source
            },
            &flag,
            &tx,
            &settled,
            &mut steps,
            &mut durability,
            &mut batch,
        ),
        movebatch::MoveOutcome::Deferred
    ));
    let (counts, retry) = movebatch::close_normal(&mut batch, 91, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (1, 0, 0));
    assert!(retry.is_empty());
    assert!(!src.exists());
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "payload");
    assert!(matches!(&steps[..], [Step::Moved { from, to, .. }] if from == &src && to == &dst));
    drop(tx);
    assert!(rx.iter().any(|message| matches!(message,
        OpMsg::Item { id: 91, index: 7, name, ok: true, .. } if name == "source")));
}
