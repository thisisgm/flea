// A slow mount's statfs is a network round trip (about 800 ms on the NAS), so it runs on a worker, never the loop.
use super::events::Event;
use super::extclass::{classify_entry_with_magic, fstype_is_network, gvfs_class, gvfs_root, resolved_in};
use super::fsinfo::Info;
use super::mountinfo::{mount_entry_in, on_system_volume};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::backend::events::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

// A statfs outstanding this long is presumed hung, so the next ask for its mount starts another.
pub const FSINFO_DEADLINE: Duration = Duration::from_secs(10);
// corner: past this many folders the remembered figures are dropped whole; the next visit reads them again.
const KNOWN_MAX: usize = 64;

// Sample input: root "/run/user/1000/gvfs/smb-share:server=n,share=x", dir root + "/photos", info Some(fuse, 7).
pub struct Done {
    pub root: PathBuf,
    pub dir: PathBuf,
    // Which statfs this is, so a hung one landing late is told apart from its replacement.
    pub seq: u64,
    pub info: Option<Info>,
    // The folder's tag probe, taken on the worker beside the statfs so a share is never asked on the loop.
    pub tagable: bool,
}

// The test seam: production reads statfs once for figures and magic, a test sleeps or counts instead.
type Reader = Arc<dyn Fn(PathBuf) -> (Option<Info>, Option<i64>) + Send + Sync>;

pub struct FsInfo {
    events: Sender<Event>,
    reader: Reader,
    // Figures per folder, because an sftp host or a phone's storages answer differently below one mount root.
    known: HashMap<PathBuf, Option<Info>>,
    // One statfs per mount root at a time, with its start and number, so fast navigation never piles round trips on the daemon.
    inflight: HashMap<PathBuf, (Instant, u64)>,
    // Mount roots asked for again while their statfs was busy; the folder on screen is read when it lands.
    waiting: HashSet<PathBuf>,
    // The number of the last statfs started; the next one takes one more.
    seq: u64,
    // Set by the first fsinfo ask; the TUI never asks, so its listings start no statfs, as in 0.3.5.
    asked: bool,
    // The newest listing's first plain file: statfs of a folder on an SMB share lists the whole folder again, of a file opens that file.
    probe: Option<PathBuf>,
    // The folder the tag probe last answered for and its answer, so a repeated fsinfo makes no xattr call.
    tagged: Option<(PathBuf, bool)>,
}

impl FsInfo {
    pub fn new(events: Sender<Event>) -> Self {
        Self::with_reader(events, Arc::new(|path: PathBuf| super::fsinfo::read_with_magic(&path)))
    }

    pub fn with_reader(events: Sender<Event>, reader: Reader) -> Self {
        FsInfo { events, reader, known: HashMap::new(), inflight: HashMap::new(), waiting: HashSet::new(), seq: 0, asked: false, probe: None, tagged: None }
    }

    // After a share's rows are out, so the statfs never competes with its gio listing; a kernel mount waits for fsinfo.
    pub fn list_arrived(&mut self, path: &Path, listing: &super::listing::Listing) {
        self.probe = listing.first_plain_file().map(|name| path.join(name));
        if !self.asked {
            return;
        }
        if let Some(root) = gvfs_root(path) {
            self.refresh(root, path.to_path_buf());
        }
    }

    // Never blocked on a slow mount: class, last known figures and, for a local dir, the system flag now; fresh figures later.
    pub fn answer(&mut self, path: &Path) -> (Option<Info>, &'static str, bool) {
        let body = if gvfs_root(path).is_some() { String::new() } else { std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default() };
        self.answer_in(path, &body)
    }

    // The test seam: body is one /proc/self/mountinfo read, ignored for a gvfs path.
    fn answer_in(&mut self, path: &Path, body: &str) -> (Option<Info>, &'static str, bool) {
        self.asked = true;
        if let Some(root) = gvfs_root(path) {
            return self.slow_answer(root, path, slow_class(path));
        }
        // The raw path's own mount first, so a kernel share whose server died is decided with no syscall on the loop.
        if let Some(entry) = mount_entry_in(path, body).filter(|e| fstype_is_network(&e.fstype)) {
            return self.slow_answer(entry.mount, path, "network");
        }
        let owned = resolved_in(path, body);
        if let Some(root) = gvfs_root(&owned) {
            return self.slow_answer(root, path, slow_class(&owned));
        }
        match mount_entry_in(&owned, body) {
            Some(entry) if fstype_is_network(&entry.fstype) => self.slow_answer(entry.mount, path, "network"),
            entry => {
                let (info, magic) = (self.reader)(path.to_path_buf());
                // By the owning mount's number in the mount table, so a symlink, a bind mount or a btrfs subvolume names its real volume.
                (info, classify_entry_with_magic(&owned, entry.as_ref(), magic), entry.as_ref().is_some_and(|e| on_system_volume(e, body)))
            }
        }
    }

    fn slow_answer(&mut self, root: PathBuf, path: &Path, class: &'static str) -> (Option<Info>, &'static str, bool) {
        let figures = self.known.get(path).cloned().flatten();
        self.refresh(root, path.to_path_buf());
        (figures, class, false)
    }

    fn refresh(&mut self, root: PathBuf, dir: PathBuf) {
        if self.inflight.get(&root).is_some_and(|(started, _)| started.elapsed() < FSINFO_DEADLINE) {
            self.waiting.insert(root);
            return;
        }
        self.seq += 1;
        let (events, reader, key, seq) = (self.events.clone(), Arc::clone(&self.reader), root.clone(), self.seq);
        // Only a share's own file stands in for its folder; a file gone since the listing falls back to the folder.
        let probe = self.probe.clone().filter(|file| gvfs_root(&dir).is_some() && file.parent() == Some(dir.as_path()));
        let worker = std::thread::Builder::new().name("flea-fsinfo".into()).spawn(move || {
            let info = probe.and_then(|file| reader(file).0).or_else(|| reader(dir.clone()).0);
            let tagable = super::tags::dir_supports(&dir);
            let _ = events.send(Event::FsInfo(Done { root: key, dir, seq, info, tagable }));
        });
        // corner: a spawn that fails records nothing, so the next ask tries again and answers unknown meanwhile.
        if worker.is_ok() {
            self.inflight.insert(root, (Instant::now(), seq));
        }
    }

    // The figures to print, only for the folder on screen and only when they moved since its last answer.
    pub fn finish(&mut self, done: Done, base: &Path) -> Option<Option<Info>> {
        // A hung statfs landing late never clears the newer one that replaced it.
        if self.inflight.get(&done.root).is_some_and(|(_, seq)| *seq == done.seq) {
            self.inflight.remove(&done.root);
        }
        if self.waiting.remove(&done.root) && base != done.dir && base.starts_with(&done.root) {
            self.refresh(done.root.clone(), base.to_path_buf());
        }
        // A share's tag answer that differs from the one shown is news too, so the line goes out again.
        let moved = self.known.get(&done.dir) != Some(&done.info) || (done.dir == base && done.tagable != self.tagable_shown(base));
        if done.dir == base {
            self.tagged = Some((done.dir.clone(), done.tagable));
        }
        if self.known.len() >= KNOWN_MAX && !self.known.contains_key(&done.dir) {
            self.known.clear();
        }
        self.known.insert(done.dir.clone(), done.info.clone());
        if moved && done.dir == base {
            return Some(done.info);
        }
        None
    }
}

impl FsInfo {
    // Once per folder: a local one is probed here, a share answers its worker's probe or true until it lands.
    pub fn tagable(&mut self, base: &Path, class: &str) -> bool {
        if self.tagged.as_ref().is_some_and(|(dir, _)| dir == base) || matches!(class, "network" | "phone") {
            return self.tagable_shown(base);
        }
        let supported = super::tags::dir_supports(base);
        self.tagged = Some((base.to_path_buf(), supported));
        supported
    }

    // The answer last shown for this folder, true when none is known, as a transient probe error answers.
    pub fn tagable_shown(&self, base: &Path) -> bool {
        !self.tagged.as_ref().is_some_and(|(dir, supported)| dir == base && !supported)
    }
}

// A slow mount is a gvfs share or phone by its path, else a kernel network mount; answer passes a resolved path.
pub fn slow_class(path: &Path) -> &'static str {
    gvfs_class(path).unwrap_or("network")
}

#[cfg(test)]
#[path = "fsinforeq_tests.rs"]
mod tests;
