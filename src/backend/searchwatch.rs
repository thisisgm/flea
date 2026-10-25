// A search session owns its recursive inotify instance and joins its pump before closing either descriptor.
use super::events::Event;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::{c_char, c_int, c_void, CString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::backend::events::Sender;
use std::sync::{Arc, Mutex};

pub(crate) const IGNORED: u32 = 0x8000;
pub(crate) const OVERFLOW: u32 = 0x4000;
const UNMOUNT: u32 = 0x2000;
const DELETE_SELF: u32 = 0x400;
const MOVE_SELF: u32 = 0x800;
const MOVED_FROM: u32 = 0x40;
const MOVED_TO: u32 = 0x80;
const ISDIR: u32 = 0x4000_0000;
const MASK: u32 = 0x100 | 0x200 | MOVED_FROM | MOVED_TO | 0x4 | 0x8 | DELETE_SELF | MOVE_SELF | UNMOUNT;
const FLAGS: c_int = 0x80000 | 0x800;
static NEXT: AtomicUsize = AtomicUsize::new(1);

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, name: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn eventfd(init: u32, flags: c_int) -> c_int;
    fn read(fd: c_int, bytes: *mut c_void, count: usize) -> isize;
    fn write(fd: c_int, bytes: *const c_void, count: usize) -> isize;
}

#[derive(Default)]
pub(crate) struct Delta {
    pub dirs: BTreeSet<PathBuf>,
    pub resets: BTreeSet<PathBuf>,
    pub overflow: bool,
    pub failed: bool,
}

struct State {
    fd: Option<OwnedFd>,
    dirs: HashMap<c_int, PathBuf>,
    paths: BTreeMap<PathBuf, c_int>,
    retired: HashSet<c_int>,
    delta: Delta,
    signalled: bool,
}

pub(crate) struct Core {
    root: PathBuf,
    hidden: bool,
    state: Mutex<State>,
    reader: Mutex<()>,
    pub token: usize,
    events: Sender<Event>,
    stopped: AtomicBool,
    unmounted: AtomicBool,
    #[cfg(test)]
    pub(crate) refuse: AtomicBool,
}

pub(crate) struct Watch {
    pub core: Arc<Core>,
    stop: Option<OwnedFd>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Watch {
    pub(crate) fn new(root: &Path, hidden: bool, events: Sender<Event>) -> Watch {
        let fd = unsafe { inotify_init1(FLAGS) };
        let stop = unsafe { eventfd(0, FLAGS) };
        let fd = (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) });
        let stop = (stop >= 0).then(|| unsafe { OwnedFd::from_raw_fd(stop) });
        let failed = fd.is_none() || stop.is_none();
        let core = Arc::new(Core {
            root: root.to_path_buf(), hidden, token: NEXT.fetch_add(1, Ordering::Relaxed), events,
            stopped: AtomicBool::new(false), unmounted: AtomicBool::new(false), reader: Mutex::new(()),
            state: Mutex::new(State { fd, dirs: HashMap::new(), paths: BTreeMap::new(), retired: HashSet::new(),
                delta: Delta { failed, ..Delta::default() }, signalled: false }),
            #[cfg(test)]
            refuse: AtomicBool::new(false),
        });
        let mut watch = Watch { core, stop, handle: None };
        if !failed {
            watch.core.arm(Path::new(""));
            let core = Arc::clone(&watch.core);
            let fd = core.state.lock().unwrap().fd.as_ref().unwrap().as_raw_fd();
            let stop = watch.stop.as_ref().unwrap().as_raw_fd();
            watch.handle = Some(std::thread::spawn(move || super::searchwatchpump::pump(core, fd, stop)));
        }
        watch
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.core.stopped.store(true, Ordering::Release);
        if let Some(stop) = &self.stop {
            let value = 1u64;
            unsafe { write(stop.as_raw_fd(), (&value as *const u64).cast(), std::mem::size_of::<u64>()); }
        }
        if let Some(handle) = self.handle.take() { let _ = handle.join(); }
        let mut state = self.core.state.lock().unwrap();
        state.fd.take();
        state.paths.clear();
        state.dirs.clear();
    }
}

impl Core {
    // Remote directories are snapshots, never recursive inotify registrations, including patch and replacement hooks.
    pub(crate) fn arm_local(&self, rel: &Path, body: &str) -> bool {
        if self.stopped.load(Ordering::Acquire) { return false; }
        if super::iomount::is_remote(&self.root.join(rel), body) {
            self.forget(rel);
            return true;
        }
        self.arm(rel)
    }

    pub(crate) fn unmounted(&self) -> bool {
        self.unmounted.load(Ordering::Acquire)
    }

    // Registration and decoding share a lock, so no reader resolves a new descriptor through an old path.
    pub(crate) fn arm(&self, rel: &Path) -> bool {
        if self.stopped.load(Ordering::Acquire) { return false; }
        let mut state = self.state.lock().unwrap();
        if state.paths.contains_key(rel) { return true; }
        #[cfg(test)]
        if self.refuse.load(Ordering::Relaxed) {
            state.delta.failed = true;
            state.delta.dirs.insert(rel.to_path_buf());
            return false;
        }
        let path = CString::new(self.root.join(rel).as_os_str().as_bytes());
        let valid_path = path.is_ok();
        let fd = state.fd.as_ref().map(AsRawFd::as_raw_fd);
        let wd = match (fd, path) {
            (Some(fd), Ok(path)) => unsafe { inotify_add_watch(fd, path.as_ptr(), MASK | 0x0100_0000 | 0x0200_0000) },
            _ => -1,
        };
        if wd < 0 {
            if fd.is_some() && valid_path && !rel.as_os_str().is_empty()
                && matches!(std::io::Error::last_os_error().raw_os_error(), Some(2 | 20 | 40)) { return true; }
            state.delta.failed = true;
            state.delta.dirs.insert(rel.to_path_buf());
            return false;
        }
        if let Some(old) = state.dirs.insert(wd, rel.to_path_buf()) { state.paths.remove(&old); }
        state.paths.insert(rel.to_path_buf(), wd);
        true
    }

    // Retired descriptors ignore buffered payloads until their IN_IGNORED barrier, including a reused integer.
    pub(crate) fn forget(&self, rel: &Path) {
        let mut state = self.state.lock().unwrap();
        let gone: Vec<_> = state.paths.range(rel.to_path_buf()..).take_while(|(path, _)| path.starts_with(rel)).map(|(path, _)| path.clone()).collect();
        for path in gone {
            if let Some(wd) = state.paths.remove(&path) {
                state.dirs.remove(&wd);
                if let Some(fd) = &state.fd {
                    if unsafe { inotify_rm_watch(fd.as_raw_fd(), wd) } == 0 { state.retired.insert(wd); }
                }
            }
        }
    }

    fn signal(&self, state: &mut State) {
        if !state.signalled {
            state.signalled = true;
            let _ = self.events.send(Event::SearchDelta(self.token));
        }
    }

    // Landing synchronously drains the kernel queue, including a burst the pump has already read.
    pub(crate) fn flush(&self) {
        let _reader = self.reader.lock().unwrap();
        let Some(fd) = self.state.lock().unwrap().fd.as_ref().map(AsRawFd::as_raw_fd) else { return };
        let mut bytes = [0u8; 65536];
        for _ in 0..16 {
            let count = unsafe { read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
            if count > 0 { self.ingest(&bytes[..count as usize]); continue; }
            if count < 0 {
                match std::io::Error::last_os_error().kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    std::io::ErrorKind::WouldBlock => return,
                    _ => {}
                }
            }
            self.failed();
            return;
        }
    }

    pub(crate) fn drain(&self) -> Delta {
        let mut state = self.state.lock().unwrap();
        state.signalled = false;
        let mut delta = std::mem::take(&mut state.delta);
        if delta.overflow {
            delta.dirs.extend(state.paths.keys().cloned());
            delta.dirs.insert(PathBuf::new());
        }
        delta
    }

    // A burst pairs move cookies and records raw relative paths before any subscriber is woken.
    pub(crate) fn ingest(&self, bytes: &[u8]) {
        let mut state = self.state.lock().unwrap();
        let mut offset = 0;
        let mut moves = HashMap::new();
        while offset + 16 <= bytes.len() {
            let wd = i32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let mask = u32::from_ne_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
            let cookie = u32::from_ne_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
            let len = u32::from_ne_bytes(bytes[offset + 12..offset + 16].try_into().unwrap()) as usize;
            let Some(end) = offset.checked_add(16).and_then(|at| at.checked_add(len)).filter(|&end| end <= bytes.len()) else {
                state.delta.overflow = true;
                break;
            };
            let name = &bytes[offset + 16..end];
            let name = &name[..name.iter().position(|&byte| byte == 0).unwrap_or(name.len())];
            offset = end;
            if mask & OVERFLOW != 0 { state.delta.overflow = true; continue; }
            if mask & IGNORED != 0 && state.retired.remove(&wd) { continue; }
            if state.retired.contains(&wd) { continue; }
            let Some(rel) = state.dirs.get(&wd).cloned() else { continue };
            if mask & UNMOUNT != 0 { self.unmounted.store(true, Ordering::Release); }
            if mask & (IGNORED | UNMOUNT) != 0 {
                state.dirs.remove(&wd);
                state.paths.remove(&rel);
                if rel.as_os_str().is_empty() || mask & UNMOUNT != 0 { state.delta.failed = true; }
            }
            if mask & (DELETE_SELF | MOVE_SELF | IGNORED | UNMOUNT) != 0 {
                state.delta.resets.insert(rel.clone());
                state.delta.dirs.insert(rel.parent().unwrap_or(Path::new("")).to_path_buf());
                if rel.as_os_str().is_empty() { state.delta.failed = true; }
                continue;
            }
            if !self.hidden && name.first() == Some(&b'.') { continue; }
            state.delta.dirs.insert(rel.clone());
            if mask & ISDIR != 0 && mask & (0x100 | 0x200 | MOVED_FROM | MOVED_TO) != 0 {
                state.delta.resets.insert(rel.join(std::ffi::OsStr::from_bytes(name)));
            }
            if mask & (MOVED_FROM | MOVED_TO) != 0 {
                if cookie != 0 && mask & MOVED_FROM != 0 { moves.insert(cookie, rel); }
                if cookie != 0 && mask & MOVED_TO != 0 {
                    if let Some(from) = moves.remove(&cookie) { state.delta.dirs.insert(from); }
                }
            }
        }
        if offset != bytes.len() { state.delta.overflow = true; }
        if !state.delta.dirs.is_empty() || state.delta.failed || state.delta.overflow { self.signal(&mut state); }
    }

    pub(crate) fn failed(&self) {
        let mut state = self.state.lock().unwrap();
        state.delta.failed = true;
        self.signal(&mut state);
    }
}

#[cfg(test)]
#[path = "searchwatch_tests.rs"]
mod tests;
