// Tag terms inside a live search session: the names are read once per row on a worker thread, never on the loop; local rows read direct and remote rows read under the mount bound.
use crate::backend::events::Event;
use crate::backend::search::{names_carry, split_query};
use crate::backend::searchindex::{is_hidden, Index};
use crate::backend::searchsession::Session;
use std::collections::{BTreeMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::backend::events::Sender;
use std::sync::{Arc, Mutex};

// One bounded call reads the tags of this many entries, so a slow share costs its deadline per chunk and never per directory.
pub(crate) const TAG_CHUNK: usize = 128;

// One lock and one batch of attribute reads per this many rows, so an answer never waits long on the cache.
const CHUNK: usize = 512;

// A query cut into its tag names (lowercased) and the text the fuzzy scorer sees; text is the query itself when no tag term stands.
pub(crate) struct Terms {
    pub tags: Vec<String>,
    pub text: String,
}

// Where the tags of a scope's rows are read: a row under a remote mount, even one nested in a local scope, reads under the mount bound.
pub(crate) struct Bound {
    scope: PathBuf,
    body: String,
    scope_remote: bool,
    // The remote mounts below a local scope, found once, so a row under none of them pays no mount-table parse.
    nested: Vec<PathBuf>,
}

impl Bound {
    pub(crate) fn new(scope: &Path, body: &str) -> Bound {
        let scope_remote = super::iomount::is_remote(scope, body);
        let nested: Vec<PathBuf> = if scope_remote {
            Vec::new()
        } else {
            let below = |mount: &PathBuf| mount.starts_with(scope) && super::iomount::is_remote(mount, body);
            super::mountinfo::mounts_in(body).into_iter().map(|(mount, _)| mount).filter(below).collect()
        };
        Bound { scope: scope.to_path_buf(), body: body.to_string(), scope_remote, nested }
    }

    // The tags of each scope-relative row, parallel to `rels`: a local row reads direct, a remote mount's rows read in bounded chunks, and a chunk the mount does not answer ends that mount's reads with its rest left unread (None) for a later retry.
    pub(crate) fn read(&self, rels: &[Vec<u8>], stop: &Arc<AtomicBool>) -> Vec<Option<Vec<String>>> {
        let mut out: Vec<Option<Vec<String>>> = vec![None; rels.len()];
        let mut remote: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
        for (at, rel) in rels.iter().enumerate() {
            if stop.load(Ordering::Relaxed) { return out; }
            let path = self.scope.join(OsStr::from_bytes(rel));
            match self.mount_of(&path) {
                Some(mount) => remote.entry(mount).or_default().push(at),
                None => {
                    let have = super::tags::read(&path).unwrap_or_default();
                    if stop.load(Ordering::Relaxed) { return out; }
                    out[at] = Some(have);
                }
            }
        }
        for (mount, rows) in remote {
            for chunk in rows.chunks(TAG_CHUNK) {
                if stop.load(Ordering::Relaxed) { return out; }
                let paths: Vec<PathBuf> = chunk.iter().map(|&at| self.scope.join(OsStr::from_bytes(&rels[at]))).collect();
                let worker_stop = Arc::clone(stop);
                let read = move || {
                    let mut have = Vec::new();
                    for path in paths {
                        if worker_stop.load(Ordering::Relaxed) { break; }
                        let tags = super::tags::read(&path).unwrap_or_default();
                        if worker_stop.load(Ordering::Relaxed) { break; }
                        have.push(tags);
                    }
                    have
                };
                if stop.load(Ordering::Relaxed) { return out; }
                let Ok(have) = super::iomount::call(mount, &self.body, "search", read) else { break };
                if stop.load(Ordering::Relaxed) { return out; }
                for (&at, tags) in chunk.iter().zip(have) {
                    out[at] = Some(tags);
                }
            }
        }
        out
    }

    // The remote mount a path reads under, None for a local path.
    fn mount_of(&self, path: &Path) -> Option<&Path> {
        if self.scope_remote {
            return Some(&self.scope);
        }
        self.nested.iter().find(|mount| path.starts_with(mount)).map(PathBuf::as_path)
    }
}

// Files whose tags Flea itself changed and one worker to re-read them: busy from the first read until the queue runs dry.
struct Rewrites {
    queue: Vec<Vec<u8>>,
    busy: bool,
}

// The row-keyed names the worker has read so far: None is not read yet, an empty list is an untagged entry.
struct Pass {
    names: Mutex<Vec<Option<Vec<String>>>>,
    bound: Bound,
    // One lock decides who re-reads: the first worker takes the queue before it lands, a later write wakes a worker of its own.
    rewrites: Mutex<Rewrites>,
    done: AtomicBool,
    announced: AtomicBool,
    // Raised when a re-read has swapped its names in, spent by the one re-run of the standing query that owes it.
    rewritten: AtomicBool,
    stop: Arc<AtomicBool>,
    // Bumped when a patch renumbers the rows, so a re-read that located its rows before it locates them again.
    epoch: AtomicUsize,
    // Raised when a bounded chunk fails and leaves rows unread, so the answer stays partial until a retry reads them; a patch that only adds rows never raises it.
    unread_failed: AtomicBool,
    // Test seams: attribute reads this pass made, so a query with no tag term can pin zero; and a gate that holds the worker before its first chunk.
    #[cfg(test)]
    reads: AtomicUsize,
    #[cfg(test)]
    held: AtomicBool,
}

impl Pass {
    fn new(len: usize, bound: Bound) -> Arc<Pass> {
        Arc::new(Pass {
            names: Mutex::new(vec![None; len]),
            bound,
            rewrites: Mutex::new(Rewrites { queue: Vec::new(), busy: true }),
            done: AtomicBool::new(false),
            announced: AtomicBool::new(false),
            rewritten: AtomicBool::new(false),
            stop: Arc::new(AtomicBool::new(false)),
            epoch: AtomicUsize::new(0),
            unread_failed: AtomicBool::new(false),
            #[cfg(test)]
            reads: AtomicUsize::new(0),
            #[cfg(test)]
            held: AtomicBool::new(false),
        })
    }
}

// A session's tag state; dropping it stops the worker, so close and a patch end the pass with no call.
pub(crate) struct Tags {
    pass: Option<Arc<Pass>>,
    stopped: bool,
    events: Sender<Event>,
    // False while the last answer ran on a pass still reading, so its matches cannot seed a narrowing.
    whole: bool,
    // The unread retry's stamp: a new query or a patch arms it, the retry spends it, so its own landing never queues another.
    armed: bool,
    // Test seam: the next pass starts held.
    #[cfg(test)]
    hold: bool,
}

impl Drop for Tags {
    fn drop(&mut self) {
        if let Some(pass) = &self.pass {
            pass.stop.store(true, Ordering::Relaxed);
        }
    }
}

impl Tags {
    pub(crate) fn new(events: Sender<Event>) -> Tags {
        Tags { pass: None, stopped: false, events, whole: true, armed: true, #[cfg(test)] hold: false }
    }

    // Unmount keeps known tags but ends this scope's reads and retries until reopen.
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
        self.whole = false;
        if let Some(pass) = &self.pass { pass.stop.store(true, Ordering::Relaxed); }
    }

    // A new query lets the unread retry fire once more.
    pub(crate) fn arm(&mut self) {
        self.armed = true;
    }

    // A patch moves row ids: a finished pass follows its rows and answers the added ones to queue; a pass still reading is dropped.
    pub(crate) fn carry(&mut self, index: &Index) -> Vec<Vec<u8>> {
        if self.stopped { return Vec::new(); }
        let Some(pass) = &self.pass else { return Vec::new() };
        if !pass.done.load(Ordering::Relaxed) {
            *self = Tags::new(self.events.clone());
            return Vec::new();
        }
        self.armed = true;
        let (moves, survivors) = index.last_patch();
        let mut names = pass.names.lock().unwrap();
        let mut next = std::mem::take(&mut *names);
        for &(at, last) in moves {
            if last < next.len() { next.swap_remove(at); }
        }
        next.resize(index.len(), None);
        // A re-read that found its rows under the old ids sees this and finds them again.
        pass.epoch.fetch_add(1, Ordering::Relaxed);
        *names = next;
        (survivors..index.len()).map(|row| index.raw(row)).collect()
    }

    // Queues the rows a patch added, so their names are read off the loop and the standing query is re-run once they are in.
    pub(crate) fn read_added(&mut self, added: Vec<Vec<u8>>, index: &Arc<Mutex<Index>>) {
        if !added.is_empty() {
            self.refresh(added, index);
        }
    }

    // Sample input: "TAG:red holiday" answers tags ["red"], text "holiday"; "bench" answers no tags and text "bench".
    pub(crate) fn terms(query: &str) -> Terms {
        let (tags, text) = split_query(query);
        let text = if tags.is_empty() { query.to_string() } else { text };
        Terms { tags, text }
    }

    // An extension scores only the previous matches: same tags, text grown, the previous answer saw every row, and no landed rewrite is unanswered.
    pub(crate) fn narrows(&self, last: &str, query: &str) -> bool {
        if last.is_empty() || !self.whole || self.pass.as_ref().is_some_and(|pass| pass.rewritten.load(Ordering::Relaxed)) {
            return false;
        }
        let (before, now) = (Tags::terms(last), Tags::terms(query));
        before.tags == now.tags && now.text.starts_with(&before.text)
    }

    // Starts the worker on the first tagged query once the walk has landed; true while rows are unread, when the answer is partial.
    pub(crate) fn prepare(&mut self, terms: &Terms, index: &Arc<Mutex<Index>>, scope: &Path, body: &str, walking: bool) -> bool {
        if self.stopped { return false; }
        if terms.tags.is_empty() {
            self.whole = true;
            return false;
        }
        // The index is still growing, so a pass now would read a part of it; the landing re-run starts it.
        if walking {
            self.whole = false;
            return true;
        }
        let pass = match &self.pass {
            Some(pass) => Arc::clone(pass),
            None => {
                let len = index.lock().unwrap().len();
                let pass = Pass::new(len, Bound::new(scope, body));
                #[cfg(test)]
                pass.held.store(self.hold, Ordering::Relaxed);
                let (worker, index, events) = (Arc::clone(&pass), Arc::clone(index), self.events.clone());
                std::thread::spawn(move || read_all(&worker, &index, len, &events));
                self.pass = Some(Arc::clone(&pass));
                pass
            }
        };
        // A failed bounded chunk left rows unread: the answer stays partial and the missing rows are queued for one re-read per query. Rows a patch added without any failure keep the old whole answer.
        if pass.done.load(Ordering::Relaxed) && pass.unread_failed.load(Ordering::Relaxed) {
            let unread: Vec<bool> = pass.names.lock().unwrap().iter().map(Option::is_none).collect();
            let missing: Vec<Vec<u8>> = {
                let index = index.lock().unwrap();
                (0..index.len()).filter(|&row| unread.get(row).is_some_and(|unread| *unread)).map(|row| index.raw(row)).collect()
            };
            if missing.is_empty() {
                pass.unread_failed.store(false, Ordering::Relaxed);
            } else {
                self.whole = false;
                let armed = std::mem::take(&mut self.armed);
                if armed {
                    self.refresh(missing, index);
                }
                // Walking while a read is in flight, and in the call that armed one, whose landing the loop answers even if the worker already cleared busy; a spent retry leaves the answer final and partial until a query or patch re-arms.
                return armed || pass.rewrites.lock().unwrap().busy;
            }
        }
        self.whole = pass.done.load(Ordering::Relaxed);
        !self.whole
    }

    // Keeps the hits whose entry carries every named tag; a row the worker has not reached yet carries none, so with no pass none is kept.
    pub(crate) fn keep(&self, hits: &mut Vec<(i32, usize)>, terms: &Terms) {
        if terms.tags.is_empty() {
            return;
        }
        let Some(pass) = &self.pass else {
            hits.clear();
            return;
        };
        let names = pass.names.lock().unwrap();
        hits.retain(|&(_, row)| matches!(names.get(row), Some(Some(have)) if names_carry(have, &terms.tags)));
    }

    // An own tag write: the named files' rows get their names re-read off the loop and swapped in whole. With no pass open it reads nothing.
    fn refresh(&mut self, rels: Vec<Vec<u8>>, index: &Arc<Mutex<Index>>) {
        if self.stopped { return; }
        let Some(pass) = &self.pass else { return };
        let mut rewrites = pass.rewrites.lock().unwrap();
        rewrites.queue.extend(rels);
        // A worker already running drains this too, before it lands, so a write during the first read is never lost.
        if !rewrites.busy {
            rewrites.busy = true;
            let (worker, index, events) = (Arc::clone(pass), Arc::clone(index), self.events.clone());
            std::thread::spawn(move || land(&worker, &index, &events, false));
        }
    }

    // True once after a re-read landed when the standing query carries a tag term, so it is re-run whatever answered in between.
    pub(crate) fn rewrite_due(&self, standing: &str) -> bool {
        let landed = self.pass.as_ref().is_some_and(|pass| pass.rewritten.swap(false, Ordering::Relaxed));
        landed && !Tags::terms(standing).tags.is_empty()
    }

    // True once after the worker finished, so the loop re-runs the standing query exactly once.
    pub(crate) fn poll_read(&self) -> bool {
        if self.stopped { return false; }
        self.pass.as_ref().is_some_and(|pass| pass.done.load(Ordering::Relaxed) && !pass.announced.swap(true, Ordering::Relaxed))
    }
}

impl Session {
    // Own tag writes: an open tag pass re-reads exactly these files' names, then the standing tagged query is re-run once; no pass reads nothing.
    pub(crate) fn note_tags_written(&mut self, written: &[PathBuf]) {
        let rels: Vec<Vec<u8>> =
            written.iter().filter_map(|path| path.strip_prefix(&self.scope).ok()).map(|rel| rel.as_os_str().as_bytes().to_vec()).collect();
        if !rels.is_empty() {
            self.tags.refresh(rels, &self.index);
        }
    }
}

// The worker: each indexed row's tags once, in chunks, then the landing event the loop answers the standing query on.
fn read_all(pass: &Pass, index: &Mutex<Index>, len: usize, events: &Sender<Event>) {
    let mut row = 0;
    #[cfg(test)]
    while pass.held.load(Ordering::Acquire) && !pass.stop.load(Ordering::Relaxed) {
        std::thread::yield_now();
    }
    while row < len {
        if pass.stop.load(Ordering::Relaxed) {
            return;
        }
        let end = (row + CHUNK).min(len);
        let rels: Vec<Vec<u8>> = {
            let index = index.lock().unwrap();
            // A patch may have shortened the index; its reset stops this pass, but the chunk in hand must not read past the end.
            (row..end.min(index.len())).map(|at| index.raw(at)).collect()
        };
        if rels.is_empty() {
            return;
        }
        let read = pass.bound.read(&rels, &pass.stop);
        #[cfg(test)]
        pass.reads.fetch_add(read.len(), Ordering::Relaxed);
        if pass.stop.load(Ordering::Relaxed) { return; }
        if read.iter().any(Option::is_none) {
            pass.unread_failed.store(true, Ordering::Relaxed);
        }
        let mut names = pass.names.lock().unwrap();
        for (offset, have) in read.into_iter().enumerate() {
            if let Some(tags) = have {
                names[row + offset] = Some(tags);
            }
        }
        row = end;
    }
    land(pass, index, events, true);
}

// Re-reads each queued write until the queue is dry, then announces: the first worker also marks the pass done under the same lock.
fn land(pass: &Pass, index: &Mutex<Index>, events: &Sender<Event>, first: bool) {
    let mut reread_any = false;
    loop {
        if pass.stop.load(Ordering::Relaxed) {
            return;
        }
        let rels = {
            let mut rewrites = pass.rewrites.lock().unwrap();
            if rewrites.queue.is_empty() {
                rewrites.busy = false;
                if first {
                    pass.done.store(true, Ordering::Relaxed);
                }
                break;
            }
            std::mem::take(&mut rewrites.queue)
        };
        reread(pass, index, rels);
        reread_any = true;
    }
    // Raised only now that the names are in, so an earlier landing cannot spend the re-run on the old ones.
    if reread_any {
        pass.rewritten.store(true, Ordering::Relaxed);
    }
    if !first {
        pass.announced.store(false, Ordering::Relaxed);
    }
    let _ = events.send(Event::SearchWalked);
}

// Finds the written files' rows in chunks, reads their names, and swaps them in under one lock: the old names stand until then.
fn reread(pass: &Pass, index: &Mutex<Index>, rels: Vec<Vec<u8>>) {
    let want: HashSet<Vec<u8>> = rels.into_iter().collect();
    loop {
        if pass.stop.load(Ordering::Relaxed) { return; }
        let epoch = pass.epoch.load(Ordering::Relaxed);
        let mut found: Vec<(usize, Vec<u8>)> = Vec::new();
        let mut row = 0;
        loop {
            if pass.stop.load(Ordering::Relaxed) { return; }
            let index = index.lock().unwrap();
            let end = (row + CHUNK).min(index.len());
            found.extend((row..end).filter(|&at| want.contains(&index.raw(at))).map(|at| (at, index.raw(at))));
            if end >= index.len() || found.len() == want.len() {
                break;
            }
            row = end;
        }
        let paths: Vec<Vec<u8>> = found.iter().map(|(_, rel)| rel.clone()).collect();
        let have = pass.bound.read(&paths, &pass.stop);
        #[cfg(test)]
        pass.reads.fetch_add(have.len(), Ordering::Relaxed);
        if pass.stop.load(Ordering::Relaxed) { return; }
        let mut names = pass.names.lock().unwrap();
        // A patch renumbered the rows while they were read, so the ids in hand are stale: locate and read again.
        if pass.epoch.load(Ordering::Relaxed) != epoch {
            continue;
        }
        // A failed re-read drops the slot's old names, so the row reads unread and partial until a retry, never whole on stale tags.
        let mut failed = false;
        for ((at, _), have) in found.iter().zip(have) {
            match (have, names.get_mut(*at)) {
                (Some(have), Some(slot)) => *slot = Some(have),
                (None, Some(slot)) => {
                    *slot = None;
                    failed = true;
                }
                _ => {}
            }
        }
        if failed {
            pass.unread_failed.store(true, Ordering::Relaxed);
        }
        return;
    }
}

// The tags of one directory's entries, one bounded call per chunk, parallel to `entries`.
pub(crate) fn read_tags(dir: &Path, body: &str, entries: &[(OsString, bool)], hidden: bool) -> Vec<Vec<String>> {
    tags_in_chunks(entries, hidden, |names| {
        let paths: Vec<PathBuf> = names.iter().map(|name| dir.join(name)).collect();
        let read = move || paths.iter().map(|path| super::tags::read(path).unwrap_or_default()).collect();
        super::iomount::call(dir, body, "search", read).ok()
    })
}

// Chunks the names the walk can match through `read`; a chunk that answers None ends the reads and leaves the rest untagged.
pub(crate) fn tags_in_chunks(entries: &[(OsString, bool)], hidden: bool, mut read: impl FnMut(&[&OsStr]) -> Option<Vec<Vec<String>>>) -> Vec<Vec<String>> {
    let mut out = vec![Vec::new(); entries.len()];
    // A dot name the hidden rule drops is never read, so it costs no call.
    let wanted: Vec<usize> = (0..entries.len()).filter(|&i| hidden || !is_hidden(&entries[i].0)).collect();
    for chunk in wanted.chunks(TAG_CHUNK) {
        let names: Vec<&OsStr> = chunk.iter().map(|&i| entries[i].0.as_os_str()).collect();
        let Some(read_tags) = read(&names) else { break };
        for (&i, tags) in chunk.iter().zip(read_tags) {
            out[i] = tags;
        }
    }
    out
}

#[cfg(test)]
#[path = "searchtags_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "searchtags_write_tests.rs"]
mod write_tests;

#[cfg(test)]
#[path = "searchtagcancel_tests.rs"]
mod cancel_tests;
