// Anonymous files retain complete Trash reviews without retaining every descendant in memory.
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

mod anonymous;
const LENGTH_BYTES: u64 = 8;

#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    generation: Arc<AtomicUsize>,
    expected: usize,
    flag: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl Cancellation {
    pub fn operation(flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { flag: Some(flag), ..Self::default() }
    }
    pub fn next(&self) -> Self {
        Self {
            generation: self.generation.clone(),
            expected: self.generation.fetch_add(1, Ordering::Relaxed) + 1,
            flag: self.flag.clone(),
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if self.generation.load(Ordering::Relaxed) == self.expected
            && !self.flag.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            Ok(())
        } else {
            Err("Trash request cancelled.".into())
        }
    }
}

// What a person is told about a Trash delete, restore or recovery: one sentence per outcome, never a failure's own text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op { Delete, Restore }

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    // A check before anything moved found the Trash unlike its review, or a review record unreadable.
    Changed,
    ConfirmAgain,
    NothingDeleted,
    // No node was deleted; after a claim this outcome survives only a successful rollback.
    Cancelled,
    ItemChanged,
    Damaged,
    Kept(PathBuf),
    Prepare(Op),
    NoRecord(Op),
    Stuck(Op, PathBuf),
    StuckInTrash(Op, PathBuf),
    StuckAtOriginal(PathBuf),
    Partial(Op),
    PartialAtOriginal,
    InfoLeft(Op, PathBuf),
    FolderLeft(Op, Option<PathBuf>),
    RecordLeft(Option<Op>),
    RecordUnconfirmed(Option<Op>),
    ChildChanged,
    ChildLeft,
    Blocked,
    OriginalChanged,
    Held(PathBuf),
    RecoveryUnreadable(PathBuf),
    NothingMoved,
    // The Trash listing could not be read, or its answer could not be made sense of.
    Unreadable,
    TookTooLong,
    // A selection no longer matches the Trash: it changed, expired, or arrived without the identity that reviews it.
    SelectionChanged,
    // The Trash reports items that cannot be told apart. True when the ask was to select them, false once an action began.
    Indistinct(bool),
    // The Trash could not be checked against its backing store.
    Unchecked(Op),
    // A sentence another copy row owns, passed through whole.
    Text(String),
}

// The detail goes to stderr, and in a test also to a list the test can read, since libtest drops stderr.
pub fn log(line: &str) {
    eprintln!("flea: {line}");
    #[cfg(test)]
    LOGGED.with(|lines| lines.borrow_mut().push(line.to_string()));
}
#[cfg(test)]
thread_local! {
    static LOGGED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
pub fn logged() -> Vec<String> {
    LOGGED.with(|lines| lines.borrow().clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Refusal { outcome: Outcome, detail: String, reason: String }
impl Refusal {
    pub fn new(outcome: Outcome, detail: impl Into<String>) -> Self {
        Self { outcome, detail: detail.into(), reason: String::new() }
    }
    pub fn changed(detail: impl Into<String>) -> Self { Self::new(Outcome::Changed, detail) }
    // An OS failure: the detail keeps the raw error, the sentence gets io_message's plain clause.
    pub fn os(outcome: Outcome, what: &str, error: &std::io::Error) -> Self {
        Self { outcome, detail: format!("{what}: {error}"), reason: crate::error::io_message(error) }
    }
    pub fn said(text: String) -> Self { Self { detail: text.clone(), outcome: Outcome::Text(text), reason: String::new() } }
    pub fn cancelled(text: String) -> Self { Self::new(Outcome::Cancelled, text) }
    pub fn because(mut self, reason: impl Into<String>) -> Self { self.reason = reason.into(); self }
    // The same failure under the outcome of the step that met it, its cause carried into that sentence.
    pub fn wrap(&self, outcome: Outcome) -> Self { Self { outcome, detail: self.detail.clone(), reason: self.cause() } }
    pub fn outcome(&self) -> &Outcome { &self.outcome }
    pub fn detail(&self) -> &str { &self.detail }
    pub fn logged(self) -> Self {
        log(&format!("Trash {:?}: {}", self.outcome, self.detail));
        self
    }
    fn cause(&self) -> String {
        if !self.reason.is_empty() { return self.reason.clone(); }
        match &self.outcome {
            Outcome::ChildChanged => "an item in the Trash changed".into(),
            Outcome::ItemChanged => "the item changed".into(),
            Outcome::Damaged => "the Trash record is damaged".into(),
            Outcome::Text(text) => text.trim_end_matches('.').to_string(),
            Outcome::Cancelled => self.detail.trim_end_matches('.').to_string(),
            _ => "the contents changed".into(),
        }
    }
    pub fn shown(&self) -> String {
        use Op::{Delete, Restore};
        let cause = self.cause();
        let why = cause.trim_end_matches('.');
        match &self.outcome {
            Outcome::Changed => "The Trash changed, so nothing was done. Try again.".into(),
            Outcome::ConfirmAgain => "The Trash changed, so nothing was done; confirm again.".into(),
            Outcome::NothingDeleted => "The Trash changed, so nothing was deleted.".into(),
            Outcome::Cancelled => self.detail.clone(),
            Outcome::ItemChanged => "The item changed after you confirmed, so nothing was deleted.".into(),
            Outcome::Damaged => "The Trash record for this item is damaged, so nothing was restored.".into(),
            Outcome::Kept(at) => format!("The Trash changed, so nothing was done. Kept items are in {}.", at.display()),
            Outcome::Prepare(Delete) => format!("Could not prepare the deletion, so nothing was deleted: {why}."),
            Outcome::Prepare(Restore) => format!("Could not prepare the restore, so nothing was restored: {why}."),
            Outcome::NoRecord(Delete) => format!("Flea could not keep a record of this deletion, so nothing was deleted: {why}."),
            Outcome::NoRecord(Restore) => format!("Flea could not keep a record of this restore, so nothing was restored: {why}."),
            Outcome::Stuck(Delete, at) => format!("The deletion could not finish: {why}. The remaining items are kept at {}.", at.display()),
            Outcome::Stuck(Restore, at) => format!("The restore could not finish: {why}. The remaining items are kept at {}.", at.display()),
            Outcome::StuckInTrash(Delete, at) => format!("The deletion could not finish: {why}. The remaining items are kept in the Trash and at {}.", at.display()),
            Outcome::StuckInTrash(Restore, at) => format!("The restore could not finish: {why}. The remaining items are kept in the Trash and at {}.", at.display()),
            Outcome::StuckAtOriginal(at) => format!("The deletion could not finish: {why}. The remaining items are kept where they were and at {}.", at.display()),
            Outcome::Partial(Delete) => format!("The item could not be fully deleted: {why}. What is left stays in the Trash, and a folder may be partly deleted."),
            Outcome::Partial(Restore) => format!("The item could not be restored: {why}. It stays in the Trash."),
            Outcome::PartialAtOriginal => format!("The item could not be fully deleted: {why}. What is left stays where it was, and a folder may be partly deleted."),
            Outcome::InfoLeft(Delete, at) => format!("The item was deleted, but a Trash record at {} could not be removed: {why}.", at.display()),
            Outcome::InfoLeft(Restore, at) => format!("The item was restored, but a Trash record at {} could not be removed: {why}.", at.display()),
            Outcome::FolderLeft(Delete, Some(at)) => format!("The item was deleted, but Flea could not remove its temporary folder at {}: {why}.", at.display()),
            Outcome::FolderLeft(Restore, Some(at)) => format!("The item was restored, but Flea could not remove its temporary folder at {}: {why}.", at.display()),
            Outcome::FolderLeft(Delete, None) => format!("The item was deleted, but Flea could not remove its temporary folder: {why}."),
            Outcome::FolderLeft(Restore, None) => format!("The item was restored, but Flea could not remove its temporary folder: {why}."),
            Outcome::RecordLeft(Some(Delete)) => format!("The item was deleted, but Flea could not remove its recovery record: {why}."),
            Outcome::RecordLeft(Some(Restore)) => format!("The item was restored, but Flea could not remove its recovery record: {why}."),
            Outcome::RecordLeft(None) => format!("The interrupted operation was recovered, but Flea could not remove its recovery record: {why}."),
            Outcome::RecordUnconfirmed(Some(Delete)) => format!("The item was deleted, but after removing its recovery record, Flea could not confirm the removal on disk: {why}."),
            Outcome::RecordUnconfirmed(Some(Restore)) => format!("The item was restored, but after removing its recovery record, Flea could not confirm the removal on disk: {why}."),
            Outcome::RecordUnconfirmed(None) => format!("The interrupted operation was recovered, but after removing its recovery record, Flea could not confirm the removal on disk: {why}."),
            Outcome::ChildChanged => "An item in the Trash changed, so the rest was kept.".into(),
            Outcome::ChildLeft => format!("Some items in the Trash folder could not be deleted: {why}."),
            Outcome::Blocked => format!("The item could not be restored: {why}."),
            Outcome::OriginalChanged => "The item's original folder changed; select it again.".into(),
            Outcome::Held(at) => format!("Flea found an interrupted Trash operation; its items are kept at {}.", at.display()),
            Outcome::RecoveryUnreadable(at) => format!("Flea could not read the recovery folder at {}: {why}; records not yet read were left in place.", at.display()),
            Outcome::NothingMoved => "Flea found an interrupted Trash operation; nothing was moved.".into(),
            Outcome::Unreadable => "The Trash could not be read.".into(),
            Outcome::TookTooLong => "The Trash took too long to answer.".into(),
            Outcome::SelectionChanged => "The Trash selection changed; select the items again.".into(),
            Outcome::Indistinct(true) => "Items in this Trash cannot be told apart, so they cannot be selected.".into(),
            Outcome::Indistinct(false) => "Items in this Trash cannot be told apart, so nothing was done.".into(),
            Outcome::Unchecked(Delete) => "This Trash cannot be checked, so nothing can be deleted from it.".into(),
            Outcome::Unchecked(Restore) => "This Trash cannot be checked, so nothing can be restored from it.".into(),
            Outcome::Text(text) => text.clone(),
        }
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.shown()) }
}
// Callers that still speak String, the menu delete and the browser reply, get the shown sentence.
impl From<Refusal> for String {
    fn from(refusal: Refusal) -> String { refusal.shown() }
}

// `what` heads the detail the log keeps; the failure's own text follows it.
pub trait Refuse<T>: Sized {
    fn refuse(self, outcome: Outcome, what: &str) -> Result<T, Refusal>;
    fn changed(self, what: &str) -> Result<T, Refusal> { self.refuse(Outcome::Changed, what) }
}
impl<T> Refuse<T> for Option<T> {
    fn refuse(self, outcome: Outcome, what: &str) -> Result<T, Refusal> { self.ok_or_else(|| Refusal::new(outcome, what)) }
}
impl<T> Refuse<T> for Result<T, std::io::Error> {
    fn refuse(self, outcome: Outcome, what: &str) -> Result<T, Refusal> { self.map_err(|e| Refusal::os(outcome, what, &e)) }
}
impl<T> Refuse<T> for Result<T, String> {
    fn refuse(self, outcome: Outcome, what: &str) -> Result<T, Refusal> { self.map_err(|e| Refusal::new(outcome, format!("{what}: {e}"))) }
}

// PR 135, reverb256: the kernel does not sequence a flock release after close(2) completes, so a
// thread could still see the record locked once the descriptor had closed, which is the intermittent
// recovery failure they measured. The release lives inside the Arc, so it runs exactly once, at the
// last holder, on whichever thread gets there, and always before the descriptor closes.
#[derive(Debug)]
pub struct LockedFile(File);
impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = crate::uistore::unlock(&self.0);
    }
}
impl std::ops::Deref for LockedFile {
    type Target = File;
    fn deref(&self) -> &File {
        &self.0
    }
}

#[derive(Debug)]
pub struct Manifest {
    file: Arc<LockedFile>,
    end: u64,
}

#[derive(Clone, Debug)]
pub struct Records {
    file: Arc<LockedFile>,
    start: u64,
    end: u64,
}
impl Manifest {
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new().read(true).write(true).create_new(true).mode(0o600)
            .custom_flags(crate::oflags::O_NOFOLLOW).open(path)
            .map_err(|e| format!("Could not create recovery record {}: {}", path.display(), e))?;
        crate::uistore::lock_exclusive(&file).map_err(|e| format!("Could not lock recovery record: {}", e))?;
        Ok(Self { file: Arc::new(LockedFile(file)), end: 0 })
    }
    pub fn open_inactive(path: &Path) -> Result<Option<Self>, String> {
        let file = crate::backend::regfile::open_if_regular(path, crate::oflags::O_NOFOLLOW)
            .map_err(|e| format!("Could not open recovery record {}: {}", path.display(), e))?;
        // Reopen the verified inode, so replay can append its durable completion marker without a path race.
        let file = OpenOptions::new().read(true).write(true).open(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|e| format!("Could not reopen recovery record for completion: {}", e))?;
        match crate::uistore::try_lock_exclusive(&file) {
            Ok(true) => {}
            Ok(false) => return Ok(None),
            Err(error) => return Err(format!("Could not lock recovery record: {}", error)),
        }
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() { return Err("Recovery record is not a regular file.".into()); }
        Ok(Some(Self { file: Arc::new(LockedFile(file)), end: metadata.len() }))
    }
    pub fn file(&self) -> &File { &self.file }
    pub fn sync(&self) -> Result<(), String> {
        self.file.sync_all().map_err(|e| format!("Could not sync recovery record: {}", e))
    }
    pub fn new(root: &Path) -> Result<Self, String> {
        Self::new_with_location(root, true)
    }
    pub(crate) fn new_on_filesystem(root: &Path) -> Result<Self, String> {
        Self::new_with_location(root, false)
    }
    fn new_with_location(root: &Path, delete_review: bool) -> Result<Self, String> {
        let file = anonymous::open(root, delete_review)
            .map_err(|e| format!("Could not create anonymous Trash review in {}: {}", root.display(), e))?;
        Ok(Self { file: Arc::new(LockedFile(file)), end: 0 })
    }
    pub fn len(&self) -> u64 { self.end }
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.append_labelled("Trash review", bytes)
    }
    // The batched form of append_labelled: one framing per record but a single write for the batch, so a 100,000-file tree copy pays a hundred writes and not three hundred thousand.
    pub fn append_all_labelled(&mut self, label: &str, records: &[&[u8]]) -> Result<(), String> {
        let mut total = 0u64;
        for bytes in records {
            total = total.checked_add(bytes.len() as u64).and_then(|sum| sum.checked_add(2 * LENGTH_BYTES)).ok_or_else(|| format!("{label} is too large for its backing file."))?;
        }
        let end = self.end.checked_add(total).ok_or_else(|| format!("{label} is too large for its backing file."))?;
        let mut batch = Vec::with_capacity(total as usize);
        for bytes in records {
            let length = bytes.len() as u64;
            batch.extend_from_slice(&length.to_le_bytes());
            batch.extend_from_slice(bytes);
            batch.extend_from_slice(&length.to_le_bytes());
        }
        self.file.write_all_at(&batch, self.end).map_err(|e| format!("Could not write {label}: {}", e))?;
        self.end = end;
        Ok(())
    }
    // One framing for every manifest on this file: the copy manifest names its own component through this rather than hand-building lengths.
    pub fn append_labelled(&mut self, label: &str, bytes: &[u8]) -> Result<(), String> {
        let length = bytes.len() as u64;
        let end = self.end.checked_add(length).and_then(|end| end.checked_add(2 * LENGTH_BYTES))
            .ok_or_else(|| format!("{label} is too large for its backing file."))?;
        self.file.write_all_at(&length.to_le_bytes(), self.end)
            .and_then(|_| self.file.write_all_at(bytes, self.end + LENGTH_BYTES))
            .and_then(|_| self.file.write_all_at(&length.to_le_bytes(), end - LENGTH_BYTES))
            .map_err(|e| format!("Could not write {label}: {}", e))?;
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
    #[cfg(test)]
    pub(crate) fn file_raw(&self) -> i32 { self.file.as_raw_fd() }
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
mod refusal_tests;

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
