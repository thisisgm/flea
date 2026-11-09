// A written file counts only once its close answers and a fresh look by name shows all of it (S8-12).
use crate::error::FleaError;
use std::ffi::c_int;
use std::os::unix::io::IntoRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// The words for a file the destination never showed whole, beside durable::DIR_UNCONFIRMED.
pub const FILE_UNCONFIRMED: &str = "copied, but the drive did not confirm the file";
// gvfs uploads a WebDAV file after close answers, so a short landing is looked at again this often.
const RECHECK: Duration = Duration::from_millis(50);
// Every landing gets this long, whatever its size.
const LANDING_FLOOR: Duration = Duration::from_secs(10);
// Plus one second per MiB, the slowest upload a landing waits for.
const SLOWEST_BYTES_PER_SECOND: u64 = 1024 * 1024;

extern "C" {
    fn close(fd: c_int) -> c_int;
}

// Why a landed file does not count.
pub(crate) enum Unlanded {
    // close(2) or the fresh stat failed, with the error as given.
    Failed(std::io::Error),
    // The name holds something other than a regular file, or fewer bytes than were written.
    Short,
    Cancelled,
}

impl Unlanded {
    pub(crate) fn error(self, op: &str, path: &Path) -> FleaError {
        let path = path.to_string_lossy();
        match self {
            Unlanded::Failed(e) => crate::error::from_io(op, &path, &e),
            Unlanded::Short => FleaError {
                where_: op.to_string(),
                path: path.to_string(),
                msg: FILE_UNCONFIRMED.to_string(),
            },
            Unlanded::Cancelled => FleaError {
                where_: op.to_string(),
                path: path.to_string(),
                msg: "cancelled".to_string(),
            },
        }
    }
}

// close(2)'s own answer, where a FUSE mount reports a failed upload; dropping a File throws it away.
pub(crate) fn close_file(file: std::fs::File) -> std::io::Result<()> {
    close_with(file, fault())
}

// The same checked close for other threads, carrying this thread's test fault with it.
pub(crate) fn closer() -> impl Fn(std::fs::File) -> std::io::Result<()> + Send + Clone + 'static {
    let fault = fault();
    move |file| close_with(file, fault)
}

fn close_with(file: std::fs::File, fault: Option<i32>) -> std::io::Result<()> {
    // Linux frees the descriptor whatever close answers, so a failed close is never retried.
    let closed = unsafe { close(file.into_raw_fd()) };
    if let Some(errno) = fault {
        return Err(std::io::Error::from_raw_os_error(errno));
    }
    if closed != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// A regular file of `len` bytes at `at`, looked up by name each time; a FUSE upload still running is waited for.
pub(crate) fn await_landed(at: &Path, len: u64, cancel: &AtomicBool) -> Result<(), Unlanded> {
    let deadline = landing_deadline(len);
    loop {
        match fresh_len(at) {
            Ok(Some(have)) if have == len => return Ok(()),
            Ok(None) => return Err(Unlanded::Short),
            Ok(Some(_)) => {}
            Err(e) => return Err(Unlanded::Failed(e)),
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(Unlanded::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Unlanded::Short);
        }
        std::thread::sleep(RECHECK);
    }
}

// The moved item's kind and, for a file, its size, as the destination shows them now; anything else keeps the source.
pub(crate) fn same_as_source(src: &Path, dst: &Path) -> Result<(), Unlanded> {
    let source = src.symlink_metadata().map_err(Unlanded::Failed)?;
    let landed = dst.symlink_metadata().map_err(Unlanded::Failed)?;
    if source.file_type() != landed.file_type() {
        return Err(Unlanded::Short);
    }
    if source.is_file() && fresh_len(dst).map_err(Unlanded::Failed)? != Some(source.len()) {
        return Err(Unlanded::Short);
    }
    Ok(())
}

// None for anything but a regular file.
fn fresh_len(at: &Path) -> std::io::Result<Option<u64>> {
    let meta = at.symlink_metadata()?;
    if !meta.is_file() {
        return Ok(None);
    }
    #[cfg(test)]
    if let Some(short) = SHORT_LOOKS.with(|v| v.get()).filter(|left| *left > 0) {
        SHORT_LOOKS.with(|v| v.set(Some(short - 1)));
        return Ok(Some(meta.len().saturating_sub(1)));
    }
    Ok(Some(meta.len()))
}

#[cfg(test)]
thread_local! {
    static CLOSE_FAULT: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    static SHORT_LOOKS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static NO_WAIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Sample input: test_fail_closes(Some(5)) makes every close on this thread, and on closers it starts, close and answer EIO.
#[cfg(test)]
pub(crate) fn test_fail_closes(errno: Option<i32>) {
    CLOSE_FAULT.with(|v| v.set(errno));
}

// Sample input: test_short_looks(Some(2)) shows the next two fresh stats one byte short, as gvfs does mid-upload.
#[cfg(test)]
pub(crate) fn test_short_looks(looks: Option<usize>) {
    SHORT_LOOKS.with(|v| v.set(looks));
}

// A test of a landing that never completes ends at the first short look instead of the real deadline.
#[cfg(test)]
pub(crate) fn test_no_wait(on: bool) {
    NO_WAIT.with(|v| v.set(on));
}

#[cfg(test)]
fn fault() -> Option<i32> {
    CLOSE_FAULT.with(|v| v.get())
}

#[cfg(not(test))]
fn fault() -> Option<i32> {
    None
}

// The floor plus one second per MiB written, the slowest upload a landing waits for.
fn landing_deadline(len: u64) -> Instant {
    #[cfg(test)]
    if NO_WAIT.with(|v| v.get()) {
        return Instant::now();
    }
    Instant::now() + LANDING_FLOOR + Duration::from_secs(len / SLOWEST_BYTES_PER_SECOND)
}
