// Anonymous files retain complete Trash reviews without retaining every descendant in memory.
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(target_arch = "x86_64")]
const O_DIRECTORY: i32 = 0o200000;
#[cfg(target_arch = "aarch64")]
const O_DIRECTORY: i32 = 0o40000;
const O_TMPFILE: i32 = 0o20000000 | O_DIRECTORY;
const LENGTH_BYTES: u64 = 8;

// flock(2) opcodes; identical on x86-64 and aarch64 Linux.
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;

extern "C" {
    fn flock(fd: i32, op: i32) -> i32;
}

/// Blocking exclusive flock(2). Direct syscall rather than std's File::lock,
/// which was only stabilised in Rust 1.89 while this crate declares 1.77.
fn flock_lock(file: &File) -> Result<(), String> {
    // SAFETY: flock with a valid fd and a valid opcode has no memory effects.
    if unsafe { flock(file.as_raw_fd(), LOCK_EX) } == 0 {
        Ok(())
    } else {
        Err(format!(
            "Could not lock recovery record: {}",
            std::io::Error::last_os_error()
        ))
    }
}

/// Non-blocking exclusive flock. Ok(true) = locked, Ok(false) = held elsewhere.
fn flock_try_lock(file: &File) -> Result<bool, String> {
    // SAFETY: as above.
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        return Ok(true);
    }
    let err = std::io::Error::last_os_error();
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return Ok(false);
    }
    Err(format!("Could not lock recovery record: {}", err))
}

#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    generation: Arc<AtomicUsize>,
    expected: usize,
}
impl Cancellation {
    pub fn next(&self) -> Self {
        Self {
            generation: self.generation.clone(),
            expected: self.generation.fetch_add(1, Ordering::Relaxed) + 1,
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if self.generation.load(Ordering::Relaxed) == self.expected {
            Ok(())
        } else {
            Err("Trash request cancelled.".into())
        }
    }
}

#[derive(Debug)]
pub struct Manifest {
    file: Arc<File>,
    end: u64,
}
impl std::ops::Drop for Manifest {
    fn drop(&mut self) {
        // Explicitly release the flock lock before the fd closes. The kernel
        // does not sequence lock release after close(2) completes, so relying
        // on close alone leaves a race window under thread contention.
        //
        // Only unlock as the last strong reference; other Arc clones still
        // hold the lock.
        if Arc::strong_count(&self.file) == 1 {
            // SAFETY: as above; result intentionally ignored in Drop.
            unsafe {
                flock(self.file.as_raw_fd(), LOCK_UN);
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Records {
    file: Arc<File>,
    start: u64,
    end: u64,
}
impl Manifest {
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new().read(true).write(true).create_new(true).mode(0o600)
            .custom_flags(crate::oflags::O_NOFOLLOW).open(path)
            .map_err(|e| format!("Could not create recovery record {}: {}", path.display(), e))?;
        flock_lock(&file)?;
        Ok(Self { file: Arc::new(file), end: 0 })
    }
    pub fn open_inactive(path: &Path) -> Result<Option<Self>, String> {
        let file = OpenOptions::new().read(true).write(true).custom_flags(crate::oflags::O_NOFOLLOW).open(path)
            .map_err(|e| format!("Could not open recovery record {}: {}", path.display(), e))?;
        match flock_try_lock(&file) {
            Ok(true) => {}
            Ok(false) => return Ok(None),
            Err(error) => return Err(error),
        }
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() { return Err("Recovery record is not a regular file.".into()); }
        Ok(Some(Self { file: Arc::new(file), end: metadata.len() }))
    }
    pub fn file(&self) -> &File { &self.file }
    pub fn sync(&self) -> Result<(), String> {
        self.file.sync_all().map_err(|e| format!("Could not sync recovery record: {}", e))
    }
    pub fn new(root: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(O_TMPFILE)
            .open(root)
            .map_err(|e| format!("Could not create anonymous Trash review in {}: {}", root.display(), e))?;
        Ok(Self { file: Arc::new(file), end: 0 })
    }
    pub fn len(&self) -> u64 { self.end }
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), String> {
        let length = bytes.len() as u64;
        let end = self.end.checked_add(length).and_then(|end| end.checked_add(2 * LENGTH_BYTES))
            .ok_or("Trash review is too large for its backing file.")?;
        self.file.write_all_at(&length.to_le_bytes(), self.end)
            .and_then(|_| self.file.write_all_at(bytes, self.end + LENGTH_BYTES))
            .and_then(|_| self.file.write_all_at(&length.to_le_bytes(), end - LENGTH_BYTES))
            .map_err(|e| format!("Could not write Trash review: {}", e))?;
        self.end = end;
        Ok(())
    }
    pub fn records(&self) -> Records {
        Records { file: self.file.clone(), start: 0, end: self.end }
    }
    pub fn range(&self, start: u64, end: u64) -> Result<Records, String> {
        if start > end || end > self.end { return Err("Invalid Trash review range.".into()); }
        Ok(Records { file: self.file.clone(), start, end })
    }
}
impl Records {
    pub fn start(&self) -> u64 { self.start }
    pub fn end(&self) -> u64 { self.end }
    fn length(&self, offset: u64) -> Result<u64, String> {
        let mut bytes = [0; LENGTH_BYTES as usize];
        self.file.read_exact_at(&mut bytes, offset)
            .map_err(|e| format!("Could not read Trash review: {}", e))?;
        Ok(u64::from_le_bytes(bytes))
    }
    pub fn next(&self, offset: &mut u64) -> Result<Option<Vec<u8>>, String> {
        if *offset == self.end { return Ok(None); }
        if *offset < self.start || self.end.saturating_sub(*offset) < 2 * LENGTH_BYTES {
            return Err("Truncated Trash review record.".into());
        }
        let length = self.length(*offset)?;
        let available = self.end - *offset - 2 * LENGTH_BYTES;
        if length > available || length > usize::MAX as u64 { return Err("Invalid Trash review length.".into()); }
        let mut bytes = vec![0; length as usize];
        self.file.read_exact_at(&mut bytes, *offset + LENGTH_BYTES)
            .map_err(|e| format!("Could not read Trash review: {}", e))?;
        let end = *offset + length + 2 * LENGTH_BYTES;
        if self.length(end - LENGTH_BYTES)? != length { return Err("Damaged Trash review record.".into()); }
        *offset = end;
        Ok(Some(bytes))
    }
    pub fn previous(&self, offset: &mut u64) -> Result<Option<Vec<u8>>, String> {
        if *offset == self.start { return Ok(None); }
        if *offset > self.end || offset.saturating_sub(self.start) < 2 * LENGTH_BYTES {
            return Err("Truncated Trash review record.".into());
        }
        let length = self.length(*offset - LENGTH_BYTES)?;
        if length > *offset - self.start - 2 * LENGTH_BYTES { return Err("Invalid Trash review length.".into()); }
        let start = *offset - length - 2 * LENGTH_BYTES;
        let mut forward = start;
        let bytes = self.next(&mut forward)?;
        *offset = start;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    #[test]
    fn disk_queue_can_grow_and_read_both_directions_without_named_files() {
        let d = TestDir::new("trash-manifest");
        let mut manifest = Manifest::new(d.path()).unwrap();
        manifest.append(b"first").unwrap();
        let mut offset = 0;
        assert_eq!(manifest.records().next(&mut offset).unwrap().unwrap(), b"first");
        manifest.append(b"second").unwrap();
        assert_eq!(manifest.records().next(&mut offset).unwrap().unwrap(), b"second");
        assert!(manifest.records().next(&mut offset).unwrap().is_none());
        assert_eq!(manifest.records().previous(&mut offset).unwrap().unwrap(), b"second");
        assert_eq!(manifest.records().previous(&mut offset).unwrap().unwrap(), b"first");
        assert!(manifest.records().previous(&mut offset).unwrap().is_none());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }
    #[test]
    fn superseding_request_cancels_prior_work() {
        let cancellation = Cancellation::default();
        let first = cancellation.next();
        assert!(first.check().is_ok());
        let second = cancellation.next();
        assert!(first.check().is_err());
        assert!(second.check().is_ok());
    }
}
