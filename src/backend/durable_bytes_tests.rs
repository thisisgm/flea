// A FAT batch counts each small file when it is written, so its held bytes, not only its file count, bound how far the card runs ahead.
use super::*;
use crate::backend::copyfile::Progress;
use crate::backend::testdir::TestDir;

// 900 KiB stays under the first 1 MiB slice, so each file skips its own confirm and waits for the batch.
const MID_BYTES: usize = 900 * 1024;

#[test]
fn files_just_under_a_slice_confirm_every_slice_of_bytes_not_every_64_files() {
    test_reset();
    let d = TestDir::new("durable-held-bytes");
    let (srcdir, out) = (d.dir("src"), d.dir("out"));
    test_mark_durable(&out);
    test_set_fake_mountinfo(Some(&format!(
        "1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / {} rw - vfat /dev/sda1 rw\n",
        out.display()
    )));
    let mut durability = Durability::begin(&out);
    test_set_fake_mountinfo(None);
    assert!(durability.batch_syncfs, "a /dev vfat stick batches");
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut reported = 0u64;
    let mut most_ahead = 0u64;
    for n in 0..20 {
        // The file is reported before it is held, so the peak is every held file plus this one.
        let ahead_at_report = (durability.held_len() as u64 + 1) * MID_BYTES as u64;
        let src = srcdir.join(format!("m{n}.bin"));
        std::fs::write(&src, vec![b'm'; MID_BYTES]).unwrap();
        let mut sink = |bytes: u64, _: u64| reported = bytes;
        let mut p = Progress {
            cancel: &flag,
            on_bytes: &mut sink,
            partial: None,
            tree: None,
            manifest: None,
            durability: Some(&mut durability),
            for_move: false,
        };
        crate::backend::copyfile::copy_any(&src, &out.join(format!("m{n}.bin")), &mut p)
            .expect("copy");
        drop(p);
        assert_eq!(
            reported, MID_BYTES as u64,
            "each file reports its own bytes when written"
        );
        most_ahead = most_ahead.max(ahead_at_report);
    }
    assert!(
        most_ahead < HELD_BYTES + MID_BYTES as u64,
        "counted bytes never run more than one slice and one file past the last confirm: {most_ahead}"
    );
    assert_eq!(
        test_syncfs_count(),
        2,
        "twenty 900 KiB files confirm twice, each time a slice of bytes is held"
    );
    assert_eq!(
        durability.held_len(),
        0,
        "the twentieth file closed the second slice"
    );
    durability.release_held();
    test_reset();
}

// 3 MiB passes the first slice, so the file confirms its own bytes before it is held.
const BIG_BYTES: usize = 3 * 1024 * 1024;

#[test]
fn files_that_confirmed_their_own_slices_add_no_held_bytes() {
    test_reset();
    let d = TestDir::new("durable-held-confirmed");
    let (srcdir, out) = (d.dir("src"), d.dir("out"));
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    for n in 0..4 {
        let src = srcdir.join(format!("b{n}.bin"));
        std::fs::write(&src, vec![b'b'; BIG_BYTES]).unwrap();
        let mut p = Progress {
            cancel: &flag,
            on_bytes: &mut sink,
            partial: None,
            tree: None,
            manifest: None,
            durability: Some(&mut durability),
            for_move: false,
        };
        crate::backend::copyfile::copy_any(&src, &out.join(format!("b{n}.bin")), &mut p)
            .expect("copy");
    }
    let log = test_range_log();
    assert_eq!(
        (
            test_range_waits(),
            log.iter().filter(|s| *s == "fsync").count()
        ),
        (4, 4),
        "each file waited on its own slice and fsynced itself: {log:?}"
    );
    assert_eq!(
        (durability.held_len(), test_syncfs_count()),
        (4, 0),
        "12 MiB of files that each confirmed themselves never close a batch by bytes"
    );
    durability.release_held();
    test_reset();
}

#[test]
fn small_files_still_confirm_once_per_64() {
    test_reset();
    let d = TestDir::new("durable-held-count");
    let (srcdir, out) = (d.dir("src"), d.dir("out"));
    test_mark_durable(&out);
    let mut durability = Durability::begin(&out);
    durability.batch_syncfs = true;
    let flag = std::sync::atomic::AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    for n in 0..HELD_CAP - 1 {
        let src = srcdir.join(format!("s{n}.txt"));
        std::fs::write(&src, vec![b's'; 4096]).unwrap();
        let mut p = Progress {
            cancel: &flag,
            on_bytes: &mut sink,
            partial: None,
            tree: None,
            manifest: None,
            durability: Some(&mut durability),
            for_move: false,
        };
        crate::backend::copyfile::copy_any(&src, &out.join(format!("s{n}.txt")), &mut p)
            .expect("copy");
    }
    assert_eq!(
        (durability.held_len(), test_syncfs_count()),
        (HELD_CAP - 1, 0),
        "63 small files stay one batch"
    );
    let src = srcdir.join("last.txt");
    std::fs::write(&src, vec![b's'; 4096]).unwrap();
    let mut p = Progress {
        cancel: &flag,
        on_bytes: &mut sink,
        partial: None,
        tree: None,
        manifest: None,
        durability: Some(&mut durability),
        for_move: false,
    };
    crate::backend::copyfile::copy_any(&src, &out.join("last.txt"), &mut p).expect("copy");
    drop(p);
    assert_eq!(
        (durability.held_len(), test_syncfs_count()),
        (0, 1),
        "the 64th file confirms the batch"
    );
    test_reset();
}
