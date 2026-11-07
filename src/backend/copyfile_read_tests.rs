use super::*;
use crate::backend::{copyread, testdir::TestDir};
use std::io::{self, Read};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

// Each test releases the held read only after the result, so a result inside this deadline came before the read returned; it ends only a cancel that never lands.
const CANCEL_DEADLINE: Duration = Duration::from_secs(5);
const SETUP_BOUND: Duration = Duration::from_secs(5);

fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
    Progress { cancel: flag, on_bytes: sink, partial: None, tree: None, manifest: None, durability: None, for_move: false }
}

// Mid-copy the bytes are in the destination's hidden sibling, the one file in the folder with that prefix.
fn staged_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path())
        .find(|path| path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(crate::backend::copystage::PREFIX)))
}

struct HeldRead {
    first: bool,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    left: mpsc::Sender<()>,
}

impl Read for HeldRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.first {
            self.first = false;
            buffer[..4].copy_from_slice(b"head");
            return Ok(4);
        }
        self.entered.send(()).unwrap();
        self.release.recv().unwrap();
        buffer[..4].copy_from_slice(b"late");
        Ok(4)
    }
}

impl Drop for HeldRead {
    fn drop(&mut self) {
        let _ = self.left.send(());
    }
}

#[test]
fn a_stalled_read_cancel_removes_the_destination_before_the_read_returns() {
    cancel_held_read(false);
}

#[test]
fn a_read_released_after_cancel_never_writes_to_the_partial_or_a_reused_name() {
    cancel_held_read(true);
}

fn cancel_held_read(first: bool) {
    let _workers = copyread::test_workers();
    let d = TestDir::new("copyreadcancel");
    let src = d.file("source", "body");
    let dst = d.join("destination");
    d.assert_contains(&dst);
    let flag = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&flag);
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (left, exited) = mpsc::channel();
    let (result, done) = mpsc::channel();
    let target = dst.clone();
    let worker = std::thread::spawn(move || {
        copyread::test_reader(HeldRead { first, entered, release: held, left });
        // A sibling, so the test can hold the partial inode by its name.
        let _seams = crate::backend::copystage_tests::sibling_only();
        let mut sink = |_: u64, _: u64| {};
        let mut p = quiet(&cancel, &mut sink);
        let answer = copy_any(&src, &target, &mut p);
        result.send((answer, p.partial)).unwrap();
    });
    waiting.recv_timeout(SETUP_BOUND).expect("the read is parked after the exclusive create");
    assert!(!dst.exists(), "the name stays absent while the copy is unfinished");
    let staged = staged_in(d.path()).expect("the bytes go to a hidden sibling");
    let held_destination = std::fs::File::open(&staged).expect("hold the partial inode across unlink");
    flag.store(true, Ordering::Relaxed);
    let started = Instant::now();
    let prompt = done.recv_timeout(CANCEL_DEADLINE);
    let elapsed = started.elapsed();
    let prompt_returned = prompt.is_ok();
    let removed_promptly = !dst.exists() && !staged.exists();
    if prompt_returned {
        d.assert_contains(&staged);
        std::fs::write(&staged, "replacement").unwrap();
    }
    release.send(()).expect("release the read even when the regression is red");
    let (answer, partial) = prompt.unwrap_or_else(|_| done.recv_timeout(SETUP_BOUND).unwrap());
    worker.join().unwrap();
    exited.recv_timeout(SETUP_BOUND).expect("the abandoned reader exits when released");
    assert!(prompt_returned, "Cancel remained blocked on the source read for at least {:?}", CANCEL_DEADLINE);
    assert!(removed_promptly, "cancel must unlink the partial before the read returns");
    assert_eq!(answer.unwrap_err().msg, "cancelled");
    assert!(partial.is_none(), "a cancelled partial is removed rather than journaled");
    assert_eq!(held_destination.metadata().unwrap().len(), if first { 4 } else { 0 }, "late source bytes must never reach even the unlinked inode");
    assert_eq!(std::fs::read_to_string(&staged).unwrap(), "replacement", "late bytes must never reach a reused destination name");
    assert!(!dst.exists(), "and the cancelled copy never took its name");
    eprintln!("stalled-read cancel returned within {:?}", elapsed);
}

struct LiveRead {
    step: u8,
    entered: mpsc::Sender<(u8, usize, usize)>,
    release: mpsc::Receiver<()>,
}

impl Read for LiveRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.step == 3 {
            return Ok(0);
        }
        self.entered.send((self.step, buffer.as_ptr() as usize, buffer.len())).unwrap();
        self.release.recv().unwrap();
        buffer[..4].fill(b'a' + self.step);
        self.step += 1;
        Ok(4)
    }
}

#[test]
fn a_slow_live_source_reports_each_chunk_before_its_next_read_finishes() {
    let _workers = copyread::test_workers();
    let d = TestDir::new("copyreadlive");
    let src = d.file("source", "aaaabbbbcccc");
    let dst = d.join("destination");
    d.assert_contains(&dst);
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (progress, updates) = mpsc::channel();
    let target = dst.clone();
    let worker = std::thread::spawn(move || {
        copyread::test_reader(LiveRead { step: 0, entered, release: held });
        let _seams = crate::backend::copystage_tests::sibling_only();
        let flag = AtomicBool::new(false);
        let mut sink = |done: u64, total: u64| progress.send((done, total)).unwrap();
        copy_any(&src, &target, &mut quiet(&flag, &mut sink))
    });
    let mut address = None;
    for step in 0..3 {
        let (reading, pointer, capacity) = waiting.recv_timeout(SETUP_BOUND).expect("a slow source asks for its next chunk");
        assert_eq!(reading, step);
        assert_eq!(capacity, 8 * 1024, "the source retains the existing minimum chunk size");
        assert_eq!(*address.get_or_insert(pointer), pointer, "the same buffer moves through every read without a data copy");
        assert!(updates.try_recv().is_err(), "a held read cannot report bytes it has not supplied");
        release.send(()).unwrap();
        assert_eq!(updates.recv_timeout(SETUP_BOUND).unwrap(), (u64::from(step + 1) * 4, 12));
        assert!(!dst.exists(), "the name stays absent until the copy is whole");
        assert_eq!(std::fs::metadata(staged_in(d.path()).unwrap()).unwrap().len(), u64::from(step + 1) * 4);
    }
    worker.join().unwrap().expect("the slow source finishes normally");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "aaaabbbbcccc");
}

#[test]
fn a_stalled_transfer_reports_cancelled_before_the_read_returns() {
    let _workers = copyread::test_workers();
    use crate::backend::opsreq::{run_transfer, OpMsg};
    let d = TestDir::new("copyreadtransfer");
    let src = d.file("source", "body");
    let dest = d.dir("out");
    let dst = dest.join("source");
    d.assert_contains(&dst);
    let flag = Arc::new(AtomicBool::new(false));
    let live = crate::backend::opscancel::Live::new();
    live.claim(17, &flag);
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (left, exited) = mpsc::channel();
    let (messages, received) = mpsc::channel();
    let (result, done) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        copyread::test_reader(HeldRead { first: false, entered, release: held, left });
        run_transfer(17, false, vec![src.to_string_lossy().into_owned()], dest, flag, messages);
        result.send(()).unwrap();
    });
    waiting.recv_timeout(SETUP_BOUND).expect("the transfer is inside its source read");
    live.cancel(17);
    let started = Instant::now();
    let prompt = done.recv_timeout(CANCEL_DEADLINE).is_ok();
    let elapsed = started.elapsed();
    let removed_promptly = !dst.exists();
    release.send(()).unwrap();
    worker.join().unwrap();
    exited.recv_timeout(SETUP_BOUND).expect("the released source is reaped");
    assert!(prompt, "the terminal operation result must not wait for the source read");
    assert!(removed_promptly);
    let terminal = received.try_iter().find_map(|message| match message {
        OpMsg::TransferDone { id, ok, failed, skipped, cancelled, entry, .. } => Some((id, ok, failed, skipped, cancelled, entry)),
        _ => None,
    }).expect("the cancelled operation sends its terminal row result");
    assert_eq!((terminal.0, terminal.1, terminal.2, terminal.3, terminal.4), (17, 0, 0, 1, true));
    assert!(terminal.5.steps.is_empty(), "a removed partial creates no undo step");
    eprintln!("stalled transfer cancel returned within {:?}", elapsed);
}

#[test]
#[ignore = "manual local-copy timing through the host broker"]
fn copyfile_local_benchmark() {
    let d = TestDir::new("copyreadbench");
    let src = d.file("source", &"x".repeat(64 * 1024 * 1024));
    let flag = AtomicBool::new(false);
    let mut samples = Vec::new();
    for index in 0..9 {
        let dst = d.join(&format!("copy-{index}"));
        d.assert_contains(&dst);
        let mut sink = |_: u64, _: u64| {};
        let started = Instant::now();
        copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).unwrap();
        let elapsed = started.elapsed().as_micros();
        assert_eq!(std::fs::metadata(&dst).unwrap().len(), 64 * 1024 * 1024);
        samples.push(elapsed);
    }
    eprintln!("local 64 MiB copy microseconds: {:?}", samples);
    samples.sort();
    eprintln!("local 64 MiB median microseconds: {}", samples[samples.len() / 2]);
}
