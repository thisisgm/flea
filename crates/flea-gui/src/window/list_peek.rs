//! The folder a List view preview lists: its first entries read off the UI thread, one read at a time.
//!
//! A folder wanted while a read runs waits for it, and the running read stops early once it is no longer
//! wanted. An answer lands only on the peek of the read that asked for it, so a slow read never overwrites a
//! newer one. Every entry is counted, but no more than the shown ones are held while reading. A read stuck in
//! the filesystem past STALL (a share that stopped answering) no longer holds other folders back: each read
//! runs on its own thread, and a stuck one's late answer is dropped.
use crate::state::Shell;
use gpui::Context;
use std::{
    cell::RefCell,
    collections::BinaryHeap,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

/// Entries a folder preview lists at most; the count above them is the real one.
const MAX_ENTRIES: usize = 500;

/// A folder preview older than this is read again, so a folder that changed under it catches up.
const PEEK_FRESH: std::time::Duration = std::time::Duration::from_secs(2);

/// A read running longer than this is taken as stuck: another folder may start its own read meanwhile.
const STALL: std::time::Duration = std::time::Duration::from_secs(3);

/// Symlinks a read follows to tell a folder from a file; past this many a link ranks as a file.
const MAX_FOLLOWED: usize = MAX_ENTRIES;

/// What a folder read gave: its first entries and whole count, why it could not be listed, or that its own
/// storage holds it until Load.
#[derive(Clone)]
pub(super) enum Answer {
    Listed(Vec<(String, bool)>, usize),
    Failed(String),
    Held,
}

/// The load rules a read checks the folder's own storage against, by class (CLASSES' order); none once Load
/// released the folder.
pub(super) struct Gate {
    pub parent: String,
    pub loads: [bool; 4],
}

struct Peek {
    path: String,
    hidden: bool,
    released: bool,
    /// The storage rules the read checks against, by class; a change of rules asks a new read.
    rules: Option<[bool; 4]>,
    /// The read this peek waits for; only that read's answer lands here.
    generation: u64,
    read_at: std::time::Instant,
    entries: Option<Answer>,
}

#[derive(Default)]
struct Peeks {
    shown: Option<Peek>,
    /// The one read in flight, by generation, and when it started; a newer folder waits for it and then reads
    /// by itself, unless it has run past STALL.
    running: Option<(u64, std::time::Instant)>,
    next: u64,
    /// The folder Load released under Load: manual or a held storage class, with the listing it was released
    /// in, until the cursor leaves it, the listing changes or the preview stops drawing it.
    released: Option<(String, usize)>,
}

thread_local! {
    static PEEK: RefCell<Peeks> = RefCell::default();
}

/// The generation a running read checks between entries: once a newer folder is wanted it stops early.
static WANTED: AtomicU64 = AtomicU64::new(0);

/// Load (Ctrl+Space) on a folder row beside the list: its entries are read even where the preview waits to be
/// asked. False when the cursor row is no single folder.
pub(crate) fn release(host: &super::pane::PaneHost) -> bool {
    let Some(row) = host
        .pane
        .held
        .row(host.pane.cursor)
        .filter(|row| row.dir && !row.name.is_empty() && host.selected.len() <= 1)
    else {
        return false;
    };
    let path = join(host.pane.path(), &row.name);
    PEEK.with(|peek| peek.borrow_mut().released = Some((path, host.pane.held.listing)));
    true
}

/// An entry in the order the preview lists them, folders first, then by name without case.
#[derive(PartialEq, Eq)]
struct Ranked {
    key: (bool, String),
    entry: (String, bool),
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .key
            .0
            .cmp(&self.key.0)
            .then_with(|| self.key.1.cmp(&other.key.1))
            .then_with(|| self.entry.0.cmp(&other.entry.0))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The first MAX_ENTRIES entries in listing order and the whole count, holding no more than those entries
/// at once; it gives up as soon as a newer folder is wanted.
fn read(path: &str, hidden: bool, generation: u64, gate: Option<Gate>) -> Option<Answer> {
    // The folder's own storage: a mount point or a link onto a held class waits for Load, as a file there would.
    if let Some(gate) = gate {
        let table = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
        let class = super::list_mount::crossing(&table, Path::new(&gate.parent), Path::new(path));
        let at = |class| {
            super::list_mount::CLASSES
                .iter()
                .position(|known| *known == class)
        };
        if class.is_some_and(|class| at(class).is_none_or(|at| !gate.loads[at])) {
            return Some(Answer::Held);
        }
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => return Some(Answer::Failed(error.to_string())),
    };
    let mut kept = BinaryHeap::with_capacity(MAX_ENTRIES + 1);
    let mut total = 0;
    let mut followed = 0;
    for entry in entries {
        if WANTED.load(Ordering::Relaxed) != generation {
            return None;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        // A link is followed only for the first MAX_FOLLOWED, so a folder of thousands of links costs no more
        // target checks than the rows it can show.
        let dir = entry.file_type().is_ok_and(|kind| {
            kind.is_dir()
                || (kind.is_symlink() && followed < MAX_FOLLOWED && {
                    followed += 1;
                    entry.path().is_dir()
                })
        });
        total += 1;
        kept.push(Ranked {
            key: (dir, name.to_lowercase()),
            entry: (name, dir),
        });
        // The heap's greatest is the entry listed last, so dropping it keeps the first MAX_ENTRIES.
        if kept.len() > MAX_ENTRIES {
            kept.pop();
        }
    }
    let entries = kept
        .into_sorted_vec()
        .into_iter()
        .map(|ranked| ranked.entry)
        .collect();
    Some(Answer::Listed(entries, total))
}

pub(super) fn join(directory: &str, name: &str) -> String {
    if name.starts_with('/') {
        name.to_owned()
    } else if directory.ends_with('/') {
        format!("{directory}{name}")
    } else {
        format!("{directory}/{name}")
    }
}

/// What the preview lists for this folder, once a read for it has answered.
pub(super) fn shown(path: &str, hidden: bool, released: bool) -> Option<Answer> {
    PEEK.with(|peek| {
        peek.borrow()
            .shown
            .as_ref()
            .filter(|peek| peek.path == path && peek.hidden == hidden && peek.released == released)
            .and_then(|peek| peek.entries.clone())
    })
}

/// Whether Load released this folder in this listing, forgetting a release for anything else.
pub(super) fn released(path: &str, listing: usize) -> bool {
    PEEK.with(|peek| {
        let mut peek = peek.borrow_mut();
        peek.released = peek
            .released
            .take()
            .filter(|(released, at)| released == path && *at == listing);
        peek.released.is_some()
    })
}

/// The preview draws no folder this frame (a file row, a folder it may not read, the preview hidden or another
/// view): the release is gone, and the read in flight stops at its next entry. It stays the one read in flight
/// until it really returns.
pub(super) fn leave() {
    PEEK.with(|peek| {
        let mut peek = peek.borrow_mut();
        if peek.shown.is_none() && peek.released.is_none() {
            return;
        }
        peek.shown = None;
        peek.released = None;
        peek.next += 1;
        WANTED.store(peek.next, Ordering::Relaxed);
    })
}

impl Shell {
    /// Reads the folder unless its answer is fresh. One read runs at a time: a folder wanted meanwhile is
    /// read once that read lands, and an answer lands only on the peek of the read that asked it.
    pub(super) fn peek(
        &self,
        path: &str,
        hidden: bool,
        gate: Option<Gate>,
        cx: &mut Context<Self>,
    ) {
        let released = gate.is_none();
        let rules = gate.as_ref().map(|gate| gate.loads);
        let start = PEEK.with(|peek| {
            let mut peek = peek.borrow_mut();
            let current = peek.shown.as_ref().is_some_and(|shown| {
                shown.path == path
                    && shown.hidden == hidden
                    && shown.released == released
                    && shown.rules == rules
            });
            let fresh = current
                && peek.shown.as_ref().is_some_and(|shown| {
                    shown.entries.is_none() || shown.read_at.elapsed() < PEEK_FRESH
                });
            if !current {
                // A read for another folder stops early; the peek waits for its own read.
                peek.next += 1;
                let generation = peek.next;
                WANTED.store(generation, Ordering::Relaxed);
                peek.shown = Some(Peek {
                    path: path.to_owned(),
                    hidden,
                    released,
                    rules,
                    generation,
                    read_at: std::time::Instant::now(),
                    entries: None,
                });
            } else if !fresh && peek.running.is_none() {
                // A re-read of the folder already shown keeps its entries until the new ones land.
                peek.next += 1;
                let generation = peek.next;
                WANTED.store(generation, Ordering::Relaxed);
                if let Some(shown) = peek.shown.as_mut() {
                    shown.generation = generation;
                    shown.read_at = std::time::Instant::now();
                }
            }
            let shown = peek.shown.as_ref()?;
            let waiting = shown.entries.is_none() || !fresh;
            // A stuck read holds back only its own folder; any other folder starts a read beside it.
            let free = peek.running.is_none_or(|(running, at)| {
                running != shown.generation && at.elapsed() > STALL
            });
            (free && waiting).then_some(shown.generation)
        });
        let Some(generation) = start else { return };
        PEEK.with(|peek| {
            peek.borrow_mut().running = Some((generation, std::time::Instant::now()))
        });
        let wanted = path.to_owned();
        // Its own thread, so a read stuck in the filesystem never holds a shared executor thread.
        let (sender, answer) = async_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("flea-list-peek".into())
            .spawn(move || {
                let _ = sender.send_blocking(read(&wanted, hidden, generation, gate));
            });
        if spawned.is_err() {
            PEEK.with(|peek| peek.borrow_mut().running = None);
            return;
        }
        // A ticker wakes the window once the read is past STALL, so a folder waiting behind it starts its own.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STALL).await;
            let stuck = PEEK.with(|peek| {
                peek.borrow()
                    .running
                    .is_some_and(|(running, _)| running == generation)
            });
            if stuck {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let read = answer.recv().await.ok().flatten();
            PEEK.with(|peek| {
                let mut peek = peek.borrow_mut();
                // A late answer from a stuck read leaves the read that replaced it running.
                if peek.running.is_some_and(|(running, _)| running == generation) {
                    peek.running = None;
                }
                if let (Some(shown), Some(read)) = (
                    peek.shown
                        .as_mut()
                        .filter(|shown| shown.generation == generation),
                    read,
                ) {
                    shown.entries = Some(read);
                    shown.read_at = std::time::Instant::now();
                }
            });
            // Drawn again, the folder now under the cursor starts its own read if it still waits.
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "list_peek_tests.rs"]
mod tests;
