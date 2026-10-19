// The undo journal, designed in from the first operation, which is why nothing in Flea needs a confirm dialog.
use crate::backend::renamecompat::rename_path;
use crate::backend::trash;
use crate::error::{from_io, FleaError};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// An identity field for field: dev, inode, kind, length, mtime, ctime and birth time.
pub(crate) type IdentityParts = (u64, u64, u32, u64, (i64, i64), (i64, i64), Option<(u64, u32)>);

#[derive(Clone, Debug, PartialEq)]
pub struct ItemIdentity {
    dev: u64,
    ino: u64,
    kind: u32,
    len: u64,
    mtime: (i64, i64),
    changed: (i64, i64),
    born: Option<(u64, u32)>,
}

impl ItemIdentity {
    pub fn record(meta: &std::fs::Metadata) -> Self {
        Self { dev: meta.dev(), ino: meta.ino(), kind: meta.mode() & 0o170000, len: meta.len(),
            mtime: (meta.mtime(), meta.mtime_nsec()), changed: (meta.ctime(), meta.ctime_nsec()),
            born: super::permissions::born_of(meta) }
    }
    // The shared journal's wire form, field for field; the kind set it accepts is checked on read.
    pub(crate) fn to_parts(&self) -> IdentityParts {
        (self.dev, self.ino, self.kind, self.len, self.mtime, self.changed, self.born)
    }
    pub(crate) fn from_parts(dev: u64, ino: u64, kind: u32, len: u64, mtime: (i64, i64), changed: (i64, i64), born: Option<(u64, u32)>) -> Self {
        Self { dev, ino, kind, len, mtime, changed, born }
    }
    pub fn inspect(path: &std::path::Path) -> Result<Self, FleaError> {
        #[cfg(test)]
        super::opsprobe::table_read(path);
        Self::inspect_in(path, &std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default())
    }
    // The same against a mount table the caller read once, so a pass over a thousand names does not read it a thousand times.
    pub fn inspect_in(path: &std::path::Path, body: &str) -> Result<Self, FleaError> {
        let owned = path.to_path_buf();
        super::iomount::call(path, body, "journal", move || {
            owned.symlink_metadata().map(|meta| Self::record(&meta))
                .map_err(|e| from_io("journal", &owned.to_string_lossy(), &e))
        })
        .unwrap_or_else(Err)
    }
    // The shelf keeps its one-step journal in a file of its own, so it needs these four out of here.
    pub fn parts(&self) -> (u64, u64, u32) {
        (self.dev, self.ino, self.kind)
    }
    pub fn born(&self) -> Option<(u64, u32)> {
        self.born
    }
    // With no birth time on either side this stays the dev, inode and kind check it always was.
    pub fn same_item(&self, other: &Self) -> bool {
        #[cfg(test)]
        super::undoprobe::compare();
        self.dev == other.dev && self.ino == other.ino && self.kind == other.kind && !born_differs(self.born, other.born)
    }
    // A batched move removes its source only while it still holds the bytes its copy took.
    pub fn unchanged_for_move(&self, current: &Self) -> bool {
        self.same_item(current) && self.len == current.len
            && self.mtime == current.mtime && self.changed == current.changed
    }
    // The file-type bits a merge walk reads without another stat: a folder landing on a folder merges.
    pub(crate) fn is_dir(&self) -> bool {
        self.kind == S_IFDIR
    }
}

// The directory bit of the file-type field ItemIdentity::record keeps.
const S_IFDIR: u32 = 0o040000;

// A recreated file can reuse a freed inode, so unequal birth times split two items; a side with none cannot.
pub(crate) fn born_differs(was: Option<(u64, u32)>, now: Option<(u64, u32)>) -> bool {
    matches!((was, now), (Some(was), Some(now)) if was != now)
}

// One reversible step. An operation is a list of these, reversed newest first.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    // A rename or a move: the entry now lives at `to` and came from `from`; rename_fat says a cross-device redo writes the names below the top as the card's Rename.
    Moved { from: PathBuf, to: PathBuf, before: ItemIdentity, after: ItemIdentity, rename_fat: bool },
    // This operation created `path`, so reversing it removes that path.
    #[cfg_attr(not(test), allow(dead_code))]
    Created { path: PathBuf },
    // Paste as made `path` a link: undo removes it only while it is still that link.
    Linked { path: PathBuf, identity: ItemIdentity, source: PathBuf, kind: super::link::LinkKind },
    // manifest_nonce keys the recording backend's in-memory manifest; a merge top refuses the whole-tree fallback instead; rename_fat says redo writes the names below the top as the card's Rename gave them.
    Copied { from: PathBuf, to: PathBuf, source: ItemIdentity, created: ItemIdentity, manifest: Option<super::copymanifest::Handle>, manifest_nonce: Option<u64>, merge: bool, rename_fat: bool },
    // This operation made the empty directory `path`; reversing it removes it only while it is still
    // empty, because anything inside it now was put there by someone else, never by this operation.
    MadeDir { path: PathBuf, identity: ItemIdentity },
    // A merge move removed this emptied source folder; undo recreates it with its mode first.
    RemovedEmptyDir { path: PathBuf, mode: u32 },
    // New File is removable only while it remains the same untouched empty regular file.
    MadeFile { path: PathBuf, identity: ItemIdentity },
    // This operation trashed what was at `original`, and the trash holds it under `uri`.
    Trashed(trash::Entry),
    // Permissions batch: each step pins dev, inode, birth time, inode generation and mode, so a replaced path is skipped.
    Mode { path: PathBuf, before: u32, after: u32, dev: u64, ino: u64, born: Option<(u64, u32)>, generation: Option<u64> },
    // A too-big push leaves this payload-free barrier in the shared doc; no window keeps the entry.
    Barrier,
    Tags { path: PathBuf, before: Option<Vec<u8>>, after: Option<Vec<u8>>, dev: u64, ino: u64, born: Option<(u64, u32)>, generation: Option<u64> },
}

// The codec bounds an op name, so a barrier truncates an over-long one to fit it.
const MAX_OP_LEN: usize = 128;

// The codec refuses an over-long op, so the marker carries the fitting prefix.
fn placeholder_op(op: &str) -> String {
    let short: String = op.chars().take(MAX_OP_LEN).collect();
    if short.is_empty() {
        "operation".to_string()
    } else {
        short
    }
}

// A barrier is one payload-free step and nothing else, so a mixed entry never matches.
fn is_barrier(entry: &Entry) -> bool {
    matches!(&entry.steps[..], [Step::Barrier])
}

// One user-visible operation, however many steps it took, named the way the status bar already named it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub op: String,
    pub steps: Vec<Step>,
}

impl Entry {
    pub(crate) fn rebase(&mut self, old: &ItemIdentity, new: &ItemIdentity) {
        self.rebase_with(&mut |identity| if identity.unchanged_for_move(old) { *identity = new.clone(); });
    }

    pub(crate) fn rebase_with(&mut self, change: &mut impl FnMut(&mut ItemIdentity)) {
        for step in &mut self.steps {
            super::undorebase::rebase_step(step, change);
        }
    }
}

// The operations design's own number: a 50-entry ring; the cap also bounds the session file.
pub(crate) const DEPTH: usize = 50;

// What one journal entry's steps reached: the paths whose directory changed, and the files whose tags changed alone.
#[derive(Default)]
pub(crate) struct Reached {
    pub dirs: Vec<std::path::PathBuf>,
    pub tags: Vec<std::path::PathBuf>,
}

impl Reached {
    pub(crate) fn of(steps: &[Step]) -> Reached {
        let tags = steps.iter().filter_map(|step| if let Step::Tags { path, .. } = step { Some(path.clone()) } else { None }).collect();
        Reached { dirs: Journal::touched_paths(steps), tags }
    }
}

pub(crate) struct UndoToken {
    pub(crate) generation: Option<u64>,
    pub(crate) entry: Entry,
    revision: u64,
}

pub struct Journal {
    pub(crate) deferred: Option<Vec<super::renamework::Publication>>,
    entries: Vec<Entry>,
    revision: u64,
    recorded: Option<UndoToken>,
    // Entries a busy shared lock turned away, filed in order before any later entry.
    pending: Vec<Entry>,
    redo: Vec<Result<super::redo::Replay, FleaError>>,
    shared: Option<super::undoshare::Shared>,
    // Entries queue behind open trash or rename reservations, so they file in acceptance order.
    gate: super::journalgate::Gate,
    // Paths the last reversal touched, so a session patches its index; cleared on take and on entry.
    last_touched: Reached,
    // Manifests by nonce for entries this backend recorded; a claim elsewhere finds no key and falls back.
    manifests: std::collections::HashMap<u64, super::copymanifest::Handle>,
    next_nonce: u32,
    // Random per journal, so two journals in one process never mint the same nonce.
    nonce_high: u32,
}

impl Journal {
    // The paths one journal entry's steps touch; a tag step leaves every directory as it was, so Reached carries it apart.
    pub(crate) fn touched_paths(steps: &[Step]) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for step in steps {
            match step {
                Step::Moved { from, to, .. } => {
                    out.push(from.clone());
                    out.push(to.clone());
                }
                Step::Copied { to, .. } => out.push(to.clone()),
                Step::MadeDir { path, .. } | Step::MadeFile { path, .. } | Step::Created { path, .. } | Step::RemovedEmptyDir { path, .. } => {
                    out.push(path.clone())
                }
                Step::Linked { path, .. } => out.push(path.clone()),
                Step::Trashed(entry) => out.push(entry.original.clone()),
                Step::Mode { .. } | Step::Tags { .. } | Step::Barrier => {}
            }
        }
        out
    }

    // Takes the last reversal's paths, leaving none behind for the next take.
    pub(crate) fn take_last_touched(&mut self) -> Reached {
        std::mem::take(&mut self.last_touched)
    }
    pub fn new() -> Journal {
        Journal {
            deferred: None,
            entries: Vec::new(),
            revision: 0,
            recorded: None,
            pending: Vec::new(),
            redo: Vec::new(),
            shared: None,
            gate: Default::default(),
            last_touched: Reached::default(),
            manifests: std::collections::HashMap::new(),
            next_nonce: 0,
            nonce_high: nonce_high_once(),
        }
    }

    // Production attaches the session journal file; a refused directory keeps the in-memory one.
    pub(crate) fn attach_shared(&mut self) {
        if self.shared.is_none() {
            self.shared = super::undoshare::Shared::from_env();
        }
        self.drain_local();
    }

    #[cfg(test)]
    pub(crate) fn attach_test_shared(&mut self, dir: std::path::PathBuf) {
        self.shared = super::undoshare::Shared::at(dir);
        self.drain_local();
    }

    // Entries pushed before the attach join the shared doc oldest first, so none strands locally.
    fn drain_local(&mut self) {
        if self.shared.is_none() || self.entries.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.entries);
        for entry in pending {
            self.push(entry);
        }
    }

    // Drops every manifest whose nonce left the shared doc with its entry.
    fn prune(&mut self) {
        let Some(shared) = self.shared.clone() else { return };
        let Some(live) = super::undoshare::live_nonces(&shared) else { return };
        self.manifests.retain(|nonce, _| live.contains(nonce));
    }

    pub fn push(&mut self, entry: Entry) {
        if let Some(queue) = self.deferred.as_mut() { queue.push(super::renamework::Publication::Push(entry)); return; }
        if let Some(entry) = self.gate.hold(entry) {
            self.file(entry);
        }
    }

    // A trash batch reserves its place at the announce, so a write accepted in the settle window files after it.
    pub(crate) fn reserve(&mut self, id: usize) {
        if let Some(queue) = self.deferred.as_mut() { queue.push(super::renamework::Publication::Reserve(id)); return; }
        self.gate.reserve(id);
    }

    // The batch's entry (empty when all failed) fills its place, and every ready entry now leading files in queue order.
    pub(crate) fn fill(&mut self, id: usize, entry: Entry) {
        if let Some(queue) = self.deferred.as_mut() { queue.push(super::renamework::Publication::Fill(id, entry)); return; }
        if let Some(entry) = self.gate.fill(id, entry) {
            self.push(entry);
        }
        for ready in self.gate.flush() {
            self.file(ready);
        }
    }

    // A quit files what the gate still holds, so a write accepted in the settle window keeps its entry past the window.
    pub(crate) fn release_held(&mut self) {
        if let Some(queue) = self.deferred.as_mut() { queue.push(super::renamework::Publication::Release); return; }
        for ready in self.gate.release() {
            self.file(ready);
        }
    }

    // An operation that changed nothing records nothing, so undo never reports a no-op as work.
    fn file(&mut self, entry: Entry) {
        self.file_inner(entry, false);
    }

    fn file_inner(&mut self, entry: Entry, job: bool) {
        if entry.steps.is_empty() {
            return;
        }
        if !self.pending.is_empty() {
            self.pending.push(entry);
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.recorded = None;
        let shared = match &self.shared {
            Some(shared) => shared.clone(),
            None => {
                if entry.steps.is_empty() {
                    return;
                }
                self.redo.clear();
                for step in &entry.steps {
                    if let Step::Moved { before, after, .. } = step { self.rebase(before, after); }
                }
                if job {
                    self.recorded = Some(UndoToken {
                        generation: None,
                        entry: entry.clone(),
                        revision: self.revision,
                    });
                }
                self.entries.push(entry);
                if self.entries.len() > DEPTH {
                    self.entries.remove(0);
                }
                return;
            }
        };
        let mut filed = entry.clone();
        let mut nonces = Vec::new();
        for index in 0..filed.steps.len() {
            let handle = match &filed.steps[index] {
                Step::Copied { manifest: Some(handle), .. } => handle.clone(),
                _ => continue,
            };
            let nonce = self.next_nonce();
            self.manifests.insert(nonce, handle);
            nonces.push(nonce);
            if let Step::Copied { manifest, manifest_nonce, .. } = &mut filed.steps[index] {
                *manifest = None;
                *manifest_nonce = Some(nonce);
            }
        }
        match super::undoshare::push_entry_generation(&shared, &filed) {
            Ok((super::undoshare::PushResult::Stored, generation)) => {
                if job {
                    self.recorded = generation.map(|generation| UndoToken {
                        generation: Some(generation),
                        entry: filed,
                        revision: self.revision,
                    });
                }
                self.prune();
                return;
            }
            Ok((super::undoshare::PushResult::TooBig, _)) => {
                for nonce in &nonces {
                    self.manifests.remove(nonce);
                }
                // A small barrier keeps the entry's place in line; a refused one keeps the entry here.
                let barrier = Entry { op: placeholder_op(&entry.op), steps: vec![Step::Barrier] };
                match super::undoshare::push_entry(&shared, &barrier) {
                    Ok(super::undoshare::PushResult::Stored) => {
                        self.prune();
                        return;
                    }
                    Ok(super::undoshare::PushResult::TooBig) => return,
                    Err(()) if shared.contended() => {
                        self.pending.push(entry);
                        return;
                    }
                    Err(()) => self.shared = None,
                }
            }
            Err(()) if shared.contended() => {
                for nonce in &nonces {
                    self.manifests.remove(nonce);
                }
                self.pending.push(entry);
                return;
            }
            Err(()) => {
                for nonce in &nonces {
                    self.manifests.remove(nonce);
                }
                self.shared = None;
            }
        }
        if entry.steps.is_empty() {
            return;
        }
        self.redo.clear();
        for step in &entry.steps {
            if let Step::Moved { before, after, .. } = step { self.rebase(before, after); }
        }
        if job {
            self.recorded = Some(UndoToken {
                generation: None,
                entry: entry.clone(),
                revision: self.revision,
            });
        }
        self.entries.push(entry);
        if self.entries.len() > DEPTH {
            self.entries.remove(0);
        }
    }

    // Test-only: production reads the journal by undoing it, never by asking how deep it is.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // A failing step stops the rest, mode and tag steps skip changed paths with a note, and shared entries reattach this backend's manifests.
    pub fn undo(&mut self) -> Result<String, FleaError> {
        self.undo_matching(None)
    }

    fn undo_matching(&mut self, token: Option<&UndoToken>) -> Result<String, FleaError> {
        self.last_touched = Reached::default();
        self.retry_pending();
        if self.has_pending() {
            return Err(err("the undo history is busy; try again"));
        }
        if token.is_some_and(|key| {
            key.revision != self.revision || key.generation.is_some() != self.shared.is_some()
        }) {
            return Err(err("That job can no longer be undone."));
        }
        self.recorded = None;
        if let Some(shared) = self.shared.clone() {
            let (mut entry, gen) = match super::undoshare::claim_undo_matching(&shared, token) {
                Ok(Some(pair)) => pair,
                Ok(None) => {
                    return Err(err(if token.is_some() {
                        "That job can no longer be undone."
                    } else {
                        "there is nothing to undo"
                    }))
                }
                Err(()) if shared.contended() => {
                    return Err(err("the undo history is busy; try again"))
                }
                Err(()) => {
                    self.shared = None;
                    return Err(FleaError { where_: "undo".into(), path: String::new(), msg: "the undo history could not be read".into() });
                }
            };
            self.revision = self.revision.wrapping_add(1);
            // The claim already spent the barrier, so this answers and never replays it.
            if is_barrier(&entry) {
                self.prune();
                return Err(FleaError { where_: "undo".into(), path: String::new(), msg: "That operation was too large to undo.".into() });
            }
            for step in &mut entry.steps {
                if let Step::Copied { manifest, manifest_nonce: Some(nonce), .. } = step {
                    if let Some(handle) = self.manifests.remove(nonce) {
                        *manifest = Some(handle);
                    }
                }
            }
            let op = entry.op.clone();
            self.last_touched = Reached::of(&entry.steps);
            let mut changes = Vec::new();
            for step in entry.steps.iter().rev() {
                match reverse(step) {
                    Ok(Some((old, new))) => {
                        changes.push((old.clone(), new.clone()));
                        self.rebase(&old, &new);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = super::undoshare::finish_undone(&shared, None, &changes, gen);
                        self.prune();
                        return Err(error);
                    }
                }
            }
            let _ = super::undoshare::finish_undone(&shared, Some(entry), &changes, gen);
            self.prune();
            return Ok(op);
        }
        if token.is_some_and(|key| self.entries.last() != Some(&key.entry)) {
            return Err(err("That job can no longer be undone."));
        }
        let entry = match self.entries.pop() {
            Some(e) => e,
            None => return Err(err("there is nothing to undo")),
        };
        self.revision = self.revision.wrapping_add(1);
        self.last_touched = Reached::of(&entry.steps);
        let mut skipped: Vec<String> = Vec::new();
        let mut restored: Vec<Step> = Vec::new();
        for step in entry.steps.iter().rev() {
            if matches!(step, Step::Mode { .. } | Step::Tags { .. }) {
                if let Err(e) = reverse(step) {
                    skipped.push(format!("{}: {}", e.path, e.msg));
                } else {
                    restored.push(step.clone());
                }
                continue;
            }
            if let Some((old, new)) = reverse(step)? { self.rebase(&old, &new); }
        }
        if !skipped.is_empty() {
            // The undone half stays redoable, so redo replays it before anything older.
            if !restored.is_empty() {
                restored.reverse();
                let partial = Entry { op: entry.op.clone(), steps: restored.clone() };
                self.redo.push(super::redo::Replay::capture(partial));
            }
            return Err(FleaError { where_: "undo".into(), path: String::new(),
                msg: match restored.len() {
                    0 => super::redo::left_in_place(&skipped),
                    1 => format!("{}; 1 item was restored for redo", super::redo::left_in_place(&skipped)),
                    n => format!("{}; {n} items were restored for redo", super::redo::left_in_place(&skipped)),
                } });
        }
        let op = entry.op.clone();
        self.redo.push(super::redo::Replay::capture(entry));
        Ok(op)
    }

    pub fn redo_info(&mut self) -> Result<(String, usize), FleaError> {
        self.retry_pending();
        if self.has_pending() {
            return Err(err("the undo history is busy; try again"));
        }
        if let Some(shared) = self.shared.clone() {
            let info = super::undoshare::redo_info(&shared);
            self.prune();
            return info;
        }
        match self.redo.last() {
            Some(Ok(replay)) => Ok((replay.op().to_string(), replay.len())),
            Some(Err(error)) => Err(FleaError { where_: "redo".into(), path: error.path.clone(), msg: error.msg.clone() }),
            None => Err(FleaError { where_: "redo".into(), path: String::new(), msg: "there is nothing to redo".into() }),
        }
    }

    pub fn redo(&mut self, id: usize, cancel: &std::sync::atomic::AtomicBool,
                tx: &std::sync::mpsc::Sender<super::opsreq::OpMsg>) -> Result<String, FleaError> {
        self.retry_pending();
        if self.has_pending() {
            return Err(err("the undo history is busy; try again"));
        }
        self.revision = self.revision.wrapping_add(1);
        self.recorded = None;
        self.last_touched = Reached::default();
        if self.shared.is_some() {
            let (result, touched) = super::undoshare::redo_newest(&mut self.shared, id, cancel, tx);
            self.last_touched = touched;
            self.prune();
            return result;
        }
        let replay = self.redo.pop().ok_or_else(|| FleaError { where_: "redo".into(), path: String::new(), msg: "there is nothing to redo".into() })??;
        let (entry, changes, result) = replay.run(id, cancel, tx);
        for (old, new) in changes { self.rebase(&old, &new); }
        self.last_touched = Reached::of(&entry.steps);
        if !entry.steps.is_empty() { self.entries.push(entry); }
        if result.is_err() { self.redo.clear(); }
        result
    }

    fn rebase(&mut self, old: &ItemIdentity, new: &ItemIdentity) {
        for entry in &mut self.entries { entry.rebase(old, new); }
        for replay in self.redo.iter_mut().flatten() { replay.rebase(old, new); }
    }
}

// The shelf's one step back shares this no-clobber rename, its process having exited by then.
pub fn move_back(to: &std::path::Path, from: &std::path::Path) -> Result<(), FleaError> {
    rename_path(to, from)
}

pub fn copied(from: &std::path::Path, to: &std::path::Path, source: ItemIdentity) -> Result<Step, FleaError> {
    Ok(copied_as(from, to, source, ItemIdentity::inspect(to)?))
}

// The plain copy step for a destination the caller already inspected, so a batch reading the mount table once builds the same step.
pub fn copied_as(from: &std::path::Path, to: &std::path::Path, source: ItemIdentity, created: ItemIdentity) -> Step {
    Step::Copied { from: from.to_path_buf(), to: to.to_path_buf(), source, created, manifest: None, manifest_nonce: None, merge: false, rename_fat: false }
}

// A failed or cancelled tree copy carries what it managed to create; a success keeps the plain step.
pub fn copied_partial(from: &std::path::Path, to: &std::path::Path, source: ItemIdentity, manifest: Option<super::copymanifest::Handle>) -> Result<Step, FleaError> {
    Ok(Step::Copied { from: from.to_path_buf(), to: to.to_path_buf(), source, created: ItemIdentity::inspect(to)?, manifest, manifest_nonce: None, merge: false, rename_fat: false })
}

// A merge top journals what it created the same way, but never takes the whole-tree fallback.
pub fn merged_partial(from: &std::path::Path, to: &std::path::Path, source: ItemIdentity, manifest: Option<super::copymanifest::Handle>) -> Result<Step, FleaError> {
    Ok(Step::Copied { from: from.to_path_buf(), to: to.to_path_buf(), source, created: ItemIdentity::inspect(to)?, manifest, manifest_nonce: None, merge: true, rename_fat: false })
}

pub fn moved(from: &std::path::Path, to: &std::path::Path, before: ItemIdentity) -> Result<Step, FleaError> {
    Ok(Step::Moved { from: from.to_path_buf(), to: to.to_path_buf(), before, after: ItemIdentity::inspect(to)?, rename_fat: false })
}

pub(crate) fn reverse(step: &Step) -> Result<Option<(ItemIdentity, ItemIdentity)>, FleaError> {
    match step {
        // Back the way it came, and still refusing to clobber: something may occupy the old name now.
        // Only a RemovedEmptyDir step this entry recorded recreates a folder; a missing parent fails the rename and creates nothing.
        Step::Moved { from, to, after, .. } => {
            let current = ItemIdentity::inspect(to)?;
            if !after.same_item(&current) {
                return Err(FleaError { where_: "undo".into(), path: to.to_string_lossy().into(), msg: "the moved item was replaced, so undo left it in place".into() });
            }
            rename_path(to, from)?;
            return Ok(if after.unchanged_for_move(&current) { Some((current, ItemIdentity::inspect(from)?)) } else { None });
        }
        Step::RemovedEmptyDir { path, mode } => super::mergeapply::recreate_emptied(path, *mode)?,
        Step::Created { path } => remove(path)?,
        Step::Linked { path, identity, source, kind, .. } => remove_link(path, identity, source, kind)?,
        Step::Copied { to, created, manifest, merge, .. } => {
            if let Some(handle) = manifest {
                // The manifest names only what the copy made, so the coarse whole-tree checks below are skipped.

                match super::copymanifest::remove_owned(handle) {
                    super::copymanifest::Outcome::Done(report) if report.kept.is_empty() => {}
                    super::copymanifest::Outcome::Done(report) => {
                        return Err(super::copymanifest::undo_err(to, super::copymanifest::summarize(&report)));
                    }
                    // A merge top already existed, so a fallback refuses instead of taking it for something the copy made.
                    super::copymanifest::Outcome::Fallback if *merge => return Err(super::mergeapply::merge_foreign_undo(to)),
                    // Unreadable before anything went: today's check, never a wider delete.
                    super::copymanifest::Outcome::Fallback => return remove_copied(to, created),
                }
            } else if *merge {
                return Err(super::mergeapply::merge_foreign_undo(to));
            } else {
                return remove_copied(to, created);
            }
        }
        Step::MadeDir { path, identity } => {
            if !identity.same_item(&ItemIdentity::inspect(path)?) {
                return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(), msg: "the new folder was replaced, so undo left it in place".into() });
            }
            remove_empty(path)?;
        }
        Step::MadeFile { path, identity } => remove_new_file(path, identity)?,
        Step::Trashed(entry) => trash::restore(entry)?,
        Step::Mode { path, before, after, dev, ino, born, generation } => restore_mode(path, *dev, *ino, (*born, *generation), *after, *before)?,
        // A claimed barrier never reaches a reversal; refusing here keeps it from running as a file.
        Step::Barrier => return Err(FleaError { where_: "undo".into(), path: String::new(), msg: "That operation was too large to undo.".into() }),
        Step::Tags { path, before, after, dev, ino, born, generation } => super::tagsundo::reverse(path, before, after, (*dev, *ino), (*born, *generation))?,
    }
    Ok(None)
}

// Today's whole-tree check, kept for successes and for a manifest that never verified a record.
fn remove_copied(to: &PathBuf, created: &ItemIdentity) -> Result<Option<(ItemIdentity, ItemIdentity)>, FleaError> {
    if !created.unchanged_for_move(&ItemIdentity::inspect(to)?) {
        return Err(FleaError { where_: "undo".into(), path: to.to_string_lossy().into(),
            msg: "the copied item changed since this operation, so undo left it in place".into() });
    }
    if let Some(newer) = newer_inside(to, created.changed)? {
        return Err(FleaError { where_: "undo".into(), path: newer.to_string_lossy().into(),
            msg: "something inside the copied folder changed since this operation, so undo left it in place".into() });
    }
    remove(to)?;
    Ok(None)
}

fn remove_new_file(path: &PathBuf, identity: &ItemIdentity) -> Result<(), FleaError> {
    let meta = path.symlink_metadata().map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
    if !meta.is_file() || meta.len() != 0 || !identity.unchanged_for_move(&ItemIdentity::record(&meta)) {
        return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
            msg: "the new file changed since creation, so undo left it in place".into() });
    }
    std::fs::remove_file(path).map_err(|e| from_io("undo", &path.to_string_lossy(), &e))
}

// Issue 111: a directory keeps its own ctime while a file inside it is edited, and the copy sets the root mode last, so the root identity alone cannot prove the tree untouched.
fn newer_inside(root: &std::path::Path, copied: (i64, i64)) -> Result<Option<PathBuf>, FleaError> {
    let meta = root.symlink_metadata().map_err(|e| from_io("undo", &root.to_string_lossy(), &e))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Ok(None);
    }
    let entries = std::fs::read_dir(root).map_err(|e| from_io("undo", &root.to_string_lossy(), &e))?;
    for entry in entries {
        let entry = entry.map_err(|e| from_io("undo", &root.to_string_lossy(), &e))?;
        let path = entry.path();
        let meta = path.symlink_metadata().map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
        // corner: a change after this walk, or inside the filesystem timestamp granularity such as tmpfs, goes unseen.
        if (meta.ctime(), meta.ctime_nsec()) > copied {
            return Ok(Some(path));
        }
        if meta.is_dir() && !meta.file_type().is_symlink() {
            if let Some(found) = newer_inside(&path, copied)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

// Only ever a path this operation itself created, so a directory it made is removed with its contents.
fn remove(path: &PathBuf) -> Result<(), FleaError> {
    let meta = path
        .symlink_metadata()
        .map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
    let r = if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.map_err(|e| from_io("undo", &path.to_string_lossy(), &e))
}

// A link undo removes only the link made, never a folder put at its name since.
fn remove_link(path: &Path, identity: &ItemIdentity, source: &Path, kind: &super::link::LinkKind) -> Result<(), FleaError> {
    let meta = path.symlink_metadata().map_err(|e| from_io("undo", &path.to_string_lossy(), &e))?;
    let current = ItemIdentity::record(&meta);
    let still_link = match kind {
        super::link::LinkKind::Hard => !meta.is_dir(),
        _ => meta.file_type().is_symlink(),
    };
    if !identity.same_item(&current) || !still_link {
        return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
            msg: "the link changed since this operation, so undo left it in place".into() });
    }
    if *kind == super::link::LinkKind::Hard {
        // A hard link is removed only while its source still holds the same dev and inode.
        match source.symlink_metadata() {
            Ok(smeta) => {
                let s = ItemIdentity::record(&smeta);
                if s.dev != identity.dev || s.ino != identity.ino {
                    return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
                        msg: "the link source was replaced, so undo left the last name in place".into() });
                }
            }
            Err(_) => {
                return Err(FleaError { where_: "undo".into(), path: path.to_string_lossy().into(),
                    msg: "the link source is gone, so undo left the last name in place".into() });
            }
        }
    }
    std::fs::remove_file(path).map_err(|e| from_io("undo", &path.to_string_lossy(), &e))
}

// Only ever an empty directory this operation made.
fn remove_empty(path: &PathBuf) -> Result<(), FleaError> {
    // ENOTEMPTY from Linux errno.h: ErrorKind::DirectoryNotEmpty needs Rust 1.83 over the 1.77 floor.
    const ENOTEMPTY: i32 = 39;
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(ENOTEMPTY) => Err(FleaError {
            where_: "undo".to_string(),
            path: path.to_string_lossy().to_string(),
            msg: "the new folder has been filled since, so undo left it in place".to_string(),
        }),
        Err(e) => Err(from_io("undo", &path.to_string_lossy(), &e)),
    }
}

fn err(msg: &str) -> FleaError {
    FleaError { where_: "undo".to_string(), path: String::new(), msg: msg.to_string() }
}

// A mode undo restores bits through the held no-follow descriptor.
pub(crate) fn restore_mode(path: &Path, dev: u64, ino: u64, born: super::filegen::Birth, expected: u32, target: u32) -> Result<(), FleaError> {
    super::permissions::chmod_pinned(path, dev, ino, born, expected, target).map_err(|msg| {
        FleaError { where_: "undo".to_string(), path: path.to_string_lossy().to_string(), msg }
    })
}

#[cfg(test)]
#[path = "undo_tests.rs"]
mod tests;

#[path = "undojobs.rs"]
mod jobs;

#[path = "undononce.rs"]
mod nonce;
use nonce::nonce_high_once;

#[cfg(test)]
#[path = "undo_job_tests.rs"]
mod job_tests;
