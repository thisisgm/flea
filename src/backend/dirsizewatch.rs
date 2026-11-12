// Settled folder sizes by path, trusted only while every folder each one counted stays watched and unchanged.
use super::dirsize::{walk_display_armed, DirSize};
use super::inotifyburst::each_event;
use super::watch::{
    BUF, IN_CLOEXEC, IN_CREATE, IN_DELETE, IN_DELETE_SELF, IN_DONT_FOLLOW, IN_MODIFY,
    IN_MOVED_FROM, IN_MOVED_TO, IN_MOVE_SELF, IN_ONLYDIR, IN_Q_OVERFLOW,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Instant;

// A write, a truncate, an entry made, removed or renamed, or the folder itself moving: anything that moves a recursive size.
const MASK: u32 = IN_MODIFY
    | IN_CREATE
    | IN_DELETE
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_DELETE_SELF
    | IN_MOVE_SELF
    | IN_ONLYDIR
    | IN_DONT_FOLLOW;
// One kernel watch per counted folder, across every size held; a walk that would pass this is walked again on every return.
pub(crate) const CAP: usize = 8192;
// No instance yet; -1 is one this box refused.
const UNOPENED: c_int = -2;

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

struct Folder {
    pins: usize,
    changed: bool,
}

struct Inner {
    fd: c_int,
    folders: HashMap<c_int, Folder>,
}

impl Inner {
    // The first change ends a folder's watch at once, so a busy tree costs one event and not a stream.
    fn changed(&mut self, wd: c_int) {
        if let Some(folder) = self.folders.get_mut(&wd) {
            if !folder.changed {
                folder.changed = true;
                if self.fd >= 0 {
                    unsafe { inotify_rm_watch(self.fd, wd) };
                }
            }
        }
    }

    // An overflow or a dead descriptor has lost events, so every held folder counts as changed.
    fn all_changed(&mut self) {
        let wds: Vec<c_int> = self.folders.keys().copied().collect();
        for wd in wds {
            self.changed(wd);
        }
    }
}

// One inotify instance per backend, shared by the size worker that arms it and the loop that reads its pins.
#[derive(Clone)]
pub struct TreeWatch(Arc<Mutex<Inner>>);

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl TreeWatch {
    pub fn new() -> TreeWatch {
        TreeWatch(Arc::new(Mutex::new(Inner {
            fd: UNOPENED,
            folders: HashMap::new(),
        })))
    }

    pub fn pins(&self) -> Pins {
        Pins {
            watch: Some(self.clone()),
            wds: Vec::new(),
            whole: true,
            root: None,
        }
    }

    // The walk calls this before it reads `dir`, so any change after that read fires.
    pub fn arm(&self, pins: &mut Pins, dir: &Path) {
        if !pins.whole {
            return;
        }
        let mut inner = lock(&self.0);
        if inner.fd == UNOPENED {
            inner.fd = open(&self.0);
        }
        let wd = match CString::new(dir.as_os_str().as_encoded_bytes()) {
            Ok(path) if inner.fd >= 0 && inner.folders.len() < CAP => unsafe {
                inotify_add_watch(inner.fd, path.as_ptr(), MASK)
            },
            _ => -1,
        };
        let folder = (wd >= 0).then(|| {
            inner.folders.entry(wd).or_insert(Folder {
                pins: 0,
                changed: false,
            })
        });
        match folder {
            Some(folder) if !folder.changed => {
                folder.pins += 1;
                pins.wds.push(wd);
            }
            _ => pins.whole = false,
        }
    }
}

// The size worker's walk: the answer walk_cancellable gives, with a pin on every folder it read.
pub fn walk_watched(
    path: &Path,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
    watch: &TreeWatch,
) -> (DirSize, Pins) {
    let pins = RefCell::new(watch.pins());
    if cancelled() {
        return (
            DirSize {
                bytes: 0,
                partial: true,
            },
            pins.into_inner(),
        );
    }
    let arm = |_: &Path, held: &Path| {
        let mut pins = pins.borrow_mut();
        // The walk has checked this inode before arming it, so no earlier name probe can trigger an automount.
        if pins.root.is_none() {
            pins.root = identity(held);
        }
        watch.arm(&mut pins, held);
    };
    let stop = || cancelled() || Instant::now() >= deadline;
    let (size, linked) = walk_display_armed(path, &stop, &super::iomount::mount_body(), Some(&arm));
    let mut pins = pins.into_inner();
    // A write fires only the folder of the name it went through, so a file named outside the tree keeps nothing; see "Deliberate corners".
    if linked {
        pins.whole = false;
    }
    (size, pins)
}

// The pump owns the descriptor and holds only a weak handle, so it ends at its first wake after the last handle goes.
fn open(shared: &Arc<Mutex<Inner>>) -> c_int {
    let fd = unsafe { inotify_init1(IN_CLOEXEC) };
    if fd < 0 {
        return -1;
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let weak = Arc::downgrade(shared);
    match std::thread::Builder::new()
        .name("flea-sizewatch".into())
        .spawn(move || pump(owned, weak))
    {
        Ok(_) => fd,
        Err(_) => -1,
    }
}

fn pump(fd: OwnedFd, shared: Weak<Mutex<Inner>>) {
    let mut buf = [0u8; BUF];
    loop {
        let n = unsafe { read(fd.as_raw_fd(), buf.as_mut_ptr() as *mut c_void, BUF) };
        if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        let Some(shared) = shared.upgrade() else {
            return;
        };
        let mut inner = lock(&shared);
        if n <= 0 {
            inner.all_changed();
            inner.fd = -1;
            return;
        }
        each_event(&buf[..n as usize], |wd, mask| {
            if mask & IN_Q_OVERFLOW != 0 {
                inner.all_changed();
            } else {
                inner.changed(wd);
            }
        });
    }
}

// The watches one walk armed, one per folder it read; an empty set never armed its own start.
#[derive(Default)]
pub struct Pins {
    watch: Option<TreeWatch>,
    wds: Vec<c_int>,
    whole: bool,
    // The walked folder's (device, inode), since a rename above it fires no watch below.
    root: Option<(u64, u64)>,
}

fn identity(path: &Path) -> Option<(u64, u64)> {
    let meta = super::dirsize::stat::at(path)?;
    Some((meta.dev, meta.ino))
}

impl Pins {
    // Every counted folder watched from before its read, and none changed since.
    pub fn fresh(&self) -> bool {
        let Some(watch) = &self.watch else {
            return false;
        };
        if !self.whole || self.wds.is_empty() {
            return false;
        }
        let inner = lock(&watch.0);
        self.wds
            .iter()
            .all(|wd| inner.folders.get(wd).is_some_and(|folder| !folder.changed))
    }
}

impl Drop for Pins {
    fn drop(&mut self) {
        let Some(watch) = &self.watch else { return };
        let mut inner = lock(&watch.0);
        for wd in self.wds.drain(..) {
            let Some(folder) = inner.folders.get_mut(&wd) else {
                continue;
            };
            folder.pins -= 1;
            if folder.pins == 0 {
                let changed = folder.changed;
                inner.folders.remove(&wd);
                if !changed && inner.fd >= 0 {
                    unsafe { inotify_rm_watch(inner.fd, wd) };
                }
            }
        }
    }
}

// The loop's settled sizes by folder path, so a re-list of an unchanged folder answers without a walk.
#[derive(Default)]
pub struct Settled {
    sizes: HashMap<PathBuf, (u64, Pins)>,
    keep: Vec<PathBuf>,
}

impl Settled {
    // A size answers only while its pins are fresh and the path still names the folder it walked.
    pub fn get(&mut self, path: &Path) -> Option<u64> {
        let (bytes, pins) = self.sizes.get(path)?;
        if pins.fresh() && pins.root.is_some_and(|root| identity(path) == Some(root)) {
            return Some(*bytes);
        }
        self.sizes.remove(path);
        None
    }

    // Only a whole walk with every folder watched is kept; anything else gives its watches back here.
    pub fn settle(&mut self, path: PathBuf, bytes: u64, pins: Pins) {
        if pins.fresh() {
            self.sizes.insert(path, (bytes, pins));
        }
    }

    // The folders the client's other tabs show, named by its list request.
    pub fn keep(&mut self, tabs: Vec<PathBuf>) {
        self.keep = tabs;
    }

    // A new listing keeps the folders it and the other tabs list; the rest give their watches back.
    pub fn listed(&mut self, base: &Path) {
        let keep = &self.keep;
        self.sizes.retain(|path, (_, pins)| {
            path.parent()
                .is_some_and(|parent| parent == base || keep.iter().any(|tab| tab == parent))
                && pins.fresh()
        });
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.sizes.len()
    }
}

#[cfg(test)]
#[path = "dirsizewatch_tests.rs"]
mod tests;
