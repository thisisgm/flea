use super::*;
use crate::backend::copyfile::Progress;
use crate::backend::testdir::TestDir;

fn quiet<'a>(flag: &'a std::sync::atomic::AtomicBool, sink: &'a mut dyn FnMut(u64, u64), durability: &'a mut Durability) -> Progress<'a> {
    Progress { cancel: flag, on_bytes: sink, partial: None, tree: None, manifest: None, durability: Some(durability), for_move: false }
}

// Sample mountinfo text with dir as a /dev vfat stick, so begin batches it without two mounts.
fn vfat_body_for(dir: &std::path::Path) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / {} rw - vfat /dev/sda1 rw\n", dir.display())
}

// A Durability with a sticky unsettled and five held files, so a confirm must err drained.
fn sticky_with_five_held(name: &str) -> (TestDir, std::path::PathBuf, Durability) {
    test_reset();
    let d = TestDir::new(name);
    let srcdir = d.dir("src");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let first = d.file("first.txt", "body");
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&first, &out.join("first.txt"), &mut p).expect("copy");
    drop(p);
    test_set_syncfs_errno(EIO);
    assert!(durability.flush_dirs().is_err(), "the failed syncfs sets the sticky flag");
    test_set_syncfs_errno(0);
    for n in 0..5 {
        let src = srcdir.join(format!("h{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        let mut p = quiet(&flag, &mut sink, &mut durability);
        crate::backend::copyfile::copy_any(&src, &out.join(format!("h{n}.txt")), &mut p).expect("a copy still lands past a sticky failure");
        drop(p);
    }
    assert_eq!(durability.held_len(), 5, "five files held past a sticky failure");
    (d, out, durability)
}

// A sticky unsettled fails flush_dirs_for_many drained, so a batch keeps every source with no fd open.
#[test]
fn a_sticky_unsettled_fails_flush_dirs_for_many_drained() {
    let (_d, out, mut durability) = sticky_with_five_held("durable-sticky-many");
    assert!(durability.flush_dirs_for_many(&[out.join("h0.txt")]).is_err(), "a sticky failure fails every later confirm");
    assert_eq!(durability.held_len(), 0, "a failed confirm drains instead of leaving descriptors open");
    test_reset();
}

// A sticky unsettled fails flush_dirs_for drained, so a single-item move keeps its source with no fd open.
#[test]
fn a_sticky_unsettled_fails_flush_dirs_for_drained() {
    let (_d, out, mut durability) = sticky_with_five_held("durable-sticky-for");
    assert!(durability.flush_dirs_for(&out.join("h0.txt")).is_err(), "a sticky failure fails every later confirm");
    assert_eq!(durability.held_len(), 0, "a failed confirm drains instead of leaving descriptors open");
    test_reset();
}

// A sticky unsettled fails flush_dirs drained, so a duplicate or rename keeps everything with no fd open.
#[test]
fn a_sticky_unsettled_fails_flush_dirs_drained() {
    let (_d, _out, mut durability) = sticky_with_five_held("durable-sticky-flush");
    assert!(durability.flush_dirs().is_err(), "a sticky failure fails every later confirm");
    assert_eq!(durability.held_len(), 0, "a failed confirm drains instead of leaving descriptors open");
    test_reset();
}

// A sticky unsettled fails finish drained, so the done line reports unconfirmed with no fd open.
#[test]
fn a_sticky_unsettled_fails_finish_drained() {
    use std::sync::mpsc::channel;
    let (_d, out, mut durability) = sticky_with_five_held("durable-sticky-finish");
    let (tx, _rx) = channel();
    let done = finish(41, &tx, &mut durability, &out, 5);
    assert!(!done.ok, "an unconfirmed batch never claims the drive");
    assert_eq!(done.note, DIR_UNCONFIRMED, "the note names the unconfirmed folder: {:?}", done.note);
    assert_eq!(durability.held_len(), 0, "a failed confirm drains instead of leaving descriptors open");
    test_reset();
}

#[test]
fn a_marked_testdir_target_counts_as_usb_without_real_hardware() {
    test_reset();
    let d = TestDir::new("durable-class");
    let out = d.dir("out");
    test_mark_durable(&out);
    assert!(dest_is_durable(&out), "a test-marked target is usb");
    assert!(!dest_is_durable(d.path()), "its sibling stays local");
}

#[test]
fn fat_names_and_magics_count_as_durable() {
    assert!(fat_name_is_durable("vfat"));
    assert!(fat_name_is_durable("exfat"));
    assert!(fat_name_is_durable("ntfs"));
    assert!(fat_name_is_durable("VFAT"), "the mount table never promises a case");
    assert!(fat_name_is_durable("ntfs3"), "the in-kernel ntfs3 driver names itself");
    assert!(!fat_name_is_durable("ext4"));
    assert!(!fat_name_is_durable("btrfs"));
    assert!(fat_magic_is_durable(MSDOS_SUPER_MAGIC));
    assert!(fat_magic_is_durable(EXFAT_SUPER_MAGIC));
    assert!(fat_magic_is_durable(NTFS_SUPER_MAGIC));
    assert!(fat_magic_is_durable(NTFS3_SUPER_MAGIC));
    assert!(!fat_magic_is_durable(0xEF53));
}

#[test]
fn a_durable_copy_fsyncs_each_file_and_confirms_its_parent_once() {
    test_reset();
    let d = TestDir::new("durable-counts");
    let src = d.dir("src");
    std::fs::write(src.join("a.txt"), "a").unwrap();
    std::fs::write(src.join("b.txt"), "b").unwrap();
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src.join("a.txt"), &out.join("a.txt"), &mut p).expect("copy");
    crate::backend::copyfile::copy_any(&src.join("b.txt"), &out.join("b.txt"), &mut p).expect("copy");
    drop(p);
    assert_eq!(test_counts().0, 2, "one fsync per file, got {:?}", test_counts());
    durability.flush_dirs().expect("real dirs flush");
    assert_eq!(test_counts(), (2, 1), "one parent flush for two files, got {:?}", test_counts());
}

#[test]
fn touched_dirs_are_recorded_once_and_ordered_deepest_first() {
    test_reset();
    let d = TestDir::new("durable-order");
    test_mark_durable(d.path());
    let mut durability = Durability::begin(d.path());
    let (a, b) = (d.join("a"), d.join("a/b"));
    durability.touch(&a);
    durability.touch(&a);
    durability.touch(&b);
    durability.touch(&a);
    assert_eq!(durability.ordered(), vec![b, a]);
}

#[test]
fn a_local_copy_fsyncs_nothing() {
    test_reset();
    let d = TestDir::new("durable-local");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    let mut durability = Durability::begin(&out);
    assert!(!durability.durable, "an unmarked TestDir is local");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("a.txt"), &mut p).expect("copy");
    drop(p);
    assert_eq!(test_counts(), (0, 0), "no flush on a local target");
}

#[test]
fn a_failed_fsync_fails_the_copy_like_any_other_write_error() {
    test_reset();
    let d = TestDir::new("durable-fail");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    assert!(durability.durable);
    test_set_fail(true);
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    let err = crate::backend::copyfile::copy_any(&src, &out.join("a.txt"), &mut p).expect_err("a failed fsync is a failed copy");
    assert_eq!(err.where_, "copy");
    assert!(p.partial.is_none(), "the unconfirmed file went with its hidden name, so nothing is journalled: {:?}", p.partial);
    assert_eq!(test_counts().0, 1, "the failed flush still counts as called");
}

#[test]
fn the_writing_line_carries_the_phase_the_protocol_documents() {
    let line = writing_line(12, "128GB");
    assert!(line.contains(r#""t":"transferprogress""#));
    assert!(line.contains(r#""phase":"writing""#), "the final phase rides a progress line: {}", line);
}

#[test]
fn the_writing_line_names_the_drive_it_is_flushing() {
    let line = writing_line(12, "128GB");
    assert!(line.contains(r#""drive":"128GB""#), "the card names the drive: {}", line);
    let quoted = writing_line(12, "128\"GB");
    assert!(quoted.contains(r#""drive":"128\"GB""#), "the drive is JSON-escaped: {}", quoted);
}

#[test]
fn finish_names_the_destination_it_flushes() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-finish-drive");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    assert!(durability.durable);
    let (tx, rx) = channel();
    finish(7, &tx, &mut durability, &out, 1);
    let mut drive = None;
    for msg in rx.try_iter() {
        if let crate::backend::opsreq::OpMsg::Meta { line } = msg {
            if line.contains(r#""phase":"writing""#) {
                drive = Some(line);
            }
        }
    }
    let line = drive.expect("the final phase emits one writing line");
    let want = drive_name(&out);
    assert!(line.contains(&format!(r#""drive":"{}""#, want)), "the drive is the mount's own name: {}", line);
}

#[test]
fn drive_name_answers_the_mount_not_the_folder() {
    let body = "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw\n";
    assert_eq!(drive_name_in(std::path::Path::new("/media/stick/DCIM"), body), "stick");
    assert_eq!(drive_name_in(std::path::Path::new("/media/stick/photos"), body), "stick");
}

#[test]
fn drive_name_answers_the_share_for_a_gvfs_path() {
    let body = "";
    assert_eq!(drive_name_in(std::path::Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=media/photos"), body), "media");
    assert_eq!(drive_name_in(std::path::Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=media"), body), "media");
}

#[test]
fn a_durable_transfer_reports_writing_and_durable_true() {
    use crate::backend::opsreq::{run_transfer, OpMsg};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    test_reset();
    let d = TestDir::new("durable-wire");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    let (tx, rx) = channel();
    run_transfer(7, false, vec![src.to_string_lossy().to_string()], out.clone(), Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    let mut saw_writing = false;
    let mut durable = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Meta { line } if line.contains(r#""phase":"writing""#) => saw_writing = true,
            OpMsg::TransferDone { durable: done_durable, ok, failed, .. } => {
                durable = Some((done_durable, ok, failed));
            }
            _ => {}
        }
    }
    assert!(saw_writing, "the final phase emits one writing line");
    assert_eq!(durable, Some((true, 1, 0)), "every flush succeeded: {:?}", durable);
    let (files, dirs) = test_counts();
    assert_eq!(files, 1, "one file fsync");
    assert!(dirs >= 1, "at least the dest dir, got {:?}", test_counts());
}

#[test]
fn a_local_transfer_reports_durable_false_and_no_writing() {
    use crate::backend::opsreq::{run_transfer, OpMsg};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    test_reset();
    let d = TestDir::new("durable-wire-local");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    let (tx, rx) = channel();
    run_transfer(8, false, vec![src.to_string_lossy().to_string()], out.clone(), Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    let mut saw_writing = false;
    let mut durable = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Meta { line } if line.contains("writing") => saw_writing = true,
            OpMsg::TransferDone { durable: done_durable, .. } => durable = Some(done_durable),
            _ => {}
        }
    }
    assert!(!saw_writing, "no final phase on a local target");
    assert_eq!(durable, Some(false));
    assert_eq!(test_counts(), (0, 0), "no flush on a local target");
}

#[test]
fn a_failed_file_fsync_fails_the_item_and_leaves_nothing_to_journal() {
    use crate::backend::opsreq::{run_transfer, OpMsg};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    test_reset();
    let d = TestDir::new("durable-wire-fail");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    test_set_fail_files(true);
    let (tx, rx) = channel();
    run_transfer(9, false, vec![src.to_string_lossy().to_string()], out.clone(), Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    let mut item_err = String::new();
    let mut entry_steps = 0;
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: false, err, .. } => item_err = err,
            OpMsg::TransferDone { entry, durable: done_durable, ok, failed, note, .. } => {
                entry_steps = entry.steps.len();
                done = Some((done_durable, ok, failed, note));
            }
            _ => {}
        }
    }
    assert!(!item_err.is_empty(), "the file's name rides a copy error");
    assert_eq!(entry_steps, 0, "the unconfirmed file went with its hidden name, so nothing is journalled");
    assert_eq!(done, Some((false, 0, 1, String::new())), "a failed file flush is not durable with no note: {:?}", done);
    test_set_fail_files(false);
}

#[test]
fn a_file_fsync_failure_beside_a_landed_file_is_not_durable() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-wire-filefail");
    let first = d.file("a.txt", "body");
    let second = d.file("b.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    assert!(durability.durable);
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&first, &out.join("a.txt"), &mut p).expect("first file lands");
    drop(p);
    test_set_fail_files(true);
    let mut p = quiet(&flag, &mut sink, &mut durability);
    let err = crate::backend::copyfile::copy_any(&second, &out.join("b.txt"), &mut p).expect_err("a failed file fsync is a failed copy");
    assert_eq!(err.where_, "copy");
    drop(p);
    test_set_fail_files(false);
    let (tx, rx) = channel();
    let done = finish(16, &tx, &mut durability, &out, 1);
    assert!(!done.ok, "a failed file flush is not durable");
    assert!(done.note.is_empty(), "a failed file flush carries no note: {:?}", done.note);
    assert!(rx.try_iter().any(|m| matches!(m, crate::backend::opsreq::OpMsg::Meta { line } if line.contains(r#""phase":"writing""#))), "the writing line still runs beside a landed file");
}

#[test]
fn a_directory_flush_failure_keeps_durable_false_and_says_so() {
    use crate::backend::opsreq::{run_transfer, OpMsg};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    test_reset();
    let d = TestDir::new("durable-dirfail");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    test_set_fail_dirs(true);
    let (tx, rx) = channel();
    run_transfer(10, false, vec![src.to_string_lossy().to_string()], out.clone(), Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    let mut saw_writing = false;
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Meta { line } if line.contains(r#""phase":"writing""#) => saw_writing = true,
            OpMsg::TransferDone { durable: done_durable, ok, failed, note, .. } => {
                done = Some((done_durable, ok, failed, note));
            }
            _ => {}
        }
    }
    assert!(saw_writing, "the writing phase still runs");
    assert_eq!(done, Some((false, 1, 0, DIR_UNCONFIRMED.to_string())), "the file landed but the folder is unconfirmed: {:?}", done);
    test_set_fail_dirs(false);
}

#[test]
fn duplicate_on_a_durable_target_confirms_the_new_file() {
    test_reset();
    let d = TestDir::new("durable-dup");
    let src = d.file("a.txt", "body");
    test_mark_durable(d.path());
    let (outcome, _steps) = crate::backend::ops::duplicate(&src);
    assert!(outcome.is_ok());
    assert_eq!(test_counts().0, 1, "duplicate fsyncs its file on a durable target");
}

#[test]
fn rename_on_a_durable_target_confirms_the_directory() {
    test_reset();
    let d = TestDir::new("durable-rename");
    let src = d.file("a.txt", "body");
    test_mark_durable(d.path());
    let (to, _steps) = crate::backend::ops::rename(&src, "b.txt").expect("rename");
    assert!(to.exists());
    assert!(test_counts().1 >= 1, "rename fsyncs its directory on a durable target");
}

#[test]
fn redo_of_a_copy_confirms_the_file_again() {
    use crate::backend::undo::{Entry, Journal};
    test_reset();
    let d = TestDir::new("durable-redo");
    let src = d.file("a.txt", "body");
    test_mark_durable(d.path());
    let (outcome, steps) = crate::backend::ops::duplicate(&src);
    let copy = outcome.expect("duplicate");
    let mut journal = Journal::new();
    journal.push(Entry { op: "copy".into(), steps });
    journal.undo().expect("undo removes the copy");
    assert!(!copy.exists());
    test_reset_counts();
    let (tx, _rx) = std::sync::mpsc::channel();
    journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx).expect("redo");
    assert!(copy.exists(), "redo put the copy back");
    assert_eq!(test_counts().0, 1, "redo fsyncs its file on a durable target: {:?}", test_counts());
}

#[test]
fn an_rclone_fstype_counts_and_nothing_else_does() {
    assert!(fstype_is_rclone("fuse.rclone"));
    assert!(fstype_is_rclone("FUSE.RCLONE"), "the mount table never promises a case");
    assert!(!fstype_is_rclone("fuse.sshfs"));
    assert!(!fstype_is_rclone("fuse.gvfsd-fuse"));
    assert!(!fstype_is_rclone("ext4"));
    assert!(!fstype_is_rclone(""));
}

#[test]
fn a_copy_onto_rclone_says_it_uploads_in_the_background_and_never_claims_the_drive() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-rclone-note");
    let out = d.dir("out");
    let mut durability = Durability { durable: false, rclone: true, file_failed: false, batch_syncfs: false, rename_fat: false, unsettled: false,
        closes_failed: false, held: Vec::new(), wave: Wave::default(), touched: std::collections::HashSet::new(), last: None };
    let (tx, rx) = channel();
    let done = finish(9, &tx, &mut durability, &out, 1);
    assert!(!done.ok, "an rclone copy never claims the drive confirmed it");
    assert_eq!(done.note, RCLONE_NOTE, "the verdict names the background upload: {:?}", done.note);
    assert!(rx.try_iter().next().is_none(), "no writing phase on an rclone target");
}

#[test]
fn a_move_confirms_only_the_folders_its_destination_filled() {
    test_reset();
    let d = TestDir::new("flushfor");
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::create_dir_all(d.join("dst/tree")).unwrap();
    test_mark_durable(d.path());
    let mut durability = Durability::begin(&d.join("dst/tree"));
    durability.touch(&d.join("src"));
    durability.touch(&d.join("dst/tree"));
    durability.touch(&d.join("dst"));
    test_reset_counts();
    durability.flush_dirs_for(&d.join("dst/tree")).unwrap();
    assert_eq!(test_counts().1, 2, "the tree and its parent, never the source's folder");
    test_reset();
}

#[test]
fn finish_with_nothing_landed_sends_no_writing_line_and_claims_nothing() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-finish-empty");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    let (tx, rx) = channel();
    let done = finish(11, &tx, &mut durability, &out, 0);
    assert!(!done.ok, "nothing landed, so nothing is durable");
    assert!(done.note.is_empty(), "nothing landed, so no note: {:?}", done.note);
    assert!(rx.try_iter().next().is_none(), "nothing landed, so no writing line");
}

#[test]
fn finish_with_nothing_landed_on_rclone_says_nothing() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-rclone-empty");
    let out = d.dir("out");
    let mut durability = Durability { durable: false, rclone: true, file_failed: false, batch_syncfs: false, rename_fat: false, unsettled: false,
        closes_failed: false, held: Vec::new(), wave: Wave::default(), touched: std::collections::HashSet::new(), last: None };
    let (tx, rx) = channel();
    let done = finish(12, &tx, &mut durability, &out, 0);
    assert!(!done.ok, "nothing landed, so nothing is durable");
    assert!(done.note.is_empty(), "nothing landed, so no rclone note: {:?}", done.note);
    assert!(rx.try_iter().next().is_none(), "nothing landed, so no writing line");
}

#[test]
fn a_failed_batch_lands_nothing_and_sends_no_writing_line() {
    use crate::backend::opsreq::{run_transfer, OpMsg};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    test_reset();
    let d = TestDir::new("durable-wire-failall");
    let missing = d.join("gone.txt");
    let out = d.dir("out");
    test_mark_durable(&out);
    let (tx, rx) = channel();
    run_transfer(14, false, vec![missing.to_string_lossy().to_string()], out.clone(), Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    let mut saw_writing = false;
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Meta { line } if line.contains(r#""phase":"writing""#) => saw_writing = true,
            OpMsg::TransferDone { durable: done_durable, ok, failed, note, .. } => {
                done = Some((done_durable, ok, failed, note));
            }
            _ => {}
        }
    }
    assert!(!saw_writing, "nothing landed, so no writing line");
    assert_eq!(done, Some((false, 0, 1, String::new())), "a failed batch claims nothing: {:?}", done);
}

#[test]
fn a_cancelled_batch_lands_nothing_and_sends_no_writing_line() {
    use crate::backend::opsreq::{run_transfer, OpMsg};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    test_reset();
    let d = TestDir::new("durable-wire-skipall");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    let (tx, rx) = channel();
    run_transfer(15, false, vec![src.to_string_lossy().to_string()], out.clone(), Arc::new(AtomicBool::new(true)), tx);
    let mut saw_writing = false;
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Meta { line } if line.contains(r#""phase":"writing""#) => saw_writing = true,
            OpMsg::TransferDone { durable: done_durable, ok, failed, skipped, note, .. } => {
                done = Some((done_durable, ok, failed, skipped, note));
            }
            _ => {}
        }
    }
    assert!(!saw_writing, "nothing landed, so no writing line");
    assert_eq!(done, Some((false, 0, 0, 1, String::new())), "a skipped batch claims nothing: {:?}", done);
}

#[test]
fn a_cancelled_folder_copy_forgets_the_tree_it_removed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-cancel-forget");
    let first = d.file("first.txt", "body");
    let src = d.dir("src");
    std::fs::write(src.join("a.txt"), "a").unwrap();
    std::fs::create_dir(src.join("sub")).unwrap();
    std::fs::write(src.join("sub/b.txt"), "b").unwrap();
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    let idle = AtomicBool::new(false);
    let mut quiet_sink = |_: u64, _: u64| {};
    let mut p = quiet(&idle, &mut quiet_sink, &mut durability);
    crate::backend::copyfile::copy_any(&first, &out.join("first.txt"), &mut p).expect("one file lands first");
    drop(p);
    let dst = out.join("clone");
    d.assert_contains(&dst);
    // The first bytes of the tree raise the cancel, so the folder exists and was touched before it goes.
    let flag = AtomicBool::new(false);
    let mut cancelling = |_: u64, _: u64| flag.store(true, Ordering::Relaxed);
    let mut p = quiet(&flag, &mut cancelling, &mut durability);
    let err = crate::backend::copyfile::copy_any(&src, &dst, &mut p).expect_err("cancelled mid-tree");
    drop(p);
    assert_eq!(err.msg, "cancelled");
    assert!(!dst.exists(), "the cancelled tree goes with the cancel");
    let (tx, _rx) = channel();
    let done = finish(13, &tx, &mut durability, &out, 1);
    assert!(done.note.is_empty(), "a removed tree is not a confirmation failure: {:?}", done.note);
    assert!(done.ok, "the file that landed is confirmed");
}

#[test]
fn duplicate_classifies_the_existing_parent_not_the_missing_name() {
    test_reset();
    let d = TestDir::new("durable-dup-parent");
    let src = d.file("a.txt", "body");
    // Only the missing name is marked, so probing it answers durable and probing its parent does not.
    test_mark_durable(&d.join("a copy.txt"));
    test_set_fail_dirs(true);
    let (outcome, _) = crate::backend::ops::duplicate(&src);
    test_set_fail_dirs(false);
    test_reset();
    assert_eq!(outcome.map_err(|e| e.msg), Ok(d.join("a copy.txt")), "the marked name is the copy's, and the parent was classified, so no folder flush ran to fail");
}

// A durable copy confirms slices while it writes, so the card counts confirmed bytes mid-file.
#[test]
fn a_durable_copy_confirms_slices_while_it_writes() {
    test_reset();
    let d = TestDir::new("durable-slices");
    let out = d.dir("out");
    test_mark_durable(&out);
    // Ramp slices 1, 2, 4, 8, 8 MiB plus a tail: pipelined writeback reports through k-1, so four mids plus the final.
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as usize;
    let total: usize = confirm * 3 + 1024 * 1024;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut sink = |done: u64, against: u64| seen.push((done, against));
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("copy");
    drop(p);
    let mib = 1024 * 1024 as u64;
    assert_eq!(seen.len(), 5, "four ramped mids plus the final, got {:?}", seen);
    assert_eq!((seen[0].0, seen[1].0, seen[2].0, seen[3].0), (mib, mib * 3, mib * 7, mib * 15), "the ramped mids, got {:?}", seen);
    assert!(seen.windows(2).all(|w| w[1].0 > w[0].0), "confirmed counts only ever rise, got {:?}", seen);
    assert_eq!(seen.last().copied(), Some((total as u64, total as u64)), "the last report is the whole file, got {:?}", seen.last());
    test_reset();
}

#[test]
fn a_filesystem_without_range_writeback_is_not_a_failed_copy() {
    // EINVAL is what some FUSE mounts answer sync_file_range with; EIO is a drive refusing bytes.
    let einval = std::io::Error::from_raw_os_error(crate::backend::durable::EINVAL);
    assert_eq!(einval.kind(), std::io::ErrorKind::InvalidInput, "EINVAL must be the kernel's invalid-argument errno");
    let eio = std::io::Error::from_raw_os_error(crate::backend::durable::EIO);
    assert!(crate::backend::durable::range_unsupported(&einval));
    assert!(!crate::backend::durable::range_unsupported(&eio));
}

// A write seam answering EINVAL falls back to the final fsync: the copy still lands whole.
#[test]
fn a_range_einval_still_copies_and_reports_only_the_final_count() {
    test_reset();
    let d = TestDir::new("durable-range-einval");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as usize;
    let total: usize = confirm * 2 + 1024 * 1024;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    crate::backend::durable::test_set_fail_range_write(true);
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut sink = |done: u64, against: u64| seen.push((done, against));
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("a range EINVAL is a fallback, not a failure");
    drop(p);
    crate::backend::durable::test_set_fail_range_write(false);
    assert_eq!(seen, vec![(total as u64, total as u64)], "only the final whole-file count, got {:?}", seen);
    assert_eq!(crate::backend::durable::test_range_waits(), 0, "no slice wait ran");
    assert_eq!(test_counts().0, 1, "the final fsync still ran, got {:?}", test_counts());
    assert_eq!(std::fs::metadata(out.join("big.bin")).unwrap().len(), total as u64);
    test_reset();
}

// A slice flush failure fails the copy, marks the file and leaves no half behind.
#[test]
fn a_slice_flush_failure_fails_the_copy_and_leaves_no_half() {
    test_reset();
    let d = TestDir::new("durable-slice-fail");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as usize;
    let total: usize = confirm * 2;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    crate::backend::durable::test_set_fail_files(true);
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    let err = crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect_err("a failed slice flush is a failed copy");
    assert_eq!(err.where_, "copy");
    assert!(p.partial.is_none(), "the half went with its hidden name, so nothing is journalled: {:?}", p.partial);
    drop(p);
    crate::backend::durable::test_set_fail_files(false);
    assert!(durability.file_failed, "the file failure is noted for the verdict");
    assert_eq!(test_counts().0, 1, "the failed slice never reaches the final fsync, got {:?}", test_counts());
    test_reset();
}

// Every mid-file report follows a completed wait, so the card counts confirmed bytes only.
#[test]
fn every_mid_file_report_follows_a_completed_wait() {
    test_reset();
    let d = TestDir::new("durable-reports-waits");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as u64;
    let total: usize = confirm as usize * 3 + 1024 * 1024;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    let flag = std::sync::atomic::AtomicBool::new(false);
    // The waits seen at each report, so a report moved above its wait reddens.
    let mut seen: Vec<(u64, u64, usize)> = Vec::new();
    let mut sink = |done: u64, against: u64| seen.push((done, against, crate::backend::durable::test_range_waits()));
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("copy");
    drop(p);
    let waits = crate::backend::durable::test_range_waits();
    let mib = 1024 * 1024 as u64;
    assert_eq!(waits, 4, "four pipelined waits for five ramp slices, got {}", waits);
    assert_eq!(seen.len(), waits + 1, "mids plus the final, got {:?}", seen);
    assert_eq!((seen[0].0, seen[0].2), (mib, 1), "the first mid follows one wait, got {:?}", seen);
    assert_eq!((seen[1].0, seen[1].2), (mib * 3, 2), "the second mid follows two waits, got {:?}", seen);
    assert_eq!((seen[2].0, seen[2].2), (mib * 7, 3), "the third mid follows three waits, got {:?}", seen);
    assert_eq!((seen[3].0, seen[3].2), (mib * 15, 4), "the fourth mid follows four waits, got {:?}", seen);
    assert_eq!(seen[4].2, 4, "the final fsync adds no wait, got {:?}", seen);
    assert_eq!(seen.last().map(|s| (s.0, s.1)), Some((total as u64, total as u64)));
    test_reset();
}

// The wait leg answering EINVAL stops slicing and reports only the final count.
#[test]
fn a_wait_einval_stops_slicing_and_reports_only_the_final_count() {
    test_reset();
    let d = TestDir::new("durable-wait-einval");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as usize;
    // Three slices, so a range call after the failing wait would show in the log.
    let total: usize = confirm * 3 + 1024 * 1024;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    crate::backend::durable::test_set_fail_range_wait(crate::backend::durable::EINVAL);
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut sink = |done: u64, against: u64| seen.push((done, against));
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("a wait EINVAL is a fallback, not a failure");
    drop(p);
    crate::backend::durable::test_set_fail_range_wait(0);
    assert_eq!(seen, vec![(total as u64, total as u64)], "only the final whole-file count, got {:?}", seen);
    assert_eq!(crate::backend::durable::test_range_waits(), 0, "no wait completed");
    assert_eq!(test_counts().0, 3, "two writes plus the final fsync, got {:?}", test_counts());
    let mib = 1024 * 1024;
    assert_eq!(crate::backend::durable::test_range_log(), vec![
        "write 0".to_string(),
        format!("write {mib}"),
        "wait 0".to_string(),
        "fsync".to_string(),
    ], "slicing stops at the failing wait, got {:?}", crate::backend::durable::test_range_log());
    assert_eq!(std::fs::metadata(out.join("big.bin")).unwrap().len(), total as u64);
    test_reset();
}

// The wait leg answering EIO fails the copy, notes the file and journals the partial.
#[test]
fn a_wait_eio_fails_the_copy_and_leaves_no_half() {
    test_reset();
    let d = TestDir::new("durable-wait-eio");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as usize;
    let total: usize = confirm * 2;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    crate::backend::durable::test_set_fail_range_wait(crate::backend::durable::EIO);
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    let err = crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect_err("a failed wait is a failed copy");
    assert_eq!(err.where_, "copy");
    assert!(p.partial.is_none(), "the half went with its hidden name, so nothing is journalled: {:?}", p.partial);
    drop(p);
    crate::backend::durable::test_set_fail_range_wait(0);
    assert!(durability.file_failed, "the file failure is noted for the verdict");
    assert_eq!(crate::backend::durable::test_range_waits(), 0, "no wait completed");
    test_reset();
}

// Slice k starts with WRITE alone before slice k-1 waits, so the next slice writes while it flies.
#[test]
fn slices_start_with_write_alone_before_the_previous_waits() {
    test_reset();
    let d = TestDir::new("durable-pipeline-order");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES;
    let total: usize = confirm as usize * 3 + 1024 * 1024;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("copy");
    drop(p);
    let mib: u64 = 1024 * 1024;
    assert_eq!(crate::backend::durable::test_range_log(), vec![
        format!("write 0"),
        format!("write {mib}"),
        "wait 0".to_string(),
        format!("write {}", mib * 3),
        format!("wait {mib}"),
        format!("write {}", mib * 7),
        format!("wait {}", mib * 3),
        format!("write {}", mib * 15),
        format!("wait {}", mib * 7),
        "fsync".to_string(),
    ], "pipelined writes before waits, got {:?}", crate::backend::durable::test_range_log());
    test_reset();
}

// The first progress lands after the first ramp slice, so a large copy reports in milliseconds.
#[test]
fn first_confirmed_slice_is_first_confirm_bytes() {
    test_reset();
    let d = TestDir::new("durable-ramp-first");
    let out = d.dir("out");
    test_mark_durable(&out);
    let confirm = crate::backend::copyfile::CONFIRM_BYTES as usize;
    let total: usize = confirm * 3 + 1024 * 1024;
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; total]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    assert!(durability.durable, "the marked target is durable");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut sink = |done: u64, against: u64| seen.push((done, against));
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("copy");
    drop(p);
    assert_eq!(seen[0].0, crate::backend::copyfile::FIRST_CONFIRM_BYTES, "the first report is the first ramp slice, got {:?}", seen);
    test_reset();
}

// Sample mountinfo bodies: a /dev block vfat stick batches, everything else keeps per-file fsync.
#[test]
fn begin_classifies_batch_syncfs_off_sample_mountinfo_lines() {
    let stick = "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / /media/stick rw - vfat /dev/sda1 rw\n";
    let dest = std::path::Path::new("/media/stick/photos");
    assert!(super::batch_syncfs_for(dest, stick, true, false), "a /dev vfat stick batches");
    for (fstype, source) in [("fuse.rclone", "remote:"), ("fuse.gvfsd-fuse", "gvfsd-fuse"),
        ("nfs4", "nas:/share"), ("cifs", "//nas/media")] {
        let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / /media/x rw - {fstype} {source} rw\n");
        let dest = std::path::Path::new("/media/x/photos");
        assert!(!super::batch_syncfs_for(dest, &body, true, false), "{fstype} never batches");
    }
    assert!(!super::batch_syncfs_for(dest, stick, false, false), "a local target never batches");
    assert!(!super::batch_syncfs_for(dest, stick, true, true), "rclone never batches");
}

// A 1 MiB file keeps its range slices and its final fsync, and its descriptor stays held.
#[test]
fn a_megabyte_file_on_a_batch_target_keeps_slices_and_is_held() {
    test_reset();
    let d = TestDir::new("durable-batch-large");
    let out = d.dir("out");
    test_mark_durable(&out);
    let src = d.join("big.bin");
    std::fs::write(&src, vec![b'a'; 1024 * 1024]).expect("test sandbox file");
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("big.bin"), &mut p).expect("copy");
    drop(p);
    assert!(test_range_log().iter().any(|s| s.starts_with("write")), "the range slices ran: {:?}", test_range_log());
    assert!(test_range_log().contains(&"fsync".to_string()), "the final fsync ran: {:?}", test_range_log());
    assert_eq!(durability.held_len(), 1, "the descriptor stays held for the batch syncfs");
    durability.release_held();
    assert_eq!(durability.held_len(), 0, "a release drains the held set");
    test_reset();
}

// A finish on a batch target leaves nothing held, so an eject right after never waits on one.
#[test]
fn finish_on_a_batch_target_leaves_nothing_held() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-batch-finish");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("a.txt"), &mut p).expect("copy");
    drop(p);
    assert_eq!(durability.held_len(), 1, "one small file held");
    let (tx, _rx) = channel();
    let done = finish(21, &tx, &mut durability, &out, 1);
    assert!(done.ok, "a batch finish confirms: {:?}", done.note);
    assert_eq!(durability.held_len(), 0, "no descriptor stays open at transferdone");
    assert_eq!(test_syncfs_count(), 1, "one syncfs for the batch: {:?}", test_order());
    test_reset();
}

// The 64th hold settles with its syncfs, so a copy never runs more than one batch ahead of the drive.
#[test]
fn hold_at_cap_settles_with_one_syncfs() {
    test_reset();
    let d = TestDir::new("durable-hold-cap");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    for n in 0..64 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        let mut p = quiet(&flag, &mut sink, &mut durability);
        crate::backend::copyfile::copy_any(&src, &out.join(format!("f{n}.txt")), &mut p).expect("copy");
        drop(p);
    }
    assert_eq!(durability.held_len(), 0, "the 64th hold released the batch");
    assert_eq!(test_releases(), 64, "64 closes on scoped threads: {:?}", test_order());
    assert_eq!(test_syncfs_count(), 1, "one syncfs at the cap, so the rate stays the drive's: {:?}", test_order());
    test_reset();
}

// A confirm settles held files before its folder fsync, so no caller removes a source early.
#[test]
fn flush_dirs_settles_held_before_its_folder_fsync() {
    test_reset();
    let d = TestDir::new("durable-flush-settle");
    let src = d.file("a.txt", "body");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("a.txt"), &mut p).expect("copy");
    drop(p);
    assert_eq!(durability.held_len(), 1, "one file held");
    durability.flush_dirs().expect("confirm");
    let order = test_order();
    let syncfs_at = order.iter().position(|s| s == "syncfs").expect("one syncfs in the order");
    let dir_at = order.iter().position(|s| s == "dir").expect("one folder fsync in the order");
    let release_at = order.iter().position(|s| s == "release").expect("held file closes");
    assert!(syncfs_at < release_at && release_at < dir_at, "syncfs, release, folder fsync: {:?}", order);
    assert_eq!(durability.held_len(), 0, "nothing stays held past the confirm");
    test_reset();
}

// A failed syncfs counts nothing and logs nothing, so it never reads as confirmed.
#[test]
fn a_failed_syncfs_counts_nothing_and_logs_nothing() {
    test_reset();
    let d = TestDir::new("durable-syncfs-fail-count");
    let out = d.dir("out");
    test_set_syncfs_errno(EIO);
    let err = super::syncfs_dir(&out).expect_err("a failed syncfs is a failed confirm");
    assert_eq!(test_syncfs_count(), 0, "a failed syncfs never counts");
    assert!(!test_order().contains(&"syncfs".to_string()), "a failed syncfs never logs: {:?}", test_order());
    test_set_syncfs_errno(0);
    super::syncfs_dir(&out).expect("a good syncfs confirms");
    assert_eq!(test_syncfs_count(), 1, "one syncfs after the failure: {:?}", err);
    test_reset();
}

// copy_then_remove on a forced batch target settles before removing its source.
#[test]
fn copy_then_remove_on_a_batch_target_settles_before_removing_its_source() {
    test_reset();
    let d = TestDir::new("durable-rename-batch");
    let from = d.file("source.txt", "body");
    let to = d.join("target.txt");
    test_mark_durable(d.path());
    super::test_set_fake_mountinfo(Some(&vfat_body_for(d.path())));
    crate::backend::renamecompat::copy_then_remove(&from, &to).expect("rename by exclusive copy");
    super::test_set_fake_mountinfo(None);
    let order = test_order();
    let syncfs_at = order.iter().position(|s| s == "syncfs").expect("one syncfs in the order");
    let dir_at = order.iter().position(|s| s == "dir").expect("one folder fsync in the order");
    let release_at = order.iter().position(|s| s == "release").expect("held file closes");
    assert!(syncfs_at < release_at && release_at < dir_at, "syncfs, release, folder fsync, then source removal: {:?}", order);
    assert!(!from.exists(), "the source goes only after the confirm");
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "body");
    test_reset();
}

// A failing syncfs keeps the source of a batch rename, so a crash never loses the file.
#[test]
fn copy_then_remove_on_a_batch_target_keeps_its_source_when_syncfs_fails() {
    test_reset();
    let d = TestDir::new("durable-rename-batch-fail");
    let from = d.file("source.txt", "body");
    let to = d.join("target.txt");
    test_mark_durable(d.path());
    super::test_set_fake_mountinfo(Some(&vfat_body_for(d.path())));
    test_set_syncfs_errno(EIO);
    let err = crate::backend::renamecompat::copy_then_remove(&from, &to).expect_err("an unconfirmed rename keeps its source");
    test_set_syncfs_errno(0);
    super::test_set_fake_mountinfo(None);
    assert!(from.exists(), "the source stays: {:?}", err.msg);
    assert!(!to.exists(), "the unconfirmed copy goes back");
    assert_eq!(test_syncfs_count(), 0, "a failed syncfs never counts: {:?}", test_order());
    test_reset();
}

// Redo of a copy on a forced batch target settles before it confirms.
#[test]
fn redo_of_a_copy_on_a_batch_target_settles_before_it_confirms() {
    use crate::backend::undo::{Entry, Journal};
    test_reset();
    let d = TestDir::new("durable-redo-batch");
    let src = d.file("a.txt", "body");
    test_mark_durable(d.path());
    let (outcome, steps) = crate::backend::ops::duplicate(&src);
    let copy = outcome.expect("duplicate");
    let mut journal = Journal::new();
    journal.push(Entry { op: "copy".into(), steps });
    journal.undo().expect("undo removes the copy");
    assert!(!copy.exists());
    test_reset_counts();
    super::test_set_fake_mountinfo(Some(&vfat_body_for(d.path())));
    let (tx, _rx) = std::sync::mpsc::channel();
    journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx).expect("redo");
    super::test_set_fake_mountinfo(None);
    let order = test_order();
    let syncfs_at = order.iter().position(|s| s == "syncfs").expect("one syncfs in the order");
    let dir_at = order.iter().position(|s| s == "dir").expect("one folder fsync in the order");
    assert!(syncfs_at < dir_at, "syncfs, folder fsync: {:?}", order);
    assert!(copy.exists(), "redo put the copy back");
    test_reset();
}

// A cap settle that failed makes finish report unconfirmed, not a silent non-durable done.
#[test]
fn finish_after_a_failed_cap_settle_reports_unconfirmed() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-finish-cap-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    for n in 0..63 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        let mut p = quiet(&flag, &mut sink, &mut durability);
        crate::backend::copyfile::copy_any(&src, &out.join(format!("f{n}.txt")), &mut p).expect("copy");
        drop(p);
    }
    test_set_syncfs_errno(EIO);
    let src = srcdir.join("f63.txt");
    std::fs::write(&src, "body").unwrap();
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("f63.txt"), &mut p).expect("the 64th copy still lands");
    drop(p);
    test_set_syncfs_errno(0);
    let (tx, _rx) = channel();
    let done = finish(31, &tx, &mut durability, &out, 64);
    assert!(!done.ok, "an unconfirmed batch never claims the drive");
    assert_eq!(done.note, DIR_UNCONFIRMED, "the note names the unconfirmed folder: {:?}", done.note);
    test_reset();
}

// begin_from reads the mountinfo text it is handed, so the call below pins begin's argument order.
#[test]
fn begin_from_pins_the_mountinfo_argument_order() {
    test_reset();
    let d = TestDir::new("durable-begin-order");
    test_mark_durable(d.path());
    let dest = d.join("photos");
    let stick = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / {} rw - vfat /dev/sda1 rw\n", d.path().display());
    assert!(super::Durability::begin_from(&dest, Some(&stick)).batch_syncfs, "a /dev vfat stick batches");
    for (fstype, source) in [("fuse.rclone", "remote:"), ("fuse.gvfsd-fuse", "gvfsd-fuse"),
        ("nfs4", "nas:/share"), ("cifs", "//nas/media")] {
        let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - {fstype} {source} rw\n", d.path().display());
        assert!(!super::Durability::begin_from(&dest, Some(&body)).batch_syncfs, "{fstype} never batches");
    }
    test_reset();
}

// A sticky unsettled still drains later holds, so held never grows past one cap after a failure.
#[test]
fn a_sticky_unsettled_still_drains_later_holds() {
    test_reset();
    let d = TestDir::new("durable-sticky-drain");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let src = d.file("first.txt", "body");
    let mut p = quiet(&flag, &mut sink, &mut durability);
    crate::backend::copyfile::copy_any(&src, &out.join("first.txt"), &mut p).expect("copy");
    drop(p);
    test_set_syncfs_errno(EIO);
    assert!(durability.flush_dirs().is_err(), "the failed syncfs sets the sticky flag");
    test_set_syncfs_errno(0);
    for n in 0..64 {
        let src = srcdir.join(format!("g{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        let mut p = quiet(&flag, &mut sink, &mut durability);
        crate::backend::copyfile::copy_any(&src, &out.join(format!("g{n}.txt")), &mut p).expect("a copy still lands past a sticky failure");
        drop(p);
    }
    assert_eq!(durability.held_len(), 0, "the next cap drains instead of growing past it");
    assert!(durability.flush_dirs().is_err(), "every later confirm fails sticky");
    test_reset();
}

// The clone of the first held file confirms the batch, then drops after syncfs.
#[test]
fn clone_of_first_held_file_confirms_batch_then_drops() {
    test_reset();
    let d = TestDir::new("durable-clone-life");
    let out = d.dir("out");
    test_mark_durable(&out);
    test_set_fake_mountinfo(Some(&vfat_body_for(&out)));
    let mut durability = Durability::begin(&out);
    test_set_fake_mountinfo(None);
    assert!(durability.batch_syncfs, "a fake vfat stick batches");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    for name in ["a.txt", "b.txt"] {
        let src = d.file(name, "body");
        let mut p = quiet(&flag, &mut sink, &mut durability);
        crate::backend::copyfile::copy_any(&src, &out.join(name), &mut p).expect("copy");
        drop(p);
    }
    assert_eq!(durability.held_len(), 2, "two files held");
    durability.flush_dirs().expect("confirm");
    assert_eq!(durability.held_len(), 0, "a confirm drains the held set");
    assert_eq!(test_syncfs_count(), 1, "one syncfs on the clone: {:?}", test_order());
    let order = test_order();
    let clone_at = order.iter().position(|s| s == "clone").expect("clone before any close");
    let syncfs_at = order.iter().position(|s| s == "syncfs").expect("one syncfs in the order");
    let drop_at = order.iter().position(|s| s == "clone-drop").expect("clone drops after syncfs");
    let dir_at = order.iter().position(|s| s == "dir").expect("one folder fsync in the order");
    let release_at = order.iter().position(|s| s == "release").expect("held files close");
    assert!(clone_at < syncfs_at, "clone preserves the pre-write description before confirm: {:?}", order);
    assert_eq!(order.iter().skip(release_at).filter(|s| *s == "release").count(), 2, "both originals close together: {:?}", order);
    assert!(syncfs_at < drop_at && drop_at < release_at && release_at < dir_at, "syncfs, nonfinal clone drop, releases, folder fsync: {:?}", order);
    test_reset();
}

// A clone refused keeps every batch source after draining; injected, never a kernel proof.
#[test]
fn clone_failure_keeps_every_batch_source_drained() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-clone-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    test_mark_durable(&out);
    test_set_fake_mountinfo(Some(&vfat_body_for(&out)));
    let mut durability = Durability::begin(&out);
    test_set_fake_mountinfo(None);
    assert!(durability.batch_syncfs, "a fake vfat stick batches");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let settled = std::sync::atomic::AtomicU64::new(0);
    let mut batch = crate::backend::movebatch::MoveBatch::new();
    let (tx, rx) = channel();
    let mut steps = Vec::new();
    for n in 0..2 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        let source = crate::backend::undo::ItemIdentity::inspect(&src).expect("source recorded");
        let name = src.file_name().unwrap().to_string_lossy().to_string();
        match crate::backend::movebatch::stage_copy(1, n, &name, &src, &out.join(format!("f{n}.txt")), source, &flag, &tx, &settled, &mut steps, &mut durability, &mut batch) {
            crate::backend::movebatch::MoveOutcome::Deferred => {}
            crate::backend::movebatch::MoveOutcome::Done(r) => panic!("staging copies, got {:?}", r.map(|_| "ok")),
        }
    }
    test_set_fail_clone(true);
    let (counts, retry) = crate::backend::movebatch::close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    test_set_fail_clone(false);
    assert_eq!((counts.ok, counts.failed), (0, 2), "no clone means no confirm");
    assert_eq!(retry.len(), 2, "every unconfirmed source is offered again");
    assert_eq!(test_syncfs_count(), 0, "a refused clone never syncfs: {:?}", test_order());
    assert_eq!(durability.held_len(), 0, "a failed clone drains the held set");
    assert!(durability.flush_dirs().is_err(), "the clone failure stays sticky");
    drop(tx);
    for msg in rx.iter() {
        if let crate::backend::opsreq::OpMsg::Item { ok: false, err, .. } = msg {
            assert_eq!(err, DIR_UNCONFIRMED, "unconfirmed batch: {err}");
        }
    }
    for n in 0..2 {
        assert!(srcdir.join(format!("f{n}.txt")).exists(), "the source stays whole");
        assert!(out.join(format!("f{n}.txt")).exists(), "the landed copy stays beside it");
    }
    test_reset();
}

// A syncfs refused on the held-file clone keeps every source; injected, never a kernel EIO proof.
#[test]
fn clone_syncfs_failure_keeps_every_batch_source() {
    use std::sync::mpsc::channel;
    test_reset();
    let d = TestDir::new("durable-clone-syncfs-fail");
    let srcdir = d.dir("src");
    let out = d.dir("out");
    test_mark_durable(&out);
    test_set_fake_mountinfo(Some(&vfat_body_for(&out)));
    let mut durability = Durability::begin(&out);
    test_set_fake_mountinfo(None);
    assert!(durability.batch_syncfs, "a fake vfat stick batches");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let settled = std::sync::atomic::AtomicU64::new(0);
    let mut batch = crate::backend::movebatch::MoveBatch::new();
    let (tx, _rx) = channel();
    let mut steps = Vec::new();
    for n in 0..2 {
        let src = srcdir.join(format!("f{n}.txt"));
        std::fs::write(&src, "body").unwrap();
        let source = crate::backend::undo::ItemIdentity::inspect(&src).expect("source recorded");
        let name = src.file_name().unwrap().to_string_lossy().to_string();
        match crate::backend::movebatch::stage_copy(1, n, &name, &src, &out.join(format!("f{n}.txt")), source, &flag, &tx, &settled, &mut steps, &mut durability, &mut batch) {
            crate::backend::movebatch::MoveOutcome::Deferred => {}
            crate::backend::movebatch::MoveOutcome::Done(r) => panic!("staging copies, got {:?}", r.map(|_| "ok")),
        }
    }
    test_set_syncfs_errno(EIO);
    let (counts, retry) = crate::backend::movebatch::close_normal(&mut batch, 1, &tx, &mut steps, &mut durability);
    test_set_syncfs_errno(0);
    assert_eq!((counts.ok, counts.failed), (0, 2), "a refused syncfs confirms nothing");
    assert_eq!(retry.len(), 2, "every unconfirmed source is offered again");
    assert_eq!(test_syncfs_count(), 0, "a failed syncfs never counts: {:?}", test_order());
    assert_eq!(durability.held_len(), 0, "a failed confirm drains the held set");
    let order = test_order();
    assert!(order.contains(&"clone".to_string()), "the clone precedes the refused syncfs: {:?}", order);
    drop(tx);
    for n in 0..2 {
        assert!(srcdir.join(format!("f{n}.txt")).exists(), "the source stays whole");
    }
    test_reset();
}

// A wave held open behind its gate, then an operation that ends: no descriptor stays open when its call returns.
#[test]
fn a_duplicate_on_a_batch_target_leaves_nothing_open_when_it_returns() {
    test_reset();
    let d = TestDir::new("durable-end-duplicate");
    let src = d.file("a.txt", "body");
    test_mark_durable(d.path());
    test_set_fake_mountinfo(Some(&vfat_body_for(d.path())));
    test_hold_waves(true);
    let (outcome, _) = crate::backend::ops::duplicate(&src);
    test_set_fake_mountinfo(None);
    outcome.expect("duplicate");
    assert_eq!(test_releases(), 1, "the copy's file went to a wave");
    assert_eq!(test_open_under(d.path()), 0, "no descriptor is left on the drive");
    test_reset();
}

#[test]
fn a_fallback_rename_on_a_batch_target_leaves_nothing_open_when_it_returns() {
    test_reset();
    let d = TestDir::new("durable-end-rename");
    let from = d.file("source.txt", "body");
    let to = d.join("target.txt");
    test_mark_durable(d.path());
    test_set_fake_mountinfo(Some(&vfat_body_for(d.path())));
    test_hold_waves(true);
    crate::backend::renamecompat::copy_then_remove(&from, &to).expect("rename by exclusive copy");
    test_set_fake_mountinfo(None);
    assert_eq!(test_releases(), 1, "the copy's file went to a wave");
    assert_eq!(test_open_under(d.path()), 0, "no descriptor is left on the drive");
    test_reset();
}

#[test]
fn a_redo_on_a_batch_target_leaves_nothing_open_when_it_returns() {
    use crate::backend::undo::{Entry, Journal};
    test_reset();
    let d = TestDir::new("durable-end-redo");
    let src = d.file("a.txt", "body");
    test_mark_durable(d.path());
    let (outcome, steps) = crate::backend::ops::duplicate(&src);
    outcome.expect("duplicate");
    let mut journal = Journal::new();
    journal.push(Entry { op: "copy".into(), steps });
    journal.undo().expect("undo removes the copy");
    test_reset_counts();
    test_set_fake_mountinfo(Some(&vfat_body_for(d.path())));
    test_hold_waves(true);
    let (tx, _rx) = std::sync::mpsc::channel();
    journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx).expect("redo");
    test_set_fake_mountinfo(None);
    assert_eq!(test_releases(), 1, "the redone copy's file went to a wave");
    assert_eq!(test_open_under(d.path()), 0, "no descriptor is left on the drive");
    test_reset();
}

// A destination reached through a symlink lands on the stick the link points at, so it batches like the stick's own path.
#[test]
fn a_symlinked_destination_into_a_vfat_stick_batches() {
    test_reset();
    let d = TestDir::new("durable-link-batch");
    let stick = d.dir("stick").canonicalize().expect("the real stick path");
    let link = d.join("link");
    std::os::unix::fs::symlink(&stick, &link).expect("the link");
    test_mark_durable(&link);
    test_mark_durable(&stick);
    let body = vfat_body_for(&stick);
    assert!(super::Durability::begin_from(&stick, Some(&body)).batch_syncfs, "the stick's own path batches");
    assert!(super::Durability::begin_from(&link, Some(&body)).batch_syncfs, "the link into it batches too");
    let share = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - cifs //nas/media rw\n", stick.display());
    assert!(!super::Durability::begin_from(&link, Some(&share)).batch_syncfs, "a link into a share never batches");
    test_reset();
}

// A link into a share is followed no further than the share's edge: no readlink lands inside a mount that can wedge.
#[test]
fn a_link_into_a_share_is_never_read_past_the_share_edge() {
    test_reset();
    let d = TestDir::new("durable-link-wedge");
    let share = d.dir("share").canonicalize().expect("the real share path");
    let link = d.join("link");
    std::os::unix::fs::symlink(&share, &link).expect("the link");
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - cifs //nas/media rw\n", share.display());
    let dest = link.join("sub").join("leaf");
    let real = crate::backend::extclass::resolved_in(&dest, &body);
    let reads = crate::backend::extclass::test_link_reads();
    assert!(reads.contains(&link), "the link itself is read, so the hook sees the walk: {:?}", reads);
    assert!(reads.iter().all(|p| !p.starts_with(&share)), "no syscall reaches the share: {:?}", reads);
    assert_eq!(real, share.join("sub").join("leaf"), "the answer is the share's edge plus the unresolved tail");
    test_reset();
}

// A chain of links lands on the stick, and a loop answers the raw path once the kernel's bound is passed.
#[test]
fn a_link_chain_resolves_and_a_loop_answers_the_raw_path() {
    test_reset();
    let d = TestDir::new("durable-link-chain");
    let stick = d.dir("stick").canonicalize().expect("the real stick path");
    let (first, second) = (d.join("first"), d.join("second"));
    std::os::unix::fs::symlink("second", &first).expect("the relative link");
    std::os::unix::fs::symlink(stick.join("..").join("stick"), &second).expect("the link through a parent step");
    let body = "1 0 8:1 / / rw - ext4 /dev/a rw\n";
    assert_eq!(crate::backend::extclass::resolved_in(&first, body), stick, "both links and the parent step are followed");
    let (ping, pong) = (d.join("ping"), d.join("pong"));
    std::os::unix::fs::symlink(&pong, &ping).expect("one end of the loop");
    std::os::unix::fs::symlink(&ping, &pong).expect("the other end");
    assert_eq!(crate::backend::extclass::resolved_in(&ping, body), ping, "a loop answers the raw path");
    test_reset();
}

// Three small files through the link cost one syncfs and no per-file fsync, the count a bare stick path gives.
#[test]
fn a_transfer_through_a_symlinked_destination_confirms_with_one_syncfs() {
    test_reset();
    let d = TestDir::new("durable-link-transfer");
    let stick = d.dir("stick").canonicalize().expect("the real stick path");
    let link = d.join("link");
    std::os::unix::fs::symlink(&stick, &link).expect("the link");
    let sources: Vec<String> = ["a.txt", "b.txt", "c.txt"].iter().map(|n| d.file(n, "body").to_string_lossy().to_string()).collect();
    test_mark_durable(&link);
    test_set_fake_mountinfo(Some(&vfat_body_for(&stick)));
    let (tx, rx) = std::sync::mpsc::channel();
    crate::backend::opsreq::run_transfer(1, false, sources, link.clone(), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), tx);
    test_set_fake_mountinfo(None);
    drop(rx);
    assert_eq!(test_counts().0, 0, "no file is fsynced on its own: {:?}", test_order());
    assert_eq!(test_syncfs_count(), 1, "one syncfs covers the three files: {:?}", test_order());
    assert!(stick.join("c.txt").exists(), "the files landed on the stick");
    test_reset();
}

// A link into an rclone mount is rclone: the upload stays in the background and no file is fsynced to force it.
#[test]
fn a_symlinked_destination_into_an_rclone_mount_stays_rclone() {
    test_reset();
    let d = TestDir::new("durable-link-rclone");
    let mount = d.dir("remote").canonicalize().expect("the real mount path");
    let link = d.join("link");
    std::os::unix::fs::symlink(&mount, &link).expect("the link");
    test_mark_durable(&link);
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - fuse.rclone remote: rw\n", mount.display());
    let begun = super::Durability::begin_from(&link, Some(&body));
    assert!(begun.rclone, "the link reads as the rclone mount it points into");
    assert!(!begun.durable && !begun.batch_syncfs, "so nothing forces its upload");
    test_reset();
}
