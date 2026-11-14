// The source and its existing chunk buffer, kept apart from the destination writer.
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Linux FUSE_SUPER_MAGIC includes GVFS and MTP mounts, even when reached through a symlinked parent.
pub(crate) const FUSE_MAGIC: i64 = 0x65735546;
// Worker reads poll Cancel every 25 ms; synchronous cleanup still waits for an unresponsive destination device.
const CANCEL_POLL: Duration = Duration::from_millis(25);
// Four workers bound the threads, source descriptors and chunk buffers retained by cancelled FUSE reads.
const MAX_READ_WORKERS: usize = 4;
static LIVE_READ_WORKERS: AtomicUsize = AtomicUsize::new(0);
// Local sources keep the original chunk sizes and direct read, with no channel or extra data copy.
pub(super) const CHUNK: usize = 256 * 1024;
const SMALL_CHUNK: usize = 8 * 1024;
// lseek(2) whence values and the errors a hole search answers, the same on x86_64 and aarch64.
const SEEK_SET: i32 = 0;
const SEEK_DATA: i32 = 3;
const SEEK_HOLE: i32 = 4;
const ENXIO: i32 = 6;
const EINVAL: i32 = 22;
const EOPNOTSUPP: i32 = 95;

extern "C" {
    fn lseek(fd: i32, offset: i64, whence: i32) -> i64;
}

struct WorkerSlot;

impl WorkerSlot {
    fn claim() -> Option<WorkerSlot> {
        LIVE_READ_WORKERS.try_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
            if live < MAX_READ_WORKERS { Some(live + 1) } else { None }
        }).ok().map(|_| WorkerSlot)
    }
}

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        LIVE_READ_WORKERS.fetch_sub(1, Ordering::Relaxed);
    }
}

struct PendingRead {
    request: Sender<Vec<u8>>,
    result: Receiver<(Vec<u8>, io::Result<usize>)>,
}

impl PendingRead {
    fn start(file: File) -> Result<PendingRead, File> {
        #[cfg(test)]
        let stand_in = TEST_READER.with(|slot| slot.borrow_mut().take());
        let Some(slot) = WorkerSlot::claim() else { return Err(file); };
        // The handoff keeps the original descriptor available if spawning fails, without a duplicate descriptor.
        let held = Arc::new(Mutex::new(Some(file)));
        let worker = Arc::clone(&held);
        let (request, buffers) = mpsc::channel::<Vec<u8>>();
        let (answers, result) = mpsc::channel();
        // The worker owns only the source and one moved buffer, never a destination descriptor or path.
        let spawned = spawn_reader(move || {
            // Declared first so the slot outlives source close and every chunk buffer, including on unwind.
            let _slot = slot;
            let reader = worker.lock().unwrap().take().unwrap();
            #[cfg(test)]
            let reader: Box<dyn Read + Send> = stand_in.unwrap_or_else(|| Box::new(reader));
            let mut reader = reader;
            while let Ok(mut buffer) = buffers.recv() {
                let answer = reader.read(&mut buffer);
                if answers.send((buffer, answer)).is_err() {
                    break;
                }
            }
        });
        match spawned {
            Ok(_) => Ok(PendingRead { request, result }),
            Err(_) => {
                let file = held.lock().unwrap().take().unwrap();
                Err(file)
            }
        }
    }
}

fn spawn_reader(work: impl FnOnce() + Send + 'static) -> io::Result<std::thread::JoinHandle<()>> {
    #[cfg(test)]
    if TEST_SPAWN_FAILURE.with(|slot| slot.replace(false)) {
        return Err(io::ErrorKind::Other.into());
    }
    std::thread::Builder::new().name("flea-copy-read".into()).spawn(work)
}

enum Source {
    File(File),
    Pending(PendingRead),
}

pub(super) struct Reader {
    source: Source,
    pub(super) buffer: Vec<u8>,
}

impl Reader {
    pub(super) fn new(file: File, size: u64, magic: Option<i64>) -> io::Result<Reader> {
        let isolated = magic == Some(FUSE_MAGIC);
        #[cfg(test)]
        let isolated = isolated || TEST_READER.with(|slot| slot.borrow().is_some());
        let source = if isolated {
            match PendingRead::start(file) {
                Ok(pending) => Source::Pending(pending),
                Err(file) => Source::File(file),
            }
        } else {
            Source::File(file)
        };
        Ok(Reader::with_source(source, size))
    }

    fn with_source(source: Source, size: u64) -> Reader {
        let buffer = vec![0; usize::try_from(size).map_or(CHUNK, |len| len.clamp(SMALL_CHUNK, CHUNK))];
        #[cfg(test)]
        BUFFERS.with(|v| v.borrow_mut().push(buffer.len()));
        Reader {
            source,
            buffer,
        }
    }

    #[cfg(test)]
    pub(super) fn read(&mut self, cancel: &AtomicBool) -> io::Result<usize> {
        self.read_upto(cancel, u64::MAX)
    }

    // A local read stops at `most` bytes, the end of the data extent a sparse copy is in.
    pub(super) fn read_upto(&mut self, cancel: &AtomicBool, most: u64) -> io::Result<usize> {
        match &mut self.source {
            Source::File(file) => {
                let len = usize::try_from(most).map_or(self.buffer.len(), |most| most.min(self.buffer.len()));
                file.read(&mut self.buffer[..len])
            }
            Source::Pending(pending) => {
                pending.request.send(std::mem::take(&mut self.buffer)).map_err(|_| io::ErrorKind::BrokenPipe)?;
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(io::ErrorKind::Interrupted.into());
                    }
                    match pending.result.recv_timeout(CANCEL_POLL) {
                        Ok((buffer, answer)) => {
                            self.buffer = buffer;
                            return answer;
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return Err(io::ErrorKind::BrokenPipe.into()),
                    }
                }
            }
        }
    }
}

impl Reader {
    // The data extent at or after `pos`, seeked to its start; None is a trailing hole, and no SEEK_DATA is one extent.
    pub(super) fn extent(&mut self, pos: u64) -> io::Result<Option<(u64, u64)>> {
        let Source::File(file) = &self.source else { return Ok(Some((pos, u64::MAX))) };
        let fd = file.as_raw_fd();
        let at = i64::try_from(pos).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let start = unsafe { lseek(fd, at, SEEK_DATA) };
        if start < 0 {
            let e = io::Error::last_os_error();
            return match e.raw_os_error() {
                Some(ENXIO) => Ok(None),
                Some(EINVAL) | Some(EOPNOTSUPP) => Ok(Some((pos, u64::MAX))),
                _ => Err(e),
            };
        }
        let end = unsafe { lseek(fd, start, SEEK_HOLE) };
        if end < 0 || unsafe { lseek(fd, start, SEEK_SET) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some((start as u64, end as u64)))
    }

    // Another inode, length or mtime than when the copy began; the descriptor answers, or the path for an isolated read.
    pub(super) fn changed(&self, before: &std::fs::Metadata, path: &std::path::Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        let now = match &self.source {
            Source::File(file) => file.metadata(),
            Source::Pending(_) => path.symlink_metadata(),
        };
        let shape = |m: &std::fs::Metadata| (m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec());
        now.is_ok_and(|now| shape(&now) != shape(before))
    }
}

#[cfg(test)]
thread_local! {
    static TEST_READER: std::cell::RefCell<Option<Box<dyn Read + Send>>> = const { std::cell::RefCell::new(None) };
    static BUFFERS: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
    static TEST_SPAWN_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
static TEST_WORKERS: Mutex<()> = Mutex::new(());

// A released reader frees its slot within milliseconds; five seconds only absorbs a loaded test host.
#[cfg(test)]
const WORKER_RELEASE_DEADLINE: Duration = Duration::from_secs(5);

#[cfg(test)]
fn wait_for_workers() {
    let started = std::time::Instant::now();
    while LIVE_READ_WORKERS.load(Ordering::Relaxed) != 0 {
        assert!(started.elapsed() < WORKER_RELEASE_DEADLINE, "released source readers must free their worker slots");
        std::thread::yield_now();
    }
}

// Worker-using tests serialize on the process-wide cap and wait for the previous test's detached cleanup.
#[cfg(test)]
pub(super) fn test_workers() -> std::sync::MutexGuard<'static, ()> {
    let held = TEST_WORKERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    wait_for_workers();
    held
}

// The next copy on this thread opens and checks its source normally, then reads this deterministic stand-in.
#[cfg(test)]
pub(super) fn test_reader(reader: impl Read + Send + 'static) {
    TEST_READER.with(|slot| *slot.borrow_mut() = Some(Box::new(reader)));
}

// Test seam: the length of every copy buffer this thread allocated since the last read.
#[cfg(test)]
pub(super) fn test_take_buffers() -> Vec<usize> {
    BUFFERS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

#[cfg(test)]
#[path = "copyread_tests.rs"]
mod tests;
