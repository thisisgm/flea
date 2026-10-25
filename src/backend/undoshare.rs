// One undo history for every Flea window; see AGENTS.md "Write operations and the undo journal".
use super::undorebase::RebaseMap;
use super::undostage::{read, Read, Staged};
use crate::backend::manifestdir::current_uid;
use crate::backend::redo::Replay;
use crate::backend::undo::{Entry, ItemIdentity, Step, DEPTH};
use crate::error::FleaError;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

// A closed stderr pipe must never unwind publication or detach its journal history.
fn diagnostic(message: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "flea: {message}");
}

// Past twice the largest benched tree in manifest bytes the file is foreign, as copymanifest refuses its stream.
pub(crate) const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
// The redo stack is unbounded in memory, but a file without a cap is not, so it caps here.
const MAX_REDOS: usize = 50;
pub(crate) const JOURNAL_FILE: &str = "undo-journal";
const LOCK_FILE: &str = "undo-journal.lock";
const DIR_MODE: u32 = 0o700;

#[derive(Clone, Debug)]
pub(crate) struct Shared {
    dir: PathBuf,
    // Set when this handle's last lock attempt timed out behind another holder, so its caller retries instead of detaching.
    contended: std::cell::Cell<bool>,
}

impl Shared {
    // FLEA_UNDO_DIR isolates one test run when set; otherwise the session dir under XDG_RUNTIME_DIR.
    pub(crate) fn from_env() -> Option<Shared> {
        if let Some(dir) = crate::userfile::env_dir("FLEA_UNDO_DIR") {
            return Self::at(dir);
        }
        let runtime = crate::userfile::env_dir("XDG_RUNTIME_DIR")?;
        Self::at(runtime.join("flea"))
    }

    // Tests take their directories as arguments, never a process-wide variable another test reads.
    pub(crate) fn at(dir: PathBuf) -> Option<Shared> {
        match validate_dir(&dir) {
            Ok(()) => Some(Shared {
                dir,
                contended: Default::default(),
            }),
            Err(msg) => {
                diagnostic(format_args!("{}", msg));
                None
            }
        }
    }

    fn journal_path(&self) -> PathBuf {
        self.dir.join(JOURNAL_FILE)
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK_FILE)
    }

    pub(crate) fn contended(&self) -> bool {
        self.contended.get()
    }
}

// Created owner-only and re-read rather than trusted, so a planted symlink loses like manifestdir.
fn validate_dir(dir: &Path) -> Result<(), String> {
    if dir.as_os_str().is_empty() {
        return Err("the shared undo directory is empty, so undo stays in this process".to_string());
    }
    match dir.symlink_metadata() {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(dir).map_err(|e| {
                format!("{} could not be created, so undo stays in this process ({:?})", dir.display(), e.kind())
            })?;
        }
        Err(e) => {
            return Err(format!("{} could not be read, so undo stays in this process ({:?})", dir.display(), e.kind()))
        }
    }
    let meta = dir.symlink_metadata().map_err(|e| {
        format!("{} could not be re-read, so undo stays in this process ({:?})", dir.display(), e.kind())
    })?;
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a symbolic link, so undo stays in this process", dir.display()));
    }
    if !meta.is_dir() {
        return Err(format!("{} is not a directory, so undo stays in this process", dir.display()));
    }
    if meta.uid() != current_uid() {
        return Err(format!("{} is not owned by this user, so undo stays in this process", dir.display()));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(format!("{} is accessible to group or others, so undo stays in this process", dir.display()));
    }
    Ok(())
}

#[path = "undosharelock.rs"]
mod sharelock;
use sharelock::lock;

// A stored Err replay answers its error when taken, exactly as the in-memory pop answers it.
pub(crate) enum StoredRedo {
    Ok(Replay),
    Err(FleaError),
}

pub(crate) struct Doc {
    pub undo: Vec<Entry>,
    pub redo: Vec<StoredRedo>,
    // Counts new pushes; overlapping Undo claims must each keep their Redo.
    pub push_gen: u64,
    // Job offers expire on claims and replay publication as well as new pushes.
    pub job_gen: u64,
}

pub(crate) enum PushResult { Stored, TooBig }

fn empty_doc() -> Doc {
    Doc {
        undo: Vec::new(),
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    }
}

// An existing file this reader cannot understand stays owned by its original writer.
fn ignored(_shared: &Shared) -> Result<Doc, ()> {
    Err(())
}

// One read and one parse; anything but a readable journal of our version is Err, so its writer keeps it.
fn load(shared: &Shared) -> Result<Doc, ()> {
    let path = shared.journal_path();
    let meta = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(empty_doc()),
        Err(e) => {
            diagnostic(format_args!("the shared undo journal could not be read ({:?}), so it was ignored", e.kind()));
            return ignored(shared);
        }
    };
    if meta.file_type().is_symlink() {
        diagnostic(format_args!("the shared undo journal is a symbolic link, so it was ignored"));
        return ignored(shared);
    }
    if !meta.is_file() {
        diagnostic(format_args!("the shared undo journal is not a regular file, so it was ignored"));
        return ignored(shared);
    }
    if meta.uid() != current_uid() {
        diagnostic(format_args!("the shared undo journal is not owned by this user, so it was ignored"));
        return ignored(shared);
    }
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(e) => {
            diagnostic(format_args!("the shared undo journal could not be read ({:?}), so it was ignored", e.kind()));
            return ignored(shared);
        }
    };
    let mut bytes = Vec::new();
    use std::io::Read as _;
    if file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).is_err() {
        diagnostic(format_args!("the shared undo journal could not be read, so it was ignored"));
        return ignored(shared);
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        diagnostic(format_args!("the shared undo journal is oversized, so it was ignored"));
        return ignored(shared);
    }
    #[cfg(test)]
    super::undoprobe::read(bytes.len());
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            diagnostic(format_args!("the shared undo journal is not valid UTF-8, so it was ignored"));
            return ignored(shared);
        }
    };
    match read(&text) {
        Read::Doc(doc) => Ok(doc),
        Read::Newer => Err(()),
        Read::Malformed => {
            diagnostic(format_args!("the shared undo journal is malformed, so it was ignored"));
            Err(())
        }
    }
}

// Exclusive temp plus rename last, so a reader sees a complete file or none at all.
fn store(shared: &Shared, text: &str) -> Result<(), String> {
    let dest = shared.journal_path();
    let tmp = shared.dir.join(format!("{}.{}.tmp", JOURNAL_FILE, std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(crate::oflags::O_NOFOLLOW)
        .open(&tmp)
        .map_err(|e| format!("the shared undo journal could not be written ({:?})", e.kind()))?;
    use std::io::Write;
    #[cfg(test)]
    super::undoprobe::wrote(text.len());
    if file.write_all(text.as_bytes()).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err("the undo history could not be saved".to_string());
    }
    drop(file);
    std::fs::rename(&tmp, &dest)
        .map_err(|e| format!("the shared undo journal could not be published ({:?})", e.kind()))
}

// One walk of the document for the whole batch of moves, however many there are.
fn rebase_doc<'a>(doc: &mut Doc, pairs: impl IntoIterator<Item = (&'a ItemIdentity, &'a ItemIdentity)>) {
    let map = RebaseMap::new(pairs);
    if map.is_empty() {
        return;
    }
    for entry in &mut doc.undo {
        entry.rebase_with(&mut |identity| map.moved(identity));
    }
    for stored in &mut doc.redo {
        if let StoredRedo::Ok(replay) = stored {
            replay.rebase_with(&mut |identity| map.moved(identity), &mut |identity| map.item(identity));
        }
    }
}

// Renders the document once and trims by the sizes of its pieces; None when the newest record alone is over the cap.
fn staged_to_fit(doc: &Doc) -> Option<Staged> {
    let mut staged = Staged::new(doc);
    staged.trim_to(MAX_FILE_BYTES);
    staged.fits(MAX_FILE_BYTES).then_some(staged)
}

pub(crate) fn push_entry(shared: &Shared, entry: &Entry) -> Result<PushResult, ()> {
    push_entry_generation(shared, entry).map(|(result, _)| result)
}

pub(crate) fn push_entry_generation(
    shared: &Shared,
    entry: &Entry,
) -> Result<(PushResult, Option<u64>), ()> {
    if entry.steps.is_empty() {
        return Ok((PushResult::Stored, None));
    }
    let _guard = lock(shared).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    // A newer file is unavailable, never rewritten: the caller falls back to memory.
    let mut doc = load(shared)?;
    rebase_doc(&mut doc, entry.steps.iter().filter_map(|step| match step {
        Step::Moved { before, after, .. } => Some((before, after)),
        _ => None,
    }));
    doc.undo.push(entry.clone());
    while doc.undo.len() > DEPTH {
        doc.undo.remove(0);
    }
    doc.redo.clear();
    doc.push_gen = doc.push_gen.wrapping_add(1);
    doc.job_gen = doc.job_gen.wrapping_add(1);
    let Some(staged) = staged_to_fit(&doc) else {
        return Ok((PushResult::TooBig, None));
    };
    store(shared, &staged.text()).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    Ok((PushResult::Stored, Some(doc.job_gen)))
}

// Every nonce the doc still names, so a journal drops handles for entries it no longer holds.
pub(crate) fn live_nonces(shared: &Shared) -> Option<std::collections::HashSet<u64>> {
    let _guard = lock(shared).map_err(|msg| diagnostic(format_args!("{}", msg))).ok()?;
    let doc = load(shared).ok()?;
    let mut live = std::collections::HashSet::new();
    for entry in &doc.undo {
        for step in &entry.steps {
            if let Step::Copied { manifest_nonce: Some(nonce), .. } = step {
                live.insert(*nonce);
            }
        }
    }
    for stored in &doc.redo {
        if let StoredRedo::Ok(replay) = stored {
            for (step, _, _) in replay.steps_data().1 {
                if let Step::Copied { manifest_nonce: Some(nonce), .. } = &step {
                    live.insert(*nonce);
                }
            }
        }
    }
    Some(live)
}

// One locked read-modify-write pops the newest entry; the generation lets finish drop a stale replay.
#[cfg(test)]
pub(crate) fn claim_undo(shared: &Shared) -> Result<Option<(Entry, u64)>, ()> {
    claim_undo_matching(shared, None)
}

pub(crate) fn claim_undo_matching(
    shared: &Shared,
    token: Option<&super::undo::UndoToken>,
) -> Result<Option<(Entry, u64)>, ()> {
    let _guard = lock(shared).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    let mut doc = load(shared)?;
    if token.is_some_and(|key| {
        key.generation != Some(doc.job_gen) || doc.undo.last() != Some(&key.entry)
    }) {
        return Ok(None);
    }
    let claimed = doc.undo.pop();
    if claimed.is_some() {
        doc.job_gen = doc.job_gen.wrapping_add(1);
        store(shared, &Staged::new(&doc).text()).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    }
    Ok(claimed.map(|entry| (entry, doc.push_gen)))
}

// The entry is spent whether the reversal worked or not; a replay lands only with no push since claim.
pub(crate) fn finish_undone(shared: &Shared, entry: Option<Entry>, changes: &[(ItemIdentity, ItemIdentity)], claimed_gen: u64) -> Result<(), ()> {
    let _guard = lock(shared).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    let mut doc = load(shared)?;
    rebase_doc(&mut doc, changes.iter().map(|(old, new)| (old, new)));
    if let Some(entry) = entry {
        if doc.push_gen == claimed_gen {
            match Replay::capture(entry) {
                Ok(replay) => doc.redo.push(StoredRedo::Ok(replay)),
                Err(error) => doc.redo.push(StoredRedo::Err(error)),
            }
            while doc.redo.len() > MAX_REDOS {
                doc.redo.remove(0);
            }
        }
    }
    let mut staged = Staged::new(&doc);
    staged.trim_to(MAX_FILE_BYTES);
    if !staged.fits(MAX_FILE_BYTES) && staged.drop_newest_redo() {
        // The replay alone overflowed; keep history without it rather than publishing oversize.
        staged.trim_to(MAX_FILE_BYTES);
    }
    if !staged.fits(MAX_FILE_BYTES) {
        return Ok(());
    }
    store(shared, &staged.text()).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    Ok(())
}

// One constructor for every redo refusal, so the two sentences stay byte-identical.
fn redo_error(msg: &str) -> FleaError {
    FleaError { where_: "redo".into(), path: String::new(), msg: msg.into() }
}

pub(crate) fn redo_info(shared: &Shared) -> Result<(String, usize), FleaError> {
    let _guard = lock(shared).map_err(|msg| {
        diagnostic(format_args!("{}", msg));
        redo_error("the undo history could not be read")
    })?;
    let doc = load(shared).map_err(|()| redo_error("the undo history could not be read"))?;
    match doc.redo.last() {
        Some(StoredRedo::Ok(replay)) => Ok((replay.op().to_string(), replay.len())),
        Some(StoredRedo::Err(error)) => {
            Err(FleaError { where_: "redo".into(), path: error.path.clone(), msg: error.msg.clone() })
        }
        None => Err(redo_error("there is nothing to redo")),
    }
}

pub(crate) fn claim_redo(shared: &Shared) -> Result<Option<StoredRedo>, ()> {
    let _guard = lock(shared).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    let mut doc = load(shared)?;
    let claimed = doc.redo.pop();
    if claimed.is_some() {
        doc.job_gen = doc.job_gen.wrapping_add(1);
        store(shared, &Staged::new(&doc).text()).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    }
    Ok(claimed)
}

// Mirrors Journal::redo's tail: the redone entry rejoins the undo stack, and a failed redo clears it.
pub(crate) fn finish_redone(shared: &Shared, entry: Entry, changes: &[(ItemIdentity, ItemIdentity)], failed: bool) -> Result<(), ()> {
    let _guard = lock(shared).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    let mut doc = load(shared)?;
    rebase_doc(&mut doc, changes.iter().map(|(old, new)| (old, new)));
    if !entry.steps.is_empty() {
        doc.job_gen = doc.job_gen.wrapping_add(1);
        doc.undo.push(entry);
        while doc.undo.len() > DEPTH {
            doc.undo.remove(0);
        }
    }
    if failed {
        doc.redo.clear();
    }
    let Some(staged) = staged_to_fit(&doc) else { return Ok(()) };
    store(shared, &staged.text()).map_err(|msg| diagnostic(format_args!("{}", msg)))?;
    Ok(())
}

pub(crate) fn redo_newest(slot: &mut Option<Shared>, id: usize, cancel: &std::sync::atomic::AtomicBool,
    tx: &std::sync::mpsc::Sender<super::opsreq::OpMsg>) -> (Result<String, FleaError>, super::undo::Reached) {
    let Some(shared) = slot.clone() else {
        return (Err(redo_error("there is nothing to redo")), Default::default());
    };
    let claimed = match claim_redo(&shared) {
        Ok(claimed) => claimed,
        Err(()) if shared.contended() => {
            return (
                Err(redo_error("the undo history is busy; try again")),
                Default::default(),
            );
        }
        Err(()) => {
            *slot = None;
            return (Err(redo_error("the undo history could not be read")), Default::default());
        }
    };
    let replay = match claimed {
        Some(StoredRedo::Ok(replay)) => replay,
        Some(StoredRedo::Err(error)) => return (Err(error), Default::default()),
        None => return (Err(redo_error("there is nothing to redo")), Default::default()),
    };
    let (entry, changes, result) = replay.run(id, cancel, tx);
    let failed = result.is_err();
    // The entry moves into the shared doc, so its paths are read before the finish takes it.
    let touched = super::undo::Reached::of(&entry.steps);
    let _ = finish_redone(&shared, entry, &changes, failed);
    (result, touched)
}

#[cfg(test)]
#[path = "undoshare_tests.rs"]
mod tests;
