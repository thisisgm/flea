// Test-only: the fifo scaffolding every hang test needs, so no test builds a pipe, a writer or a
// bound by hand. It lives here because three modules refuse to open a fifo and each has to prove it,
// and because a test that forgets to wait for the writer silently measures the case above it.
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

// Long enough that a loaded box does not fail a working call, short enough that the suite fails rather than hanging.
pub const BOUND: Duration = Duration::from_secs(5);
// How often the writer's signal file is looked for, which costs nothing next to spawning the shell.
const POLL: Duration = Duration::from_millis(2);

pub fn mkfifo(path: &Path) {
    let made = Command::new("mkfifo").arg(path).status().expect("mkfifo");
    assert!(made.success(), "mkfifo {:?}", path);
}

// Runs one call on a thread with a bound, because the defect under test is an open that never returns.
pub fn within<T: Send + 'static>(what: &str, call: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(call());
    });
    match rx.recv_timeout(BOUND) {
        Ok(v) => v,
        Err(_) => panic!("{} must answer within its bound", what),
    }
}

// The library tests include this helper without the binary's watcher modules.
const IN_CLOEXEC: i32 = 0x0008_0000;
const IN_CLOSE_WRITE: u32 = 0x0000_0008;
const IN_Q_OVERFLOW: u32 = 0x0000_4000;
const FIFO_EVENT_BYTES: usize = 16;

pub enum Access {
    Read,
    Write,
}

// A writer probe stays open until the call answers, so a writer cannot slip between EOF and close.
struct FifoProbe<'a> {
    fifo: &'a Path,
    reader: Option<std::fs::File>,
    writes: Option<std::fs::File>,
}

impl<'a> FifoProbe<'a> {
    fn new(fifo: &'a Path, access: Access) -> Self {
        let reader = match access {
            Access::Read => None,
            Access::Write => Some(Self::open(fifo, false).expect("the old fifo to monitor")),
        };
        let writes = reader.as_ref().map(|_| Self::watch_writes(fifo));
        Self {
            fifo,
            reader,
            writes,
        }
    }

    fn open(fifo: &Path, write: bool) -> Option<std::fs::File> {
        std::fs::OpenOptions::new()
            .read(!write)
            .write(write)
            .custom_flags(crate::oflags::O_NONBLOCK | crate::oflags::O_NOFOLLOW)
            .open(fifo)
            .ok()
            .filter(|f| f.metadata().is_ok_and(|m| m.file_type().is_fifo()))
    }

    fn watch_writes(fifo: &Path) -> std::fs::File {
        use std::os::fd::{AsRawFd, FromRawFd};
        extern "C" {
            fn inotify_init1(flags: i32) -> i32;
            fn inotify_add_watch(fd: i32, path: *const std::ffi::c_char, mask: u32) -> i32;
        }
        // SAFETY: the flags request a new nonblocking descriptor that this File owns.
        let fd = unsafe { inotify_init1(IN_CLOEXEC | crate::oflags::O_NONBLOCK) };
        assert!(
            fd >= 0,
            "fifo probe inotify: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: inotify_init1 returned a fresh descriptor, transferred exclusively to this File.
        let events = unsafe { std::fs::File::from_raw_fd(fd) };
        let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).expect("fifo path");
        // SAFETY: the owned descriptor and NUL-terminated path stay live for this call.
        let wd = unsafe { inotify_add_watch(events.as_raw_fd(), path.as_ptr(), IN_CLOSE_WRITE) };
        assert!(
            wd >= 0,
            "fifo probe watch {}: {}",
            fifo.display(),
            std::io::Error::last_os_error()
        );
        events
    }

    fn writer_closed(&mut self) -> bool {
        let mut buf = [0u8; FIFO_EVENT_BYTES];
        // Sample FIFO event: wd=1, mask=0x8, cookie=0, len=0, exactly 16 bytes with no filename.
        loop {
            match self.writes.as_mut().unwrap().read(&mut buf) {
                Ok(n) => {
                    assert_eq!(n, FIFO_EVENT_BYTES, "fifo probe returned a partial event");
                    let mask = u32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]);
                    assert_eq!(mask & IN_Q_OVERFLOW, 0, "fifo probe events overflowed");
                    if mask & IN_CLOSE_WRITE != 0 {
                        return true;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return false,
                Err(e) => panic!("fifo probe events {}: {e}", self.fifo.display()),
            }
        }
    }

    fn held(&mut self) -> bool {
        if let Some(reader) = &mut self.reader {
            let mut byte = [0u8; 1];
            let writing = match reader.read(&mut byte) {
                Ok(n) => n > 0,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => true,
                Err(e) => panic!("fifo probe read {}: {e}", self.fifo.display()),
            };
            writing || self.writer_closed()
        } else {
            Self::open(self.fifo, true).is_some()
        }
    }
}

// A failing FIFO check may leave its callback blocked, so it runs in a disposable test process.
pub fn isolated(test: &str) -> bool {
    const CHILD: &str = "FLEA_FIFO_TEST_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(test) {
        return false;
    }
    let status = Command::new(std::env::current_exe().expect("the test executable"))
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, test)
        .status()
        .expect("the isolated FIFO test");
    assert!(
        status.success(),
        "isolated FIFO test {test} failed: {status}"
    );
    true
}

// The call waits on its answer or forbidden FIFO access, never for a result after failure is known.
pub fn answers_or_holds<T: Send + 'static>(
    fifo: &Path,
    access: Access,
    call: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let mut probe = FifoProbe::new(fifo, access);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(call());
    });
    loop {
        match rx.recv_timeout(POLL) {
            Ok(v) => return if probe.held() { None } else { Some(v) },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if probe.held() => return None,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("the call panicked"),
        }
    }
}

#[test]
fn forbidden_fifo_access_answers_before_a_callback_that_keeps_its_writer() {
    let root = super::testdir::TestDir::new("fifo-held-callback");
    let fifo = root.join("pipe");
    mkfifo(&fifo);
    let worker_fifo = fifo.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let (done, finished) = std::sync::mpsc::channel();
    let (answered, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = answers_or_holds(&fifo, Access::Write, move || {
            let held = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(worker_fifo)
                .unwrap();
            let _ = wait.recv();
            drop(held);
            let _ = done.send(());
        });
        let _ = answered.send(result);
    });
    let result = answer.recv_timeout(BOUND);
    // Release the synthetic callback before asserting, even when the old helper never answered.
    let _ = release.send(());
    finished
        .recv_timeout(BOUND)
        .expect("release the synthetic FIFO callback");
    if result.is_err() {
        let _ = answer.recv_timeout(BOUND);
    }
    assert!(
        matches!(result, Ok(None)),
        "known FIFO failure must not await the blocked callback"
    );
}

#[test]
fn fifo_writer_probe_keeps_observing_after_eof_until_the_call_answers() {
    for body in ["", "an old source record"] {
        let root = super::testdir::TestDir::new("fifo-writer-probe");
        let fifo = root.join("pipe");
        mkfifo(&fifo);
        let mut probe = FifoProbe::new(&fifo, Access::Write);
        assert!(!probe.held(), "no writer yet, so the first read is EOF");
        // The forbidden writer runs after EOF was checked, then closes before the next observation.
        let writer_fifo = fifo.clone();
        std::thread::spawn(move || {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(crate::oflags::O_NONBLOCK)
                .open(writer_fifo)
                .unwrap()
                .write_all(body.as_bytes())
                .unwrap();
        })
        .join()
        .unwrap();
        assert!(
            probe.held(),
            "a writer after EOF must never be classified safe"
        );
    }
}

// Read on a thread too, because a caller that wrongly drained the pipe would leave this blocked.
pub fn peek(path: PathBuf) -> Vec<u8> {
    within("a peek at a fifo", move || {
        let mut buf = vec![0u8; 64];
        let n = std::fs::File::open(&path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
        buf.truncate(n);
        buf
    })
}

// A Child is not killed by dropping it, so this is what stops a failed assertion leaving a writer behind.
pub struct FifoWriter(Child);

impl FifoWriter {
    // Answers only once the bytes are in the pipe, so no caller can measure the writerless case by mistake.
    pub fn feeding(fifo: &Path, body: &str, signal: &Path) -> FifoWriter {
        // exec 3<> opens both ends at once, the only way a writer sits on a fifo nobody is reading.
        let child = Command::new("sh")
            .arg("-c")
            .arg(r#"exec 3<>"$0"; printf %s "$1" >&3; : >"$2"; exec sleep 30"#)
            .arg(fifo)
            .arg(body)
            .arg(signal)
            .spawn()
            .expect("fifo writer");
        let writer = FifoWriter(child);
        await_written(signal);
        writer
    }

    // The pid this test started is the one it kills, and the caller asserts that both landed.
    pub fn stop(&mut self) -> bool {
        self.0.kill().is_ok() && self.0.wait().is_ok()
    }
}

impl Drop for FifoWriter {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// The writer creates the signal only once its printf has returned, so nothing measures a half-filled pipe.
fn await_written(signal: &Path) {
    let deadline = Instant::now() + BOUND;
    while !signal.exists() {
        assert!(Instant::now() < deadline, "the fifo writer never signalled that its write finished");
        std::thread::sleep(POLL);
    }
}
