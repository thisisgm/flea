use super::*;
use crate::backend::durable::{HELD_CAP, MAX_OPEN_HELD};
use crate::backend::{copyfile, durable, opsreq, testdir::TestDir};
use std::sync::{mpsc::channel, Arc};

const FILES: usize = 1_000;
const FILE_BYTES: usize = 4_096;
const ENOSPC: i32 = 28;
// One vfat flush body over the destination, so every confirm test mounts the same stick.
fn vfat_body(out: &std::path::Path) -> String {
    format!("21 1 8:1 / {} rw,flush - vfat /dev/test rw,flush\n", out.display())
}
// A move's finish confirms the destination folder and the source folder once each.
const FINISH_DIR_FSYNCS: usize = 2;
// Two full batches and a part, so a run starts three waves and finish joins the last.
const WAVE_FILES: usize = 2 * HELD_CAP + 3;

struct Fixture {
    _dir: TestDir,
    src: PathBuf,
    out: PathBuf,
    durability: Durability,
    batch: MoveBatch,
    cancel: AtomicBool,
    settled: AtomicU64,
    tx: Sender<OpMsg>,
    steps: Vec<Step>,
}

impl Fixture {
    fn new() -> Self {
        durable::test_reset();
        let dir = TestDir::new("usbrca");
        let src = dir.dir("src");
        let out = dir.dir("out");
        durable::test_mark_durable(&out);
        let body = vfat_body(&out);
        durable::test_set_fake_mountinfo(Some(&body));
        let durability = Durability::begin(&out);
        durable::test_set_fake_mountinfo(None);
        assert!(durability.batch_syncfs);
        let (tx, _) = channel();
        Self { _dir: dir, src, out, durability, batch: MoveBatch::new(), cancel: AtomicBool::new(false),
            settled: AtomicU64::new(0), tx, steps: Vec::new() }
    }

    fn source(&self, n: usize) -> PathBuf {
        self.src.join(format!("f{n}"))
    }

    // Sources with bytes on disk, named as the transfer takes them.
    fn written_paths(&self, count: usize) -> Vec<String> {
        (0..count).map(|n| {
            let path = self.source(n);
            std::fs::write(&path, vec![b'x'; FILE_BYTES]).unwrap();
            path.to_string_lossy().into_owned()
        }).collect()
    }

    fn destination(&self, n: usize) -> PathBuf {
        self.out.join(format!("f{n}"))
    }

    fn stage(&mut self, n: usize) -> MoveOutcome {
        let src = self.source(n);
        std::fs::write(&src, vec![b'x'; FILE_BYTES]).unwrap();
        let source = ItemIdentity::inspect(&src).unwrap();
        stage_copy(crate::backend::opsreq::TransferItem { id: 1, index: n, name: &format!("f{n}"), src: &src, dst: &self.destination(n), source }, &self.cancel, &self.tx, &self.settled, &mut self.steps, &mut self.durability, &mut self.batch)
    }

    fn land(&mut self, n: usize) {
        assert!(matches!(self.stage(n), MoveOutcome::Deferred));
    }

    fn close(&mut self) -> CloseCounts {
        close_normal(&mut self.batch, 1, &self.tx, &mut self.steps, &mut self.durability).0
    }

    fn source_survives(&self, n: usize) {
        assert_eq!(std::fs::read(self.source(n)).unwrap(), vec![b'x'; FILE_BYTES]);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        copyfile::test_fail_write(None);
        durable::test_reset();
    }
}

// Run the transfer path, pin every counted call and forbid dirty closes or a separate final clone close.
#[test]
fn usb_thousand_file_call_counts_and_confirm_before_close() {
    let fixture = Fixture::new();
    let paths = fixture.written_paths(FILES);
    let _force = ForceCopyGuard::hold();
    let (tx, rx) = channel();
    let body = vfat_body(&fixture.out);
    durable::test_set_fake_mountinfo(Some(&body));
    opsreq::run_transfer(1, true, paths, fixture.out.clone(), Arc::new(AtomicBool::new(false)), tx);
    durable::test_set_fake_mountinfo(None);
    let done = rx.iter().find_map(|m| match m {
        OpMsg::TransferDone { ok, failed, skipped, durable, entry, .. } => Some((ok, failed, skipped, durable, entry)),
        _ => None,
    }).unwrap();
    assert_eq!((done.0, done.1, done.2, done.3), (FILES, 0, 0, true));
    assert_eq!(done.4.steps.len(), FILES);
    let confirms = FILES.div_ceil(BATCH_ITEMS);
    assert_eq!(durable::test_counts(), (0, confirms + FINISH_DIR_FSYNCS));
    assert_eq!(durable::test_syncfs_count(), confirms);
    assert_eq!(durable::test_releases(), FILES);
    let order = durable::test_order();
    let count = |step: &str| order.iter().filter(|s| s.as_str() == step).count();
    assert_eq!((count("open-dest"), count("write-dest"), count("remove")), (FILES, FILES, FILES));
    assert_eq!(count("mtime-dest"), FILES);
    assert!(durable::test_range_log().is_empty(), "small files make no range sync or file fsync calls");
    assert_eq!((count("clone"), count("clone-drop")), (confirms, confirms));
    println!("USB_COUNTS files={FILES} open={} write={} file_fsync=0 dir_fsync={} syncfs={} close={} clone={} unlink={}",
        count("open-dest"), count("write-dest"), confirms + FINISH_DIR_FSYNCS, confirms, FILES + confirms, confirms, count("remove"));
    let mut dirty = false;
    let mut clone_live = false;
    let mut dirty_closes = 0;
    let mut originals_closed_before_clone_drop = 0;
    for step in &order {
        match step.as_str() {
            "write-dest" | "mtime-dest" => dirty = true,
            "clone" => clone_live = true,
            "syncfs" => dirty = false,
            "clone-drop" => clone_live = false,
            "release" => {
                dirty_closes += usize::from(dirty);
                originals_closed_before_clone_drop += usize::from(clone_live);
            }
            "remove" => assert!(!dirty, "no source goes before its covering confirm"),
            _ => {}
        }
    }
    assert_eq!(dirty_closes, 0, "all 1,000 writable closes follow their covering syncfs");
    assert_eq!(originals_closed_before_clone_drop, 0, "the clone drops while its original remains open, so it is never an original's final close");
    for n in 0..FILES {
        assert!(!fixture.source(n).exists());
        assert_eq!(std::fs::read(fixture.destination(n)).unwrap(), vec![b'x'; FILE_BYTES]);
    }
}

// Dropping the clone before all originals keeps its close nonfinal, avoiding one serial vfat sleep per batch.
#[test]
fn usb_clone_drop_does_not_serialize_a_final_writable_close() {
    let mut f = Fixture::new();
    f.land(0);
    f.land(1);
    let closed = f.close();
    assert_eq!((closed.ok, closed.failed), (2, 0));
    let order = durable::test_order();
    let clone_drop = order.iter().position(|s| s == "clone-drop").unwrap();
    let first_release = order.iter().position(|s| s == "release").unwrap();
    assert!(clone_drop < first_release, "clone must close while its original still holds the description");
}

fn failed_confirm(errno: i32) {
    let mut f = Fixture::new();
    for n in 0..BATCH_ITEMS { f.land(n); }
    assert_eq!(f.durability.held_len(), 0, "the cap has already confirmed and drained");
    f.land(BATCH_ITEMS);
    let landed: Vec<PathBuf> = (0..=BATCH_ITEMS).map(|n| f.destination(n)).collect();
    durable::test_set_syncfs_errno(errno);
    let refused = f.durability.flush_dirs_for_many(&landed).expect_err("the refused syncfs fails the confirm");
    assert_eq!(refused.raw_os_error(), Some(errno), "the confirm returns the drive's own errno");
    let closed = f.close();
    durable::test_set_syncfs_errno(0);
    assert_eq!((closed.ok, closed.failed), (0, BATCH_ITEMS + 1));
    assert_eq!(f.durability.held_len(), 0);
    assert!(f.durability.flush_dirs().is_err(), "a failed late confirm stays sticky");
    assert_eq!(f.steps.len(), BATCH_ITEMS + 1, "one journaled step per landed copy");
    assert!(f.steps.iter().all(|s| matches!(s, Step::Copied { .. })));
    for n in 0..=BATCH_ITEMS { f.source_survives(n); }
}

#[test]
fn usb_eio_confirm_keeps_sources_after_an_earlier_cap_confirm() { failed_confirm(durable::EIO); }

fn short_copy(errno: i32, reported: &str) {
    let mut f = Fixture::new();
    f.land(0);
    copyfile::test_fail_write(Some((0, errno)));
    match f.stage(1) {
        MoveOutcome::Done(Err(failure)) => assert_eq!(failure.msg, reported, "the transfer reports the drive's own refusal"),
        _ => panic!("a refused write fails the copy"),
    }
    copyfile::test_fail_write(None);
    f.source_survives(0);
    f.source_survives(1);
    assert!(!f.destination(1).exists(), "the half-written copy went with its hidden name, so no name holds it");
    let closed = f.close();
    assert_eq!((closed.ok, closed.failed), (1, 0));
    assert!(!f.source(0).exists(), "the landed copy still finishes after confirmation");
    f.source_survives(1);
    assert_eq!(f.durability.held_len(), 0);
    assert!(matches!(&f.steps[..], [Step::Moved { .. }]), "nothing of the failed copy is left to journal");
}

#[test]
fn usb_short_write_keeps_its_source() { short_copy(0, "could not write all data"); }

#[test]
fn usb_enospc_write_keeps_its_source() { short_copy(ENOSPC, "the drive is full"); }

#[test]
fn usb_eio_write_keeps_its_source() { short_copy(durable::EIO, "the drive could not read or write the data"); }

#[test]
fn usb_cancel_keeps_unfinished_source_and_confirms_landed_files() {
    let mut f = Fixture::new();
    f.land(0);
    f.cancel.store(true, Ordering::Relaxed);
    assert!(matches!(f.stage(1), MoveOutcome::Done(Err(_))));
    assert!(!f.destination(1).exists(), "cancel removes only the unfinished destination");
    let closed = close_cancelled(&mut f.batch, 1, &f.tx, &mut f.steps, &mut f.durability).0;
    assert_eq!((closed.ok, closed.failed, closed.cancelled), (1, 0, true));
    assert!(!f.source(0).exists());
    f.source_survives(1);
    assert_eq!(f.durability.held_len(), 0);
}

// A held wave keeps its descriptors open until joined, so after every landed copy at most two batches stay open.
#[test]
fn usb_open_descriptors_never_pass_two_batches_with_a_wave_held_open() {
    let mut f = Fixture::new();
    durable::test_hold_waves(true);
    let mut peak = 0;
    for n in 0..3 * HELD_CAP {
        f.land(n);
        peak = peak.max(durable::test_open_under(&f.out));
    }
    assert!(peak > HELD_CAP, "the held wave stays open behind the next batch: {peak}");
    assert!(peak <= MAX_OPEN_HELD, "never more than two batches of descriptors: {peak}");
}

// Batch 2 confirms while wave 1 closes, then joins it, and only then starts its own wave.
#[test]
fn usb_a_wave_is_joined_after_the_next_clone_drop_and_before_the_next_release() {
    let mut f = Fixture::new();
    for n in 0..2 * HELD_CAP { f.land(n); }
    let order = durable::test_order();
    let at = |step: &str| -> Vec<usize> {
        order.iter().enumerate().filter(|(_, s)| s.as_str() == step).map(|(i, _)| i).collect()
    };
    let (drops, joins, releases) = (at("clone-drop"), at("join"), at("release"));
    assert_eq!((drops.len(), joins.len(), releases.len()), (2, 1, 2 * HELD_CAP), "{order:?}");
    assert!(drops[1] < joins[0], "the join follows batch 2's clone drop: {order:?}");
    assert!(joins[0] < releases[HELD_CAP], "the join precedes batch 2's first release: {order:?}");
}

// A refused confirm still hands every file to a wave, and finish still joins that wave.
fn failed_confirm_hands_off_and_joins(inject: fn(bool)) {
    let mut f = Fixture::new();
    durable::test_hold_waves(true);
    for n in 0..HELD_CAP - 1 { f.land(n); }
    inject(true);
    f.land(HELD_CAP - 1);
    inject(false);
    assert_eq!(f.durability.held_len(), 0);
    assert_eq!(durable::test_releases(), HELD_CAP, "every file went to a wave");
    assert_eq!(durable::test_open_under(&f.out), HELD_CAP, "the wave is still closing behind the refused confirm");
    assert!(!durable::test_order().contains(&"join".to_string()), "nothing has joined it yet");
    let finished = durable::finish(1, &f.tx, &mut f.durability, &f.out, HELD_CAP);
    assert!(!finished.ok);
    assert_eq!(finished.note, durable::DIR_UNCONFIRMED);
    assert_eq!(durable::test_open_under(&f.out), 0, "finish joined the wave");
    assert_eq!(durable::test_order().iter().filter(|s| s.as_str() == "join").count(), 1);
}

#[test]
fn usb_a_refused_syncfs_still_hands_off_and_joins_its_wave() {
    failed_confirm_hands_off_and_joins(|on| durable::test_set_syncfs_errno(if on { durable::EIO } else { 0 }));
}

#[test]
fn usb_a_refused_clone_still_hands_off_and_joins_its_wave() {
    failed_confirm_hands_off_and_joins(durable::test_set_fail_clone);
}

// The done line goes out only after the last wave is joined: counted where it is sent, then on the drive.
#[test]
fn usb_the_done_line_goes_out_after_the_last_wave_is_joined() {
    let fixture = Fixture::new();
    let paths = fixture.written_paths(WAVE_FILES);
    let _force = ForceCopyGuard::hold();
    let (tx, rx) = channel();
    let body = vfat_body(&fixture.out);
    durable::test_set_fake_mountinfo(Some(&body));
    durable::test_hold_waves(true);
    opsreq::run_transfer(1, true, paths, fixture.out.clone(), Arc::new(AtomicBool::new(false)), tx);
    durable::test_set_fake_mountinfo(None);
    assert!(rx.iter().any(|m| matches!(m, OpMsg::TransferDone { ok: WAVE_FILES, .. })));
    assert_eq!(durable::test_open_at_report(), Some(0), "no closer is outstanding where the result is reported");
    assert_eq!(durable::test_open_under(&fixture.out), 0, "no descriptor is left on the drive");
    assert_eq!(durable::test_releases(), WAVE_FILES);
}
