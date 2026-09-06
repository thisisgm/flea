// Test-only: the fifo scaffolding every hang test needs, so no test builds a pipe, a writer or a
// bound by hand. It lives here because three modules refuse to open a fifo and each has to prove it,
// and because a test that forgets to wait for the writer silently measures the case above it.
use std::io::Read;
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
