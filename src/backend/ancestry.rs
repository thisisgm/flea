// The open folder and every folder above it, watched for their own delete or move so the pane leaves a folder that is no longer there; see docs/protocol.md "gone".
use crate::backend::events::{Event, Sender};
use crate::backend::inotifyburst::each_event;
use crate::backend::watch::{
    nearest_parent, BUF, COALESCE, IN_CLOEXEC, IN_DELETE_SELF, IN_MOVE_SELF, IN_ONLYDIR,
};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CString};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, OnceLock};
use std::thread;

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

// Only a folder's own delete or move; what happens inside the open folder is the listing watch's.
const MASK: u32 = IN_DELETE_SELF | IN_MOVE_SELF | IN_ONLYDIR;

// Sample: `rm -r /w/parent` with /w/parent/open-me open sends Gone { path: "/w/parent/open-me", parent: "/w" }.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gone {
    pub path: PathBuf,
    pub parent: PathBuf,
}

enum Ask {
    Follow(u64, Sender<Event>, PathBuf),
    // A client's own check comes from its listing watch; None is a folder above some client moving.
    Check(Option<u64>),
    Leave(u64),
}

// One per backend loop; every one in the process shares one inotify instance and one thread, so a backend costs one instance more, not one per listing.
#[derive(Default)]
pub struct Ancestry {
    client: u64,
    events: Option<Sender<Event>>,
    // The last gone line: the open folder was said once, and only the folder it sent the client to failing too is said anew.
    leaving: Option<Gone>,
}

impl Ancestry {
    pub fn new(events: Sender<Event>) -> Ancestry {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Ancestry {
            client: NEXT.fetch_add(1, Ordering::Relaxed),
            events: Some(events),
            leaving: None,
        }
    }

    // A landed listing: its folder and every folder above it are followed from now on.
    pub fn follow(&mut self, base: &Path) {
        self.leaving = None;
        if let Some(events) = &self.events {
            ask(Ask::Follow(self.client, events.clone(), base.to_path_buf()));
        }
    }

    // The listing watch saw the open folder itself go or move; the follower decides off the loop.
    pub fn check(&self) {
        if self.events.is_some() {
            ask(Ask::Check(Some(self.client)));
        }
    }

    // Nothing is followed until the next listing lands: a search, a path list or a volume that went.
    pub fn forget(&mut self) {
        self.leaving = None;
        if self.events.is_some() {
            ask(Ask::Leave(self.client));
        }
    }

    // Sample: open "/w/p/o", nothing said yet: "/w/p/o" is owed; after a gone line to "/w/p", only "/w/p" is.
    pub fn owes(&self, path: &Path, base: &Path) -> bool {
        match &self.leaving {
            None => path == base,
            Some(gone) => path == gone.parent,
        }
    }

    pub fn left_for(&mut self, gone: &Gone) {
        self.leaving = Some(gone.clone());
    }

    // Sample: after a gone line for "/w/p/o" to "/w", a list of "/w/p", "/w/p/o" or "/w/p/o/sub" the client sent before reading it gets that line again.
    pub fn crossed(&self, path: &Path) -> Option<&Gone> {
        self.leaving.as_ref().filter(|gone| {
            let first = gone.path.strip_prefix(&gone.parent).ok();
            let first = first.and_then(|tail| tail.components().next());
            first.is_some_and(|first| path.starts_with(gone.parent.join(first)))
        })
    }
}

impl Drop for Ancestry {
    fn drop(&mut self) {
        if self.events.is_some() {
            ask(Ask::Leave(self.client));
        }
    }
}

fn ask(ask: Ask) {
    static ASKS: OnceLock<mpsc::Sender<Ask>> = OnceLock::new();
    let _ = ASKS.get_or_init(start).send(ask);
}

// Without inotify the follower still answers the listing watch's own checks, so only a moved folder above is missed.
fn start() -> mpsc::Sender<Ask> {
    let (asks, queue) = mpsc::channel();
    let fd = unsafe { inotify_init1(IN_CLOEXEC) };
    if fd >= 0 {
        let wake = asks.clone();
        thread::spawn(move || pump(fd, wake));
    } else {
        eprintln!("flea: a folder moved or deleted above the open one will not be followed, inotify is unavailable");
    }
    thread::spawn(move || {
        Follower {
            fd,
            clients: HashMap::new(),
            refs: HashMap::new(),
        }
        .run(queue)
    });
    asks
}

// Which watch fired is not read: any folder above any client moving makes every client check its own path.
fn pump(fd: c_int, wake: mpsc::Sender<Ask>) {
    let mut buf = [0u8; BUF];
    loop {
        let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, BUF) };
        if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if n < 0 {
            eprintln!("flea: folders above the open one stopped being followed, the inotify read failed: {}", io::Error::last_os_error());
        }
        if n <= 0 {
            return;
        }
        let mut moved = false;
        each_event(&buf[..n as usize], |_, mask| {
            moved |= mask & (IN_DELETE_SELF | IN_MOVE_SELF) != 0
        });
        if moved && wake.send(Ask::Check(None)).is_err() {
            return;
        }
        thread::sleep(COALESCE);
    }
}

// Sample: "/w/a" answers its device and inode while it is a folder, None once nothing or a file answers there.
pub(crate) fn identity(dir: &Path) -> Option<(u64, u64)> {
    std::fs::metadata(dir)
        .ok()
        .filter(|meta| meta.is_dir())
        .map(|meta| (meta.dev(), meta.ino()))
}

// What a check finds at a followed path: nothing new, another folder under the same name, or nothing there.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Same,
    Replaced,
    Gone(PathBuf),
}

// Sample: "/w/p/open-me" recorded, then `mv /w/p /x` answers Gone("/w").
// A listed folder nothing could stat (a gvfs path gio reads past a disconnected FUSE mount) is left to its own re-list.
pub(crate) fn verdict(base: &Path, recorded: Option<(u64, u64)>) -> Verdict {
    match (identity(base), recorded) {
        (Some(now), Some(was)) if now == was => Verdict::Same,
        (Some(_), _) => Verdict::Replaced,
        (None, None) => Verdict::Same,
        (None, Some(_)) => Verdict::Gone(nearest_parent(base, |p| p.is_dir())),
    }
}

struct Client {
    events: Sender<Event>,
    base: PathBuf,
    id: Option<(u64, u64)>,
    wds: Vec<c_int>,
}

struct Follower {
    fd: c_int,
    clients: HashMap<u64, Client>,
    // inotify answers one descriptor per folder, so a folder two clients sit under is removed only when neither does.
    refs: HashMap<c_int, usize>,
}

impl Follower {
    fn run(mut self, queue: mpsc::Receiver<Ask>) {
        for ask in queue {
            match ask {
                Ask::Follow(client, events, base) => {
                    self.follow(client, events, base);
                    self.settle(client, false);
                }
                Ask::Check(Some(client)) => self.settle(client, true),
                Ask::Check(None) => {
                    let clients: Vec<u64> = self.clients.keys().copied().collect();
                    clients
                        .into_iter()
                        .for_each(|client| self.settle(client, false));
                }
                Ask::Leave(client) => self.leave(client),
            }
        }
    }

    // The new folders are armed before the old ones go, so a folder both paths share keeps its watch throughout.
    fn follow(&mut self, client: u64, events: Sender<Event>, base: PathBuf) {
        let wds = self.arm(&base);
        self.leave(client);
        let id = identity(&base);
        self.clients.insert(
            client,
            Client {
                events,
                base,
                id,
                wds,
            },
        );
    }

    // A listing watch's own check always answers: the open folder went, or the client re-lists it as before.
    fn settle(&mut self, client: u64, asked: bool) {
        let Some(c) = self.clients.get(&client) else {
            return;
        };
        let (events, base) = (c.events.clone(), c.base.clone());
        match verdict(&base, c.id) {
            Verdict::Same => {
                if asked {
                    let _ = events.send(Event::PollChanged(base));
                }
            }
            Verdict::Replaced => {
                self.follow(client, events.clone(), base.clone());
                let _ = events.send(Event::PollChanged(base));
            }
            Verdict::Gone(parent) => {
                // Said once; a parent that is gone too by the time the client lists it answers that list with gone in turn.
                let _ = events.send(Event::Gone(Gone { path: base, parent }));
                self.leave(client);
            }
        }
    }

    // The folder and each folder above it, never "/", which nothing deletes or moves.
    fn arm(&mut self, base: &Path) -> Vec<c_int> {
        let mut wds = Vec::new();
        if self.fd < 0 {
            return wds;
        }
        for dir in base.ancestors().filter(|dir| dir.parent().is_some()) {
            let Ok(c) = CString::new(dir.as_os_str().as_encoded_bytes()) else {
                continue;
            };
            let wd = unsafe { inotify_add_watch(self.fd, c.as_ptr(), MASK) };
            if wd < 0 {
                eprintln!(
                    "flea: {} will not be followed if it moves, inotify refused a watch on it: {}",
                    dir.display(),
                    io::Error::last_os_error()
                );
                continue;
            }
            if !wds.contains(&wd) {
                *self.refs.entry(wd).or_insert(0) += 1;
                wds.push(wd);
            }
        }
        wds
    }

    fn leave(&mut self, client: u64) {
        let Some(gone) = self.clients.remove(&client) else {
            return;
        };
        for wd in gone.wds {
            let Some(count) = self.refs.get_mut(&wd) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                self.refs.remove(&wd);
                unsafe { inotify_rm_watch(self.fd, wd) };
            }
        }
    }
}

#[cfg(test)]
#[path = "ancestry_tests.rs"]
mod tests;
