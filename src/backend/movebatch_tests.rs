use super::*;
use crate::backend::collide::CANCELLED;
use crate::backend::testdir::TestDir;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::sync::mpsc::channel;

fn staged(
    id: usize,
    batch: &mut MoveBatch,
    durability: &mut Durability,
    flag: &AtomicBool,
    tx: &std::sync::mpsc::Sender<OpMsg>,
    settled: &AtomicU64,
    steps: &mut Vec<Step>,
    index: usize,
    src: &Path,
    dst: &Path,
) {
    let source = ItemIdentity::inspect(src).expect("source recorded before its copy");
    let name = src.file_name().unwrap().to_string_lossy().to_string();
    match stage_copy(id, index, &name, src, dst, source, flag, tx, settled, steps, durability, batch) {
        MoveOutcome::Deferred => {}
        MoveOutcome::Done(r) => panic!("staging copies, got {}", r.map(|_| "ok").unwrap_err().msg),
    }
}

fn items(rx: std::sync::mpsc::Receiver<OpMsg>) -> Vec<(usize, bool, String)> {
    let mut out = Vec::new();
    for msg in rx.iter() {
        if let OpMsg::Item { index, ok, err, .. } = msg {
            out.push((index, ok, err));
        }
    }
    out
}

// 64 items into one folder confirm it once, not 64 times; the file fsyncs stay per file.
#[test]
fn a_batch_of_sixty_four_confirms_its_folder_once() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-once");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..64 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    assert!(batch.full(), "64 staged items fill the batch");
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (64, 0, 0));
    assert!(retry.is_empty());
    assert_eq!(steps.len(), 64, "one Moved step per item, the journal redo already reads");
    assert_eq!(durable::test_counts(), (64, 1), "one file fsync per file, one folder confirm: {:?}", durable::test_counts());
    assert_eq!(durable::test_syncfs_count(), 0, "no syncfs off a block stick");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 64);
    assert!(lines.iter().enumerate().all(|(n, (index, ok, _))| *index == n && *ok));
    for n in 0..64 {
        assert!(!srcdir.join(format!("f{n}.txt")).exists(), "no source survives its batch's confirm");
        assert_eq!(std::fs::read_to_string(out.join(format!("f{n}.txt"))).unwrap(), "body");
    }
    durable::test_reset();
}

// A failed confirm keeps every source of the batch whole and journals each copy as a partial.
#[test]
fn a_failed_batch_confirm_keeps_every_source_and_journals_partials() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-confirm-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for (n, body) in ["first", "second"].iter().enumerate() {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, body).unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    durable::test_set_fail_dirs(true);
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    durable::test_set_fail_dirs(false);
    assert_eq!((counts.ok, counts.failed), (0, 2));
    assert_eq!(retry.len(), 2, "every unconfirmed source is offered again");
    assert!(matches!(&steps[..], [Step::Copied { .. }, Step::Copied { .. }]), "partials, as the single-item failure journals: {:?}", steps.len());
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 2);
    for (n, body) in ["first", "second"].iter().enumerate() {
        assert!(lines.iter().any(|(index, ok, err)| *index == n && !ok && err == durable::DIR_UNCONFIRMED));
        assert_eq!(std::fs::read_to_string(srcdir.join(format!("f{n}.txt"))).unwrap(), *body, "the source stays whole");
        assert_eq!(std::fs::read_to_string(out.join(format!("f{n}.txt"))).unwrap(), *body, "the landed copy stays beside it");
        d.assert_contains(&srcdir.join(format!("f{n}.txt")));
    }
    durable::test_reset();
}

// Cancel completes what already copied instead of deleting it: sources go, copies stay.
#[test]
fn a_cancelled_batch_completes_its_landed_moves() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-cancel");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..2 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    let (counts, retry) = close_cancelled(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert!(counts.cancelled);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (2, 0, 0));
    assert!(retry.is_empty(), "a completed move is never retried");
    assert!(matches!(&steps[..], [Step::Moved { .. }, Step::Moved { .. }]), "redo sees moved steps only");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|(_, ok, _)| *ok), "landed items complete, so every line is ok");
    for n in 0..2 {
        assert!(!srcdir.join(format!("f{n}.txt")).exists(), "no source survives its batch's confirm");
        assert_eq!(std::fs::read_to_string(out.join(format!("f{n}.txt"))).unwrap(), "body");
    }
    durable::test_reset();
}

// Batched Moved steps undo exactly like single-item ones: sources come back, copies go.
#[test]
fn batched_moves_undo_like_single_item_moves() {
    use crate::backend::{durable, undo::Journal};
    durable::test_reset();
    let d = TestDir::new("movebatch-undo");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, _rx) = channel();
    let mut steps = Vec::new();
    for n in 0..2 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    let (counts, _) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!(counts.ok, 2);
    let mut journal = Journal::new();
    journal.push(crate::backend::undo::Entry { op: "move".to_string(), steps });
    assert_eq!(journal.undo().unwrap(), "move");
    for n in 0..2 {
        assert_eq!(std::fs::read_to_string(srcdir.join(format!("f{n}.txt"))).unwrap(), "body");
        assert!(!out.join(format!("f{n}.txt")).exists());
    }
    durable::test_reset();
}

// A source replaced after its copy landed is never removed: both copies stay and undo loses neither.
#[test]
fn a_replaced_batch_source_survives_close_with_both_copies_kept() {
    use crate::backend::{durable, undo::Journal};
    durable::test_reset();
    let d = TestDir::new("movebatch-changed-source");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    let first = srcdir.join("f0.txt");
    std::fs::write(&first, "old").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &first, &out.join("f0.txt"));
    let second = srcdir.join("f1.txt");
    std::fs::write(&second, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 1, &second, &out.join("f1.txt"));
    // Atomically replaced after its copy landed, so a new inode sits at the staged name.
    let repl = srcdir.join("replacement.txt");
    std::fs::write(&repl, "new").unwrap();
    std::fs::rename(&repl, &first).unwrap();
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (1, 1, 0));
    assert_eq!(retry.len(), 1, "the changed source is offered again");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|(index, ok, _)| *index == 1 && *ok));
    let reported = lines.iter().find(|(index, ok, _)| *index == 0 && !ok).expect("the changed item reports its own line");
    assert!(reported.2.contains("changed during the move; both kept"), "one line names the file: {}", reported.2);
    assert!(first.exists(), "the replacement survives");
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "new", "the replacement survives");
    assert_eq!(std::fs::read_to_string(out.join("f0.txt")).unwrap(), "old", "beside the stale copy");
    assert!(!second.exists() && std::fs::read_to_string(out.join("f1.txt")).unwrap() == "body");
    assert!(matches!(&steps[..], [Step::Copied { .. }, Step::Moved { .. }]), "the kept copy journals a Copied step");
    let mut journal = Journal::new();
    journal.push(crate::backend::undo::Entry { op: "move".to_string(), steps });
    journal.undo().unwrap();
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "new", "undo never touches the replacement");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "body", "and the clean move comes back");
    assert!(!out.join("f0.txt").exists() && !out.join("f1.txt").exists());
    durable::test_reset();
}

// A source the batch cannot re-check is never removed: a transient lstat error proves nothing.
#[test]
fn an_unverifiable_batch_source_keeps_both_names() {
    use crate::backend::{durable, undo::Journal};
    durable::test_reset();
    let d = TestDir::new("movebatch-inspect-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    let src = srcdir.join("f0.txt");
    std::fs::write(&src, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &src, &out.join("f0.txt"));
    let _inspect = InspectFailGuard::hold();
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (0, 1, 0));
    assert_eq!(retry.len(), 1, "the unverified source is offered again");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].1, "an unverified source is never removed");
    assert!(lines[0].2.contains("could not verify f0.txt"), "one line names the file: {}", lines[0].2);
    assert!(lines[0].2.contains("both kept"), "and its reason: {}", lines[0].2);
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "body", "the source survives");
    assert_eq!(std::fs::read_to_string(out.join("f0.txt")).unwrap(), "body", "beside the staged copy");
    assert!(matches!(&steps[..], [Step::Copied { .. }]), "the kept copy journals a Copied step");
    let mut journal = Journal::new();
    journal.push(crate::backend::undo::Entry { op: "move".to_string(), steps });
    journal.undo().unwrap();
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "body", "undo never touches the source");
    assert!(!out.join("f0.txt").exists(), "and takes the staged copy");
    durable::test_reset();
}

// A source gone before its batch closes keeps its copy: undo journals nothing and removes nothing.
#[test]
fn a_vanished_batch_source_keeps_its_copy_and_journals_nothing() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-vanished-source");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    let src = srcdir.join("f0.txt");
    std::fs::write(&src, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &src, &out.join("f0.txt"));
    std::fs::remove_file(&src).unwrap();
    let (counts, _) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (0, 1, 0));
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].1, "a vanished source is never removed");
    assert!(lines[0].2.contains("f0.txt was gone before the move finished; the copy stays"), "one line names the file: {}", lines[0].2);
    assert!(steps.is_empty(), "no step journals a copy whose source is gone: {:?}", steps.len());
    assert_eq!(std::fs::read_to_string(out.join("f0.txt")).unwrap(), "body", "the staged copy stays");
    durable::test_reset();
}

// A stale NFS handle at close reads as gone too, so it journals nothing undo could remove the copy with.
#[test]
fn a_stale_handle_at_close_counts_as_gone_and_journals_nothing() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-stale-source");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    let src = srcdir.join("f0.txt");
    std::fs::write(&src, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &src, &out.join("f0.txt"));
    let (counts, _) = {
        let _stale = InspectFailGuard::hold_errno(super::ESTALE);
        close_normal(&mut batch, 1, &tx, &mut steps, &mut durability)
    };
    assert_eq!((counts.ok, counts.failed, counts.skipped), (0, 1, 0));
    drop(tx);
    let lines = items(rx);
    assert!(lines[0].2.contains("f0.txt was gone before the move finished; the copy stays"), "ESTALE reads as gone: {}", lines[0].2);
    assert!(steps.is_empty(), "no step journals a copy whose source handle went stale: {:?}", steps.len());
    assert!(src.exists() && out.join("f0.txt").exists(), "nothing is removed on either side");
    durable::test_reset();
}

// A source rewritten in place keeps its inode, so only the size and time re-check catches it.
#[test]
fn a_batch_source_edited_in_place_survives_close_with_both_copies_kept() {
    use crate::backend::{durable, undo::Journal};
    durable::test_reset();
    let d = TestDir::new("movebatch-edited-source");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    let first = srcdir.join("f0.txt");
    std::fs::write(&first, "old").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &first, &out.join("f0.txt"));
    let second = srcdir.join("f1.txt");
    std::fs::write(&second, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 1, &second, &out.join("f1.txt"));
    // Appended after its copy landed, so the same inode holds new bytes.
    let ino = first.symlink_metadata().unwrap().ino();
    std::fs::OpenOptions::new().append(true).open(&first).unwrap().write_all(b"more").unwrap();
    assert_eq!(first.symlink_metadata().unwrap().ino(), ino, "the edit stayed in place");
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (1, 1, 0));
    assert_eq!(retry.len(), 1, "the edited source is offered again");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|(index, ok, _)| *index == 1 && *ok));
    let reported = lines.iter().find(|(index, ok, _)| *index == 0 && !ok).expect("the edited item reports its own line");
    assert!(reported.2.contains("changed during the move; both kept"), "one line names the file: {}", reported.2);
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "oldmore", "the edit survives");
    assert_eq!(std::fs::read_to_string(out.join("f0.txt")).unwrap(), "old", "beside the stale copy");
    assert!(!second.exists() && std::fs::read_to_string(out.join("f1.txt")).unwrap() == "body");
    assert!(matches!(&steps[..], [Step::Copied { .. }, Step::Moved { .. }]), "the kept copy journals a Copied step");
    let mut journal = Journal::new();
    journal.push(crate::backend::undo::Entry { op: "move".to_string(), steps });
    journal.undo().unwrap();
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "oldmore", "undo never touches the edit");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "body", "and the clean move comes back");
    assert!(!out.join("f0.txt").exists() && !out.join("f1.txt").exists());
    durable::test_reset();
}

// A same-filesystem move never stages: the rename answers at once with no batch behind it.
#[test]
fn same_filesystem_moves_answer_at_once_and_never_stage() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-rename");
    let out = d.dir("out");
    let mut durability = durable::Durability::begin(&out);
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let (tx, _rx) = channel();
    let mut steps = Vec::new();
    let mut batch = MoveBatch::new();
    let src = d.file("a.txt", "body");
    let source = ItemIdentity::inspect(&src).unwrap();
    let dst = out.join("a.txt");
    match land_move(1, 0, "a.txt", &src, &dst, source, &flag, &tx, &settled, &mut steps, &mut durability, &mut batch) {
        MoveOutcome::Done(Ok(())) => {}
        other => panic!("a rename answers at once, got {}", if matches!(other, MoveOutcome::Deferred) { "deferred" } else { "an error" }),
    }
    assert!(batch.is_empty(), "no batch behind a rename");
    assert!(!src.exists() && std::fs::read_to_string(&dst).unwrap() == "body");
    assert!(matches!(&steps[..], [Step::Moved { .. }]));
    // A move onto an existing name refuses with the rename's own error, still with no batch.
    let src = d.file("b.txt", "source");
    std::fs::write(&dst, "taken").unwrap();
    let source = ItemIdentity::inspect(&src).unwrap();
    let err = match land_move(1, 1, "b.txt", &src, &dst, source, &flag, &tx, &settled, &mut steps, &mut durability, &mut batch) {
        MoveOutcome::Done(Err(e)) => e,
        _ => panic!("a clobbering move must refuse"),
    };
    assert_eq!(err.where_, "rename");
    assert!(batch.is_empty());
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "taken");
    durable::test_reset();
}

// One drain for transfer-level cases: every item line plus the done counts, steps and retry.
struct Collected {
    lines: Vec<(usize, bool, String)>,
    ok: usize,
    failed: usize,
    skipped: usize,
    cancelled: bool,
    steps: Vec<Step>,
    retry: Vec<(PathBuf, ItemIdentity)>,
}

fn collect(rx: std::sync::mpsc::Receiver<OpMsg>) -> Collected {
    let mut out = Collected { lines: Vec::new(), ok: 0, failed: 0, skipped: 0, cancelled: false, steps: Vec::new(), retry: Vec::new() };
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { index, ok, err, .. } => out.lines.push((index, ok, err)),
            OpMsg::TransferDone { ok, failed, skipped, cancelled, entry, retry, .. } => {
                out.ok = ok;
                out.failed = failed;
                out.skipped = skipped;
                out.cancelled = cancelled;
                out.steps = entry.steps;
                out.retry = retry;
            }
            _ => {}
        }
    }
    out
}

// A cancel landing mid-batch completes what already copied and still reports cancelled.
#[test]
fn a_cancelled_move_transfer_reports_cancelled_with_landed_complete() {
    use std::sync::Arc;
    let _force = ForceCopyGuard::hold();
    let d = TestDir::new("movebatch-cancel-transfer");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    let names: Vec<String> = (0..4).map(|n| format!("s{n}.txt")).collect();
    for name in &names {
        std::fs::write(srcdir.join(name), "small").unwrap();
    }
    let paths: Vec<String> = names.iter().map(|n| srcdir.join(n).to_string_lossy().into_owned()).collect();
    // The flag lands after the loop-top check of item 2, the way a real cancel lands mid-item.
    let _cancel_at = crate::backend::opsreq::CancelAtGuard::hold(2);
    let (tx, rx) = channel();
    crate::backend::opsreq::run_transfer_checked(1, true, paths, out.clone(), Arc::new(AtomicBool::new(false)), tx,
        None, None, crate::backend::collide::Policy::default());
    let got = collect(rx);
    assert!(got.cancelled, "a pressed cancel answers cancelled whatever the last item hit");
    assert_eq!((got.ok, got.failed, got.skipped), (2, 0, 2));
    assert_eq!(got.lines.iter().map(|(index, ok, _)| (*index, *ok)).collect::<Vec<_>>(),
        vec![(0, true), (1, true), (2, false)], "landed items complete in order, then the cancelled one");
    assert_eq!(got.lines[2].2, CANCELLED, "the item the cancel landed on says so");
    assert!(got.steps.iter().all(|s| matches!(s, Step::Moved { .. })), "redo sees moved steps only");
    assert_eq!(got.steps.len(), got.ok);
    for name in &names[..2] {
        assert!(!srcdir.join(name).exists(), "a completed move leaves nothing at its source: {name}");
        assert!(out.join(name).exists(), "and its copy stays: {name}");
    }
    for name in &names[2..] {
        assert!(srcdir.join(name).exists(), "an unstarted item keeps its source whole: {name}");
        assert!(!out.join(name).exists(), "and copies nothing: {name}");
    }
    let mut journal = crate::backend::undo::Journal::new();
    journal.push(crate::backend::undo::Entry { op: "move".to_string(), steps: got.steps });
    journal.undo().unwrap();
    for name in &names {
        assert!(srcdir.join(name).exists(), "undo brings every source back: {name}");
        assert!(!out.join(name).exists(), "and takes every landed copy: {name}");
    }
}

// Directories and symlinks never stage: they move alone by rename, keeping their inode.
#[test]
fn non_regular_items_close_the_batch_and_move_alone() {
    let _force = ForceCopyGuard::hold();
    let d = TestDir::new("movebatch-nonregular");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    let a = srcdir.join("a.txt");
    std::fs::write(&a, "a").unwrap();
    let b = srcdir.join("b.txt");
    std::fs::write(&b, "b").unwrap();
    let tree = srcdir.join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::fs::write(tree.join("inner.txt"), "inner").unwrap();
    let target = srcdir.join("target.txt");
    std::fs::write(&target, "target").unwrap();
    let link = srcdir.join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let tree_ino = tree.symlink_metadata().unwrap().ino();
    let link_ino = link.symlink_metadata().unwrap().ino();
    let paths = [a, tree, link, b].iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
    let (tx, rx) = channel();
    crate::backend::opsreq::run_transfer_checked(1, true, paths, out.clone(),
        std::sync::Arc::new(AtomicBool::new(false)), tx, None, None, crate::backend::collide::Policy::default());
    let got = collect(rx);
    assert_eq!((got.ok, got.failed, got.skipped, got.cancelled), (4, 0, 0, false));
    assert_eq!(out.join("tree").symlink_metadata().unwrap().ino(), tree_ino, "a directory moves by rename, not by copy");
    assert_eq!(out.join("link.txt").symlink_metadata().unwrap().ino(), link_ino, "and so does a symlink");
    assert_eq!(std::fs::read_to_string(out.join("tree/inner.txt")).unwrap(), "inner");
    assert_eq!(std::fs::read_to_string(out.join("a.txt")).unwrap(), "a");
    assert_eq!(std::fs::read_to_string(out.join("b.txt")).unwrap(), "b");
    assert!(got.steps.iter().all(|s| matches!(s, Step::Moved { .. })));
    assert_eq!(got.steps.len(), 4);
    assert_eq!(got.lines.iter().map(|(index, ok, _)| (*index, *ok)).collect::<Vec<_>>(),
        vec![(0, true), (1, true), (2, true), (3, true)], "a staged file's line lands before the directory that follows it");
    let order: Vec<String> = got.steps.iter().map(|step| match step {
        Step::Moved { from, .. } => from.file_name().unwrap().to_string_lossy().into_owned(),
        _ => panic!("every batched move journals Moved: {step:?}"),
    }).collect();
    assert_eq!(order, vec!["a.txt", "tree", "link.txt", "b.txt"], "and the steps journal in input order");
}

// A 64-file batch on a batch_syncfs stick skips every file fsync for one syncfs and one folder fsync.
#[test]
fn a_sixty_four_batch_on_a_block_vfat_stick_skips_file_fsyncs() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-syncfs");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..64 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    assert_eq!(durability.held_len(), 0, "the 64th hold already released the batch");
    assert_eq!(durable::test_releases(), 64, "64 closes on scoped threads: {:?}", durable::test_order());
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, counts.skipped), (64, 0, 0));
    assert!(retry.is_empty());
    assert_eq!(durable::test_counts().0, 0, "no file fsync on a batch stick: {:?}", durable::test_counts());
    assert_eq!(durable::test_syncfs_count(), 1, "one syncfs for the batch");
    assert_eq!(durable::test_counts().1, 1, "one folder fsync: {:?}", durable::test_counts());
    let order = durable::test_order();
    let syncfs_at = order.iter().position(|s| s == "syncfs").expect("one syncfs in the order");
    let dir_at = order.iter().position(|s| s == "dir").expect("one folder fsync in the order");
    let first_remove = order.iter().position(|s| s == "remove").expect("removals are logged");
    let release_at = order.iter().position(|s| s == "release").expect("held files close");
    assert_eq!(order.iter().skip(release_at).filter(|s| *s == "release").count(), 64, "64 releases together: {:?}", order);
    assert!(syncfs_at < release_at && release_at < dir_at && dir_at < first_remove, "syncfs, releases, folder, first removal: {:?}", order);
    assert_eq!(durability.held_len(), 0, "nothing stays held past the confirm");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 64);
    for n in 0..64 {
        assert!(!srcdir.join(format!("f{n}.txt")).exists(), "no source survives its batch's syncfs");
        assert_eq!(std::fs::read_to_string(out.join(format!("f{n}.txt"))).unwrap(), "body");
    }
    durable::test_reset();
}

// A failed syncfs keeps every source and journals each landed copy as a partial.
#[test]
fn a_failed_syncfs_keeps_every_source_and_journals_partials() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-syncfs-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..4 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    durable::test_set_syncfs_errno(durable::EIO);
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    durable::test_set_syncfs_errno(0);
    assert_eq!((counts.ok, counts.failed), (0, 4));
    assert_eq!(retry.len(), 4, "every unconfirmed source is offered again");
    assert!(matches!(&steps[..], [Step::Copied { .. }, Step::Copied { .. }, Step::Copied { .. }, Step::Copied { .. }]), "partials, as a failed confirm journals");
    assert_eq!(durability.held_len(), 0, "a failed syncfs still drains the held set");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 4);
    for n in 0..4 {
        assert!(lines.iter().any(|(index, ok, err)| *index == n && !ok && err == durable::DIR_UNCONFIRMED));
        assert!(srcdir.join(format!("f{n}.txt")).exists(), "the source stays whole");
        assert!(out.join(format!("f{n}.txt")).exists(), "the landed copy stays beside it");
    }
    durable::test_reset();
}

// A cancelled batch_syncfs close still drains the held set.
#[test]
fn a_cancelled_batch_syncfs_close_leaves_nothing_held() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-syncfs-cancel");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, _rx) = channel();
    let mut steps = Vec::new();
    for n in 0..2 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    assert_eq!(durability.held_len(), 2, "two small files held");
    let (counts, _) = close_cancelled(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed), (2, 0));
    assert_eq!(durability.held_len(), 0, "a cancelled close drains the held set");
    durable::test_reset();
}

// A copy error mid-batch leaves the batch closable with nothing held.
#[test]
fn a_copy_error_mid_batch_still_closes_with_nothing_held() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-syncfs-copyerr");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, _rx) = channel();
    let mut steps = Vec::new();
    let first = srcdir.join("f0.txt");
    std::fs::write(&first, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &first, &out.join("f0.txt"));
    assert_eq!(durability.held_len(), 1, "one file held");
    // A destination already taken refuses the second copy before it holds anything.
    let second = srcdir.join("f1.txt");
    std::fs::write(&second, "body").unwrap();
    std::fs::write(out.join("f1.txt"), "taken").unwrap();
    let source = ItemIdentity::inspect(&second).expect("source recorded");
    match stage_copy(1, 1, "f1.txt", &second, &out.join("f1.txt"), source, &flag, &tx, &settled, &mut steps, &mut durability, &mut batch) {
        MoveOutcome::Done(Err(_)) => {}
        _ => panic!("a taken destination must refuse"),
    }
    let (counts, _) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!(counts.ok, 1, "the good item still moves");
    assert_eq!(durability.held_len(), 0, "the error never strands a held descriptor");
    assert!(!first.exists() && out.join("f0.txt").exists());
    assert!(second.exists(), "the refused source stays");
    durable::test_reset();
}

// A non-block durable batch keeps today's counts and never calls syncfs.
#[test]
fn a_non_block_durable_batch_keeps_per_file_fsyncs_and_no_syncfs() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-no-syncfs");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    assert!(!durability.batch_syncfs, "a testdir mount is not a block vfat stick");
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..4 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    let (counts, _) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed), (4, 0));
    assert_eq!(durable::test_counts(), (4, 1), "one file fsync per file, one folder: {:?}", durable::test_counts());
    assert_eq!(durable::test_syncfs_count(), 0, "no syncfs off a block stick");
    drop(tx);
    assert_eq!(items(rx).len(), 4);
    durable::test_reset();
}

// A syncfs failing at the 64th hold keeps all 64 sources, so unconfirmed bytes never lose a source.
#[test]
fn a_failed_cap_syncfs_keeps_all_sixty_four_sources_and_journals_partials() {
    use crate::backend::durable;
    durable::test_reset();
    let d = TestDir::new("movebatch-cap-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = AtomicBool::new(false);
    let settled = AtomicU64::new(0);
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..63 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, n, &src, &out.join(format!("f{n}.txt")));
    }
    // Only the cap settle fails; the 64th copy still lands, then every confirm must fail sticky.
    durable::test_set_syncfs_errno(durable::EIO);
    let src = srcdir.join("f63.txt");
    std::fs::write(&src, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 63, &src, &out.join("f63.txt"));
    durable::test_set_syncfs_errno(0);
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed), (0, 64), "every source stays when the cap syncfs failed");
    assert_eq!(retry.len(), 64, "every unconfirmed source is offered again");
    assert_eq!(steps.len(), 64, "each landed copy journals as a partial: {:?}", steps.len());
    assert!(steps.iter().all(|s| matches!(s, Step::Copied { .. })), "partials, never moves");
    drop(tx);
    let lines = items(rx);
    assert_eq!(lines.len(), 64);
    for n in 0..64 {
        assert!(srcdir.join(format!("f{n}.txt")).exists(), "no source is removed on an unconfirmed batch");
        assert!(out.join(format!("f{n}.txt")).exists(), "the landed copy stays beside it");
    }
    durable::test_reset();
}

// A staged copy that no longer shows every byte keeps its source and journals the copy, so undo can take it back.
#[test]
fn a_batch_copy_shown_short_keeps_the_source_and_journals_the_copy() {
    use crate::backend::{durable, undo::Journal};
    durable::test_reset();
    let d = TestDir::new("movebatch-short");
    let (srcdir, out) = (d.dir("src"), d.dir("out"));
    durable::test_mark_durable(&out);
    let mut durability = durable::Durability::begin(&out);
    let (flag, settled) = (AtomicBool::new(false), AtomicU64::new(0));
    let mut batch = MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    let src = srcdir.join("f0.txt");
    std::fs::write(&src, "body").unwrap();
    staged(1, &mut batch, &mut durability, &flag, &tx, &settled, &mut steps, 0, &src, &out.join("f0.txt"));
    std::fs::OpenOptions::new().write(true).open(out.join("f0.txt")).unwrap().set_len(2).unwrap();
    let (counts, retry) = close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    assert_eq!((counts.ok, counts.failed, retry.len()), (0, 1, 1));
    drop(tx);
    let lines = items(rx);
    assert_eq!((lines.len(), lines[0].1, lines[0].2.as_str()), (1, false, crate::backend::copyfile::landed::FILE_UNCONFIRMED));
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "body", "the source stays");
    assert!(matches!(&steps[..], [Step::Copied { .. }]), "the short copy journals a Copied step");
    let mut journal = Journal::new();
    journal.push(crate::backend::undo::Entry { op: "move".to_string(), steps });
    journal.undo().unwrap();
    assert!(!out.join("f0.txt").exists() && src.exists(), "undo takes the copy back and keeps the source");
    durable::test_reset();
}
