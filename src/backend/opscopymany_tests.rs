// What one paste of many small files costs: work counts per file, never wall time.
use crate::backend::collide::Policy;
use crate::backend::opsprobe;
use crate::backend::opsreq::{run_transfer_checked, OpMsg};
use crate::backend::testdir::TestDir;
use crate::backend::undo::{self, ItemIdentity};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;

const MANY: usize = 1000;
const FEW: usize = 10;
const FILE_BYTES: usize = 4096;

// Copies `files` small files into a fresh folder and returns the counts the whole transfer cost, with its item lines.
fn paste(files: usize) -> (opsprobe::Counts, usize) {
    let d = TestDir::new("copymany");
    let src = d.dir("src");
    let dst = d.dir("dst");
    let body = "x".repeat(FILE_BYTES);
    let paths: Vec<String> = (0..files).map(|n| d.file(&format!("src/f{:04}.bin", n), &body).to_string_lossy().into()).collect();
    assert!(src.is_dir());
    let watch = opsprobe::watch(d.path());
    let (tx, rx) = channel();
    run_transfer_checked(1, false, paths, dst.clone(), Arc::new(AtomicBool::new(false)), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: Policy::default() });
    let counts = watch.snapshot();
    let (mut ok, mut journaled) = (0, 0);
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: true, .. } => ok += 1,
            OpMsg::TransferDone { entry, .. } => journaled = entry.steps.len(),
            _ => {}
        }
    }
    assert_eq!(ok, files, "every file answers its own item line");
    assert_eq!(journaled, files, "every file is journaled for undo");
    assert_eq!(std::fs::read_dir(&dst).unwrap().count(), files, "every file landed");
    (counts, ok)
}

#[test]
fn a_paste_of_a_thousand_files_reads_the_mount_table_and_resolves_paths_no_more_often_than_a_paste_of_ten() {
    let (few, _) = paste(FEW);
    let (many, _) = paste(MANY);
    assert_eq!(many, few, "the work of a paste does not grow with its file count: {} files {:?}, {} files {:?}", MANY, many, FEW, few);
}

#[test]
fn a_paste_reads_the_mount_table_once_for_the_journal_and_resolves_the_source_folder_once() {
    let (counts, _) = paste(MANY);
    assert_eq!(counts.table_reads, 1, "one read of the mount table serves every journaled copy: {:?}", counts);
    assert_eq!(counts.resolves, 1, "one folder resolved serves every source in it: {:?}", counts);
}

#[test]
fn a_pasted_file_journals_the_step_the_plain_copy_constructor_builds() {
    let d = TestDir::new("copystep");
    d.dir("src");
    let src = d.file("src/one.bin", "x");
    let dst = d.dir("dst");
    let source = ItemIdentity::inspect(&src).unwrap();
    let (tx, rx) = channel();
    run_transfer_checked(1, false, vec![src.to_string_lossy().into()], dst.clone(), Arc::new(AtomicBool::new(false)), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: Policy::default() });
    let steps = rx.iter().find_map(|msg| if let OpMsg::TransferDone { entry, .. } = msg { Some(entry.steps) } else { None }).unwrap();
    let want = undo::copied(&src, &dst.join("one.bin"), source).unwrap();
    assert_eq!(steps, vec![want], "the journal step is the one undo::copied builds for that file");
}
