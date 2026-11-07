// Tree-copy creation manifest on the runtime filesystem, never on the destination, never in the journal entry.
use crate::backend::trashmanifest::{Manifest, Records};
use crate::error::FleaError;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

// Twice the largest tree this product is benched against; past it the copy still runs, but undo falls back to the whole-tree check rather than recording more.
const MAX_ENTRIES: usize = 200_000;
// About 32 bytes of header plus short names times the entry cap, rounded up.
const MAX_BYTES: u64 = 32 * 1024 * 1024;
// dev(8) + ino(8) + kind(4) + len(8) + mtime sec(8) + mtime nsec(8).
const HEADER: usize = 44;
// The two length words Manifest::append writes around each record.
const FRAMING: u64 = 16;

// Buffered while the copy runs and appended every 64 KiB, so a 100,000-file tree never sits in memory whole; dropped unread when the copy succeeds.
pub struct Writer {
    inner: Manifest,
    root: PathBuf,
    count: usize,
    overflow: bool,
    failed: Option<String>,
    buf: Vec<u8>,
    ends: Vec<u32>,
}

// Records reach the anonymous file in 64 KiB batches as the copy runs, so memory stays bounded whatever the tree holds.
const FLUSH_AT: usize = 65536;

// The journal step holds this instead of the records: an fd to the same anonymous file, so dropping the step closes the last holder and the manifest goes with it.
#[derive(Clone, Debug)]
pub struct Handle {
    records: Records,
    root: PathBuf,
    count: usize,
}

// PartialEq is journal identity (which copy this belongs to), never proof that the filesystem still holds those paths; the per-record checks at undo time are that.
impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.count == other.count
    }
}

impl Handle {
    #[cfg(test)]
    pub fn fd_raw(&self) -> i32 {
        self.records.file_raw()
    }
}

pub struct Kept {
    pub path: PathBuf,
    pub reason: &'static str,
}

pub struct Report {
    pub total: usize,
    pub removed: usize,
    pub kept: Vec<Kept>,
}

// Fallback means the stream never verified a record, so the caller runs today's whole-tree check.

pub enum Outcome {
    Done(Report),
    Fallback,
}

// A tree copy records its destination; anything else keeps today's single check.
pub fn writer_for(src: &Path, dst: &Path) -> Option<Writer> {
    let meta = src.symlink_metadata().ok()?;
    if meta.file_type().is_symlink() || !meta.is_dir() || !dst.is_absolute() {
        return None;
    }
    match Writer::create_for(src, dst) {
        // No off-copy filesystem still copies, but never silently: undo falls back to the whole-tree check.
        Err(e) => {
            eprintln!("flea: copy manifest unavailable for {}: {e}", dst.display());
            None
        }
        Ok(writer) => Some(writer),
    }
}

// A move copies only across filesystems, but EXDEV through a symlinked parent defeats a device check, so every directory move is manifested; a rename drops it unread.
pub fn writer_for_move(src: &Path, dst: &Path) -> Option<Writer> {
    writer_for(src, dst)
}

// Finish a failed copy loud: Ok manifest or None, plus a loud error when the append itself failed.
pub fn finish_loud(writer: Option<Writer>) -> (Option<Handle>, Option<String>) {
    match writer {
        Some(writer) => match writer.finish() {
            Ok(manifest) => (manifest, None),
            Err(e) => (None, Some(e)),
        },
        None => (None, None),
    }
}

// Tests only: a handle whose stream verifies nothing, so remove_owned answers Fallback.
#[cfg(test)]
pub(crate) fn test_unverified(root: &std::path::Path) -> Handle {
    let mut inner = crate::backend::trashmanifest::Manifest::new_on_filesystem(root).expect("anonymous manifest");
    inner.append(b"framing without a record inside").expect("one damaged frame");
    Handle { records: inner.records(), root: root.to_path_buf(), count: 1 }
}

impl Writer {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn create_for(src: &Path, root: &Path) -> Result<Self, String> {
        use crate::backend::manifestdir::{candidate_dirs, current_uid, forbid_for, qualifying_dirs};
        if !root.is_absolute() {
            return Err("a manifest root must be absolute".into());
        }
        let uid = current_uid();
        let forbid = forbid_for(src, root);
        let mut last = "no runtime directory for the copy manifest lives off the copy's own filesystems".to_string();
        for dir in qualifying_dirs(&candidate_dirs(uid), &forbid, uid) {
            // Retry qualifying off-copy filesystems without moving the named fallback onto the copy.
            match Manifest::new_on_filesystem(&dir) {
                Ok(inner) => {
                    return Ok(Self { inner, root: root.to_path_buf(), count: 0, overflow: false, failed: None, buf: Vec::new(), ends: Vec::new() })
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    // Tests record under their own sandbox, which names both ends; production names the real copy.
    #[cfg(test)]
    fn create(root: &Path) -> Result<Self, String> {
        Self::create_for(root, root)
    }

    // Identity arrives with the path from the caller's own descriptor (fstat, no lookup), so a rename between create and record cannot substitute another file.
    pub fn record(&mut self, named: &Path, meta: &std::fs::Metadata) {
        if self.failed.is_some() || self.overflow || self.count >= MAX_ENTRIES {
            self.overflow = true;
            return;
        }
        let rel = match named.strip_prefix(&self.root) {
            Ok(rel) => rel,
            Err(_) => {
                self.overflow = true;
                return;
            }
        };
        let rel = rel.as_os_str().as_bytes();
        // Encoded straight into the batch buffer, so a warm batch allocates nothing of its own.
        let need = HEADER + rel.len();
        // The buffer holds unframed records, so every buffered record's framing counts, not only the new one's.
        let framed = (self.ends.len() as u64 + 1) * FRAMING;
        if self.inner.len() + self.buf.len() as u64 + need as u64 + framed > MAX_BYTES {
            self.overflow = true;
            return;
        }
        encode_into(&mut self.buf, meta, rel);
        self.ends.push(self.buf.len() as u32);
        self.count += 1;
        if self.buf.len() >= FLUSH_AT {
            self.flush();
        }
    }

    // No descriptor to fstat for symlinks and nodes, so the at-path pins every parent and only the final name resolves, the way copyfile.rs holds them.
    pub fn record_stat(&mut self, at: &Path, named: &Path) {
        if self.failed.is_some() {
            return;
        }
        match at.symlink_metadata() {
            Ok(meta) => self.record(named, &meta),
            Err(_) => {
                self.overflow = true;
            }
        }
    }

    // A stat this copy could not take records nothing: finish answers None and undo falls back to the whole-tree check.
    pub fn overflow(&mut self) {
        self.overflow = true;
    }

    #[cfg(test)]
    pub fn failed(&self) -> bool {
        self.failed.is_some()
    }

    // Every buffered record through Manifest in one write, so the framing lives in one place; a failed batch latches and finish reports it loud.
    fn flush(&mut self) {
        if self.failed.is_some() {
            self.buf.clear();
            self.ends.clear();
            return;
        }
        // Cleared rather than taken, so the next batch reuses both allocations.
        let mut start = 0usize;
        let mut records: Vec<&[u8]> = Vec::with_capacity(self.ends.len());
        for end in self.ends.iter().copied() {
            records.push(&self.buf[start..end as usize]);
            start = end as usize;
        }
        if let Err(e) = self.inner.append_all_labelled("copy manifest", &records) {
            self.failed = Some(e);
        }
        self.buf.clear();
        self.ends.clear();
    }

    // Finish never stats the destination: every identity was captured at create, so a failed or cancelled copy answers at once.
    pub fn finish(mut self) -> Result<Option<Handle>, String> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        if self.overflow || self.count == 0 {
            return Ok(None);
        }
        self.flush();
        if let Some(e) = self.failed.take() {
            return Err(e);
        }
        Ok(Some(Handle { records: self.inner.records(), root: self.root, count: self.count }))
    }
}

#[cfg(test)]
fn encode(meta: &std::fs::Metadata, rel: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + rel.len());
    encode_into(&mut out, meta, rel);
    out
}

// The record bytes appended to a caller-held buffer, so the hot path encodes with no per-record Vec.
fn encode_into(out: &mut Vec<u8>, meta: &std::fs::Metadata, rel: &[u8]) {
    out.extend_from_slice(&meta.dev().to_le_bytes());
    out.extend_from_slice(&meta.ino().to_le_bytes());
    out.extend_from_slice(&(meta.mode() & 0o170000).to_le_bytes());
    out.extend_from_slice(&meta.len().to_le_bytes());
    out.extend_from_slice(&meta.mtime().to_le_bytes());
    out.extend_from_slice(&meta.mtime_nsec().to_le_bytes());
    out.extend_from_slice(rel);
}

struct Decoded {
    dev: u64,
    ino: u64,
    kind: u32,
    len: u64,
    mtime: (i64, i64),
    rel: PathBuf,
}

fn decode(bytes: &[u8]) -> Option<Decoded> {
    if bytes.len() < HEADER {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<[u8; 8]> { bytes.get(range)?.try_into().ok() };
    let rel = std::ffi::OsStr::from_bytes(bytes.get(HEADER..)?);
    Some(Decoded {
        dev: u64::from_le_bytes(num(0..8)?),
        ino: u64::from_le_bytes(num(8..16)?),
        kind: u32::from_le_bytes(bytes.get(16..20)?.try_into().ok()?),
        len: u64::from_le_bytes(num(20..28)?),
        mtime: (i64::from_le_bytes(num(28..36)?), i64::from_le_bytes(num(36..44)?)),
        rel: PathBuf::from(rel),
    })
}

// A manifest path is always under its root; anything else is kept, never followed.
fn contained(root: &Path, rel: &Path) -> Option<PathBuf> {
    if rel.is_absolute() || rel.components().any(|part| part == Component::ParentDir) {
        return None;
    }
    let abs = root.join(rel);
    if abs.starts_with(root) {
        Some(abs)
    } else {
        None
    }
}

// Reverse creation order is deepest first; Records::previous streams that way without holding the manifest whole.

pub fn remove_owned(handle: &Handle) -> Outcome {
    let mut report = Report { total: handle.count, removed: 0, kept: Vec::new() };
    let mut offset = handle.records.end();
    let mut processed = 0usize;
    loop {
        let bytes = match handle.records.previous(&mut offset) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break,
            Err(_) => return unfinished(handle, report, processed),
        };
        processed += 1;
        let Some(record) = decode(&bytes) else { return unfinished(handle, report, processed - 1) };
        remove_one(handle, &record, &mut report);
    }
    Outcome::Done(report)
}

// A damaged stream after verified deletions never widens into a whole-tree removal.

fn unfinished(handle: &Handle, mut report: Report, processed: usize) -> Outcome {
    if processed == 0 {
        return Outcome::Fallback;
    }
    report.kept.push(Kept { path: handle.root.clone(), reason: "the record of this copy could not be read" });
    Outcome::Done(report)
}

fn remove_one(handle: &Handle, record: &Decoded, report: &mut Report) {
    // ENOTEMPTY from Linux errno.h: ErrorKind::DirectoryNotEmpty needs Rust 1.83 over the 1.77 floor.
    const ENOTEMPTY: i32 = 39;
    let Some(abs) = contained(&handle.root, &record.rel) else {
        report.kept.push(Kept { path: handle.root.join(&record.rel), reason: "it was replaced after the copy" });
        return;
    };
    let meta = match abs.symlink_metadata() {
        Ok(meta) => meta,
        // Gone already is the state undo wanted; it is neither removed nor kept.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => {
            report.kept.push(Kept { path: abs, reason: "it could not be removed" });
            return;
        }
    };
    if meta.dev() != record.dev || meta.ino() != record.ino || meta.mode() & 0o170000 != record.kind {
        report.kept.push(Kept { path: abs, reason: "it was replaced after the copy" });
        return;
    }
    if meta.is_file() && (meta.len() != record.len || (meta.mtime(), meta.mtime_nsec()) != record.mtime) {
        report.kept.push(Kept { path: abs, reason: "it was modified after the copy" });
        return;
    }
    if meta.is_dir() && !meta.file_type().is_symlink() {
        match std::fs::remove_dir(&abs) {
            Ok(()) => report.removed += 1,
            Err(e) if e.raw_os_error() == Some(ENOTEMPTY) => {
                report.kept.push(Kept { path: abs, reason: "it is not empty" })
            }
            Err(e) => report.kept.push(Kept {
                path: abs,
                reason: if e.kind() == std::io::ErrorKind::NotFound { return } else { "it could not be removed" },
            }),
        }
        return;
    }
    // Unlink, never through the link: a symlink the copy made goes without touching its target.
    match std::fs::remove_file(&abs) {
        Ok(()) => report.removed += 1,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => report.kept.push(Kept { path: abs, reason: "it could not be removed" }),
    }
}

// Whole sentences for the existing undo reply shape: how many went, then the first kept item and why; the log names every kept item.
pub fn summarize(report: &Report) -> String {
    let (removed, total) = (report.removed, report.total);
    let Some(first) = report.kept.first() else {
        return if total == 1 {
            format!("Undo removed {removed} of 1 copied item.")
        } else {
            format!("Undo removed {removed} of {total} copied items.")
        };
    };
    for kept in &report.kept {
        eprintln!("flea: undo kept {}: {}", kept.path.display(), kept.reason);
    }
    let (path, reason) = (first.path.display(), first.reason);
    match (total == 1, report.kept.len()) {
        (true, 1) => format!("Undo removed {removed} of 1 copied item. {path} was kept: {reason}."),
        (false, 1) => format!("Undo removed {removed} of {total} copied items. {path} was kept: {reason}."),
        (true, many) => format!("Undo removed {removed} of 1 copied item. {many} were kept, first {path}: {reason}."),
        (false, many) => format!("Undo removed {removed} of {total} copied items. {many} were kept, first {path}: {reason}."),
    }
}

pub fn undo_err(to: &Path, msg: String) -> FleaError {
    FleaError { where_: "undo".into(), path: to.to_string_lossy().into(), msg }
}

#[cfg(test)]
#[path = "copymanifest_tests.rs"]
mod tests;
