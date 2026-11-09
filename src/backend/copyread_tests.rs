use super::*;
use crate::backend::testdir::TestDir;
use std::path::Path;
use std::time::Instant;

const TEST_BOUND: Duration = Duration::from_secs(5);
const PARKED_READERS: usize = 4;

struct HeldRead {
    entered: Sender<()>,
    release: Receiver<()>,
    exited: Sender<()>,
}

impl Read for HeldRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        let _ = self.entered.send(());
        let _ = self.release.recv();
        Ok(0)
    }
}

impl Drop for HeldRead {
    fn drop(&mut self) {
        let _ = self.exited.send(());
    }
}

struct Parked {
    release: Option<Sender<()>>,
    exited: Receiver<()>,
}

impl Drop for Parked {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
            let _ = self.exited.recv_timeout(TEST_BOUND);
        }
    }
}

fn park(path: &Path) -> Parked {
    let (entered, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (exited, done) = mpsc::channel();
    test_reader(HeldRead { entered, release: held, exited });
    let mut reader = Reader::new(File::open(path).unwrap(), CHUNK as u64, Some(FUSE_MAGIC)).unwrap();
    let parked = Parked { release: Some(release), exited: done };
    assert!(matches!(&reader.source, Source::Pending(_)), "a free slot starts a worker");
    assert_eq!(reader.read(&AtomicBool::new(true)).unwrap_err().kind(), io::ErrorKind::Interrupted);
    drop(reader);
    started.recv_timeout(TEST_BOUND).expect("the cancelled source read remains parked");
    parked
}

#[test]
fn four_parked_reads_fall_back_to_direct_and_releasing_one_reuses_its_slot() {
    let _workers = test_workers();
    let d = TestDir::new("copyreadcap");
    let path = d.file("source", "body");
    let mut parked: Vec<_> = (0..PARKED_READERS).map(|_| park(&path)).collect();
    let mut overflow = Reader::new(File::open(&path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
    assert!(matches!(&overflow.source, Source::File(_)), "four stalled reads must cap worker retention; the next FUSE source reads directly");
    let cancel = AtomicBool::new(false);
    assert_eq!(overflow.read(&cancel).unwrap(), 4);
    assert_eq!(&overflow.buffer[..4], b"body");
    drop(overflow);
    drop(parked.pop().unwrap());
    let started = Instant::now();
    loop {
        let next = Reader::new(File::open(&path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
        if matches!(&next.source, Source::Pending(_)) {
            break;
        }
        assert!(started.elapsed() < TEST_BOUND, "releasing a parked reader must free its worker slot");
        std::thread::yield_now();
    }
    drop(parked);
    wait_for_workers();
}

#[test]
fn only_fuse_sources_take_the_read_worker() {
    let _workers = test_workers();
    let d = TestDir::new("copyreadclass");
    let src = d.file("source", "body");
    for magic in [None, Some(0xEF53), Some(0x01021994), Some(FUSE_MAGIC)] {
        let file = File::open(&src).unwrap();
        let mut reader = Reader::new(file, 4, magic).unwrap();
        assert_eq!(matches!(&reader.source, Source::Pending(_)), magic == Some(FUSE_MAGIC));
        let cancel = AtomicBool::new(false);
        assert_eq!(reader.read(&cancel).unwrap(), 4);
        assert_eq!(&reader.buffer[..4], b"body");
        assert_eq!(reader.read(&cancel).unwrap(), 0);
    }
}

#[test]
fn a_failed_spawn_releases_its_slot_and_reads_the_original_source_directly() {
    let _workers = test_workers();
    let d = TestDir::new("copyreadspawnfail");
    let path = d.file("source", "body");
    TEST_SPAWN_FAILURE.with(|slot| slot.set(true));
    let mut reader = Reader::new(File::open(&path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
    assert!(matches!(&reader.source, Source::File(_)), "a failed spawn falls back instead of failing the copy");
    assert_eq!(LIVE_READ_WORKERS.load(Ordering::Relaxed), 0, "a failed spawn returns its reservation");
    assert_eq!(reader.read(&AtomicBool::new(false)).unwrap(), 4);
    assert_eq!(&reader.buffer[..4], b"body", "the original source descriptor survives the failed spawn");
    let next = Reader::new(File::open(&path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
    assert!(matches!(&next.source, Source::Pending(_)), "a later spawn can claim the released slot");
    drop(next);
    wait_for_workers();
}

struct PanickingRead;

impl Read for PanickingRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("{}", crate::backend::quietpanic::DELIBERATE);
    }
}

#[test]
fn a_panicking_source_releases_its_worker_slot() {
    let _workers = test_workers();
    let d = TestDir::new("copyreadpanic");
    let path = d.file("source", "body");
    crate::backend::quietpanic::quiet();
    test_reader(PanickingRead);
    let mut reader = Reader::new(File::open(path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
    assert_eq!(reader.read(&AtomicBool::new(false)).unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    drop(reader);
    wait_for_workers();
}

struct HeldClose(HeldRead);

impl Read for HeldClose {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

impl Drop for HeldClose {
    fn drop(&mut self) {
        let _ = self.0.entered.send(());
        let _ = self.0.release.recv();
    }
}

#[test]
fn a_source_stalled_in_close_keeps_its_worker_slot() {
    let _workers = test_workers();
    let d = TestDir::new("copyreadclose");
    let path = d.file("source", "body");
    let parked: Vec<_> = (0..PARKED_READERS - 1).map(|_| park(&path)).collect();
    let (entered, closing) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (exited, done) = mpsc::channel();
    test_reader(HeldClose(HeldRead { entered, release: held, exited }));
    let close = Parked { release: Some(release), exited: done };
    let mut reader = Reader::new(File::open(&path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
    assert_eq!(reader.read(&AtomicBool::new(false)).unwrap(), 0);
    drop(reader);
    closing.recv_timeout(TEST_BOUND).expect("the source is held inside close");
    let overflow = Reader::new(File::open(path).unwrap(), 4, Some(FUSE_MAGIC)).unwrap();
    assert!(matches!(&overflow.source, Source::File(_)), "source close must finish before its worker slot is returned");
    drop(close);
    drop(parked);
    wait_for_workers();
}

#[test]
fn the_hole_search_flags_match_the_target_abi() {
    assert_eq!((SEEK_SET, SEEK_DATA, SEEK_HOLE), (libc::SEEK_SET, libc::SEEK_DATA, libc::SEEK_HOLE));
    assert_eq!((ENXIO, EINVAL, EOPNOTSUPP), (libc::ENXIO, libc::EINVAL, libc::EOPNOTSUPP));
}
