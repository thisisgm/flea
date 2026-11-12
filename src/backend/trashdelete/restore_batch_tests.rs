use super::*;
use crate::backend::testdir::TestDir;
use recovery::batch::{Batch, FAIL, PROBE};

fn fixture(d: &TestDir, count: usize) -> Vec<(Reviewed, PathBuf)> {
    d.dir("files");
    d.dir("info");
    d.dir("back");
    d.dir("recovery");
    (0..count)
        .map(|index| {
            let original = d.join(&format!("back/item{index}"));
            d.file(
                &format!("info/item{index}.trashinfo"),
                &format!("[Trash Info]\nPath={}\n", original.display()),
            );
            let path = d.file(&format!("files/item{index}"), &format!("payload{index}"));
            let metadata = path.symlink_metadata().unwrap();
            let reviewed =
                Reviewed::inspect(path, &format!("l{}:{}", metadata.dev(), metadata.ino()))
                    .unwrap();
            (reviewed, original)
        })
        .collect()
}

fn restore(d: &TestDir, items: &[(Reviewed, PathBuf)]) -> Vec<Result<(), Refusal>> {
    let folders = FolderSyncs::open();
    let requests: Vec<_> = items
        .iter()
        .map(|(item, original)| (item, original.as_path()))
        .collect();
    let results = restore_many(&requests, &d.join("recovery"));
    drop(folders);
    results
}

struct Fault;
impl Fault {
    fn at(phase: &str) -> Self {
        FAIL.with(|fail| *fail.borrow_mut() = Some(phase.to_string()));
        Self
    }
}
impl Drop for Fault {
    fn drop(&mut self) {
        FAIL.with(|fail| *fail.borrow_mut() = None);
    }
}

fn recovered(d: &TestDir, count: usize) {
    let report = recover(&d.join("recovery")).unwrap();
    assert_eq!(report.recovered, count);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 0);
}

#[test]
fn restore_batch_confirms_four_phases_for_many_items() {
    let d = TestDir::new("restore-batch-barriers");
    let items = fixture(&d, 8);
    PROBE.with(|probe| probe.borrow_mut().clear());
    recovery::ITEM_BARRIERS.with(|count| count.set(0));
    assert!(restore(&d, &items).iter().all(Result::is_ok));
    assert_eq!(
        recovery::ITEM_BARRIERS.with(|count| count.get()),
        0,
        "no per-item record barriers in a restore batch"
    );
    assert_eq!(
        PROBE.with(|probe| probe.borrow().clone()),
        [
            "preparation",
            "completed moves",
            "empty quarantines",
            "completion markers"
        ]
    );
    assert_eq!(std::fs::read_dir(d.join("files")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(d.join("info")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 0);
    for (index, (_, original)) in items.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(original).unwrap(),
            format!("payload{index}")
        );
    }
}

#[test]
fn restore_batch_refuses_every_claim_when_preparation_is_unconfirmed() {
    for phase in ["include", "preparation"] {
        let d = TestDir::new("restore-batch-prepare-fail");
        let items = fixture(&d, 3);
        let fault = Fault::at(phase);
        for result in restore(&d, &items) {
            assert_eq!(
                result.unwrap_err().outcome(),
                &Outcome::NoRecord(Op::Restore)
            );
        }
        drop(fault);
        for (item, original) in &items {
            assert!(item.path.exists() && item.info.exists() && !original.exists());
        }
        assert_eq!(
            std::fs::read_dir(d.join("files")).unwrap().count(),
            items.len(),
            "an unconfirmed preparation leaves no quarantine behind"
        );
        assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 0);
    }
}

#[test]
fn restore_batch_restores_included_claims_when_a_later_include_fails() {
    let d = TestDir::new("restore-batch-later-include");
    let items = fixture(&d, 3);
    let fault = Fault::at("later include");
    let results = restore(&d, &items);
    drop(fault);
    assert!(results[0].is_ok());
    for result in &results[1..] {
        assert_eq!(
            result.as_ref().unwrap_err().outcome(),
            &Outcome::NoRecord(Op::Restore)
        );
    }
    assert_eq!(std::fs::read_to_string(&items[0].1).unwrap(), "payload0");
    for (item, original) in &items[1..] {
        assert!(item.path.exists() && item.info.exists() && !original.exists());
    }
    assert_eq!(std::fs::read_dir(d.join("files")).unwrap().count(), 2);
    assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 0);
}

#[test]
fn restore_batch_preserves_recovery_at_all_completion_barrier_failures() {
    for phase in ["completed moves", "empty quarantines", "completion markers"] {
        let d = TestDir::new("restore-batch-finish-fail");
        let items = fixture(&d, 3);
        let fault = Fault::at(phase);
        for result in restore(&d, &items) {
            assert_eq!(
                result.unwrap_err().outcome(),
                &Outcome::RecordLeft(Some(Op::Restore))
            );
        }
        drop(fault);
        if phase == "completed moves" {
            for entry in std::fs::read_dir(d.join("files")).unwrap() {
                assert!(
                    entry.unwrap().path().join("metadata").is_file(),
                    "paired metadata must survive an unconfirmed payload move"
                );
            }
        }
        assert_eq!(
            std::fs::read_dir(d.join("recovery")).unwrap().count(),
            items.len()
        );
        for (index, (item, original)) in items.iter().enumerate() {
            assert!(!item.path.exists());
            assert_eq!(
                std::fs::read_to_string(original).unwrap(),
                format!("payload{index}")
            );
        }
        recovered(&d, items.len());
    }
}

#[test]
fn restore_batch_crash_before_and_during_claims_replays_existing_records() {
    for stage in 0..4 {
        let d = TestDir::new("restore-batch-interrupted");
        let items = fixture(&d, 3);
        let mut batch = Batch::default();
        let mut prepared: Vec<_> = items
            .iter()
            .enumerate()
            .map(|(index, (item, original))| {
                let prepared = item
                    .prepare_claim(
                        Op::Restore,
                        &d.join("recovery"),
                        Some(original),
                        Some(batch.ticket(index)),
                    )
                    .unwrap();
                batch.include(&prepared.journal, &prepared.files).unwrap();
                prepared
            })
            .collect();
        batch.prepare().unwrap();
        if stage == 3 {
            let first = prepared.remove(0);
            items[0]
                .0
                .restore_prepared(&items[0].1, first, || {})
                .unwrap();
        } else if stage > 0 {
            let (item, _) = &items[0];
            for (from, to) in [
                (&item.path, prepared[0].recovery.join("payload")),
                (&item.info, prepared[0].recovery.join("metadata")),
            ] {
                d.assert_contains(from);
                d.assert_contains(&to);
                std::fs::rename(from, to).unwrap();
            }
        }
        if stage == 2 {
            let payload = prepared[0].recovery.join("payload");
            d.assert_contains(&payload);
            d.assert_contains(&items[0].1);
            std::fs::rename(payload, &items[0].1).unwrap();
        }
        assert_eq!(
            recover(&d.join("recovery")).unwrap().recovered,
            0,
            "active records stay locked"
        );
        drop(prepared);
        drop(batch);
        recovered(&d, items.len());
        for (index, (item, original)) in items.iter().enumerate() {
            let at = if index == 0 && stage >= 2 {
                original
            } else {
                &item.path
            };
            assert_eq!(
                std::fs::read_to_string(at).unwrap(),
                format!("payload{index}")
            );
            assert_eq!(item.info.exists(), !(index == 0 && stage >= 2));
        }
    }
}

#[test]
fn restore_batch_bounds_each_confirmation_to_sixty_four_items() {
    // One batch at the bound and two just past it pin the bound exactly.
    for (count, phases) in [(64, 4), (65, 8)] {
        let d = TestDir::new("restore-batch-bound");
        let items = fixture(&d, count);
        PROBE.with(|probe| probe.borrow_mut().clear());
        assert!(restore(&d, &items).iter().all(Result::is_ok));
        assert_eq!(
            PROBE.with(|probe| probe.borrow().len()),
            phases,
            "{count} items"
        );
        assert!(items.iter().all(|(_, original)| original.exists()));
    }
}

#[test]
fn restore_batch_rechecks_claimed_identity_after_batch_preparation() {
    let d = TestDir::new("restore-batch-stale");
    let items = fixture(&d, 2);
    let (item, original) = &items[0];
    let mut batch = Batch::default();
    let prepared = item
        .prepare_claim(
            Op::Restore,
            &d.join("recovery"),
            Some(original),
            Some(batch.ticket(0)),
        )
        .unwrap();
    batch.include(&prepared.journal, &prepared.files).unwrap();
    batch.prepare().unwrap();
    d.replace_file(&item.path, "new arrival");
    assert!(item.restore_prepared(original, prepared, || {}).is_err());
    assert!(batch.finish().is_empty());
    assert_eq!(std::fs::read_to_string(&item.path).unwrap(), "new arrival");
    assert!(item.info.exists() && !original.exists());
    assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 0);
}

#[test]
fn restore_batch_keeps_a_changed_quarantine_at_deferred_cleanup() {
    let d = TestDir::new("restore-batch-cleanup-race");
    let items = fixture(&d, 2);
    let mut batch = Batch::default();
    let prepared: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, (item, original))| {
            let prepared = item
                .prepare_claim(
                    Op::Restore,
                    &d.join("recovery"),
                    Some(original),
                    Some(batch.ticket(index)),
                )
                .unwrap();
            batch.include(&prepared.journal, &prepared.files).unwrap();
            prepared
        })
        .collect();
    let changed = prepared[0].recovery.clone();
    batch.prepare().unwrap();
    for ((item, original), prepared) in items.iter().zip(prepared) {
        item.restore_prepared(original, prepared, || {}).unwrap();
    }
    let stray = changed.join("new arrival");
    d.assert_contains(&stray);
    std::fs::write(&stray, "keep").unwrap();
    let errors = batch.finish();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, 0);
    assert_eq!(
        errors[0].1.outcome(),
        &Outcome::RecordLeft(Some(Op::Restore))
    );
    assert_eq!(std::fs::read_to_string(stray).unwrap(), "keep");
    assert_eq!(std::fs::read_dir(d.join("recovery")).unwrap().count(), 1);
    for (index, (_, original)) in items.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(original).unwrap(),
            format!("payload{index}")
        );
    }
}

#[test]
fn restore_batch_keeps_collisions_and_raw_names_and_continues_after_a_failure() {
    let d = TestDir::new("restore-batch-collision");
    let mut items = fixture(&d, 4);
    d.file("back/item1", "keep collision");
    let raw = OsString::from_vec(b"raw\xff".to_vec());
    items[2].1 = d.join("back").join(raw);
    let encoded = items[2]
        .1
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect::<String>();
    d.file(
        "info/item2.trashinfo",
        &format!("[Trash Info]\nPath={encoded}\n"),
    );
    let path = items[2].0.path.clone();
    let metadata = path.symlink_metadata().unwrap();
    items[2].0 =
        Reviewed::inspect(path, &format!("l{}:{}", metadata.dev(), metadata.ino())).unwrap();
    let results = restore(&d, &items);
    assert!(results[0].is_ok() && results[1].is_err() && results[2].is_ok() && results[3].is_ok());
    assert_eq!(
        std::fs::read_to_string(&items[1].1).unwrap(),
        "keep collision"
    );
    assert_eq!(
        std::fs::read_to_string(&items[1].0.path).unwrap(),
        "payload1"
    );
    assert_eq!(std::fs::read_to_string(&items[2].1).unwrap(), "payload2");
    assert!(items[1].0.info.exists());
    assert_eq!(recover(&d.join("recovery")).unwrap().recovered, 0);
}
