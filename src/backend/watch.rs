// The listed directory, watched so an outside change reaches the client; see docs/protocol.md.
use crate::backend::events::Event;
use crate::backend::inotifyburst::each_event;
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::io;
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;
use crate::backend::events::Sender;
use std::thread;
use std::time::Duration;

// Exactly the events that change what a listing draws; a file still being written is not one.
pub(crate) const IN_ATTRIB: u32 = 0x0000_0004;
pub(crate) const IN_CLOSE_WRITE: u32 = 0x0000_0008;
pub(crate) const IN_MOVED_FROM: u32 = 0x0000_0040;
pub(crate) const IN_MOVED_TO: u32 = 0x0000_0080;
pub(crate) const IN_CREATE: u32 = 0x0000_0100;
pub(crate) const IN_DELETE: u32 = 0x0000_0200;
// Not IN_DELETE_SELF: the watch's own removal is reported whatever the mask holds, measured.
pub(crate) const IN_MOVE_SELF: u32 = 0x0000_0800;
pub(crate) const IN_DELETE_SELF: u32 = 0x0000_0400;
// The mount went away: the pane leaves the volume instead of re-listing a vanished path.
pub(crate) const IN_UNMOUNT: u32 = 0x0000_2000;
// IN_IGNORED on the open watch is the folder going, answered by its path; IN_Q_OVERFLOW arrives on wd -1 and is_current drops it.
pub(crate) const IN_Q_OVERFLOW: u32 = 0x0000_4000;
pub(crate) const IN_MODIFY: u32 = 0x0000_0002;
pub(crate) const IN_IGNORED: u32 = 0x0000_8000;
const MASK: u32 = IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_CREATE
    | IN_DELETE
    | IN_MOVE_SELF;

// IN_CLOEXEC is O_CLOEXEC, so no thumbnailer child inherits this descriptor.
pub(crate) const IN_CLOEXEC: c_int = 0x0008_0000;
pub(crate) const IN_NONBLOCK: c_int = 0x0000_0800;
pub(crate) const IN_DONT_FOLLOW: u32 = 0x0200_0000;
pub(crate) const IN_ONLYDIR: u32 = 0x0100_0000;
// One burst says one thing to a client that re-reads it all, so a thousand writes cost one line.
pub(crate) const COALESCE: Duration = Duration::from_millis(100);
// A batch size and not a limit: the kernel's own drop is at max_queued_events, which this misses.
pub(crate) const BUF: usize = 8192;

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

// The open directory, its expanded children and the one a scan is reading; a negative descriptor is nothing watched.
pub struct Watch {
    outline: Vec<i32>,
    fd: std::sync::OnceLock<c_int>,
    events: Sender<Event>,
    wd: c_int,
    incoming: c_int,
    mount: std::path::PathBuf,
    incoming_mount: std::path::PathBuf,
    // The listed folder and every folder above it, so a delete or move anywhere on its path moves the pane.
    pub(crate) ancestry: ancestry::Ancestry,
}

#[path = "ancestry.rs"]
pub mod ancestry;

// Sample mountinfo: "30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw" keys "/media/stick".
pub(crate) fn live_mount_of(path: &Path) -> std::path::PathBuf {
    super::iomount::mount_key_plain(path, &super::iomount::mount_body())
}

impl Watch {
    // A box with no inotify still gets a file manager, as live as 0.1.4's was, and is told once.
    pub fn start(tx: Sender<Event>) -> Watch {
        let ancestry = ancestry::Ancestry::new(tx.clone());
        Watch { outline: Vec::new(), fd: std::sync::OnceLock::new(), events: tx, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new(), ancestry }
    }

    // Armed beside the current watch once a pending one goes, so a scan that fails costs the open folder nothing.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn begin(&mut self, path: &Path) {
        self.abandon();
        self.set_incoming(self.add(path), live_mount_of(path));
    }

    // The folder and its expanded children land together, so going up or in keeps a descriptor the new listing reuses.
    pub fn commit(&mut self, outline: Vec<i32>) {
        let retired: Vec<c_int> = std::iter::once(self.wd).chain(std::mem::replace(&mut self.outline, outline)).collect();
        self.wd = self.incoming;
        self.incoming = -1;
        if !self.incoming_mount.as_os_str().is_empty() {
            self.mount = std::mem::take(&mut self.incoming_mount);
        }
        retired.into_iter().for_each(|wd| self.retire(wd));
    }

    // The scan failed, so the listing did not move and neither does its watch, aliased or not.
    pub fn abandon(&mut self) {
        self.retire(self.incoming);
        self.incoming = -1;
        self.incoming_mount = std::path::PathBuf::new();
    }

    // A failed scan armed its watch on the worker, so the loop removes that descriptor too; an aliased re-list stays.
    pub fn abandon_wd(&mut self, wd: c_int) {
        self.retire(wd);
        self.abandon();
    }

    // Nothing lands after a stop, so every descriptor goes, the aliased ones included.
    pub fn stop(&mut self) {
        self.abandon();
        self.commit(Vec::new());
        self.mount = std::path::PathBuf::new();
        self.ancestry.forget();
    }

    // Sample input: open "/media/stick/photos" with mount "/media/stick" starts "/media".
    pub fn unmount_start(&self, open: &Path) -> std::path::PathBuf {
        if !self.mount.as_os_str().is_empty() && open.starts_with(&self.mount) {
            if let Some(p) = self.mount.parent() {
                return p.to_path_buf();
            }
        }
        open.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("/"))
    }

    // A directory that cannot be watched is not an error the client can act on: it listed fine.
    fn add(&self, path: &Path) -> c_int {
        let fd = self.raw_fd();
        if fd < 0 {
            return -1;
        }
        match CString::new(path.as_os_str().as_encoded_bytes()) {
            Ok(c) => unsafe { inotify_add_watch(fd, c.as_ptr(), MASK) },
            Err(_) => -1,
        }
    }
    // The listing worker adds the watch, so a dead mount never blocks the loop.
    pub(crate) fn raw_fd(&self) -> c_int {
        *self.fd.get_or_init(|| {
            let fd = unsafe { inotify_init1(IN_CLOEXEC) };
            if fd < 0 {
                eprintln!("flea: the open folder will not follow outside changes, inotify is unavailable");
            } else {
                let tx = self.events.clone();
                thread::spawn(move || pump(fd, tx));
            }
            fd
        })
    }
    // The worker's descriptor becomes the incoming watch the commit takes over; the one it replaces goes only if unused.
    pub(crate) fn set_incoming(&mut self, wd: c_int, mount: std::path::PathBuf) {
        if self.incoming != wd {
            self.retire(self.incoming);
        }
        self.incoming = wd;
        self.incoming_mount = mount;
    }
    // One inotify_add_watch on a worker, never on the loop, keyed by mount outside.
    pub(crate) fn add_raw(fd: c_int, path: &Path) -> c_int {
        if fd < 0 {
            return -1;
        }
        match CString::new(path.as_os_str().as_encoded_bytes()) {
            Ok(c) => unsafe { inotify_add_watch(fd, c.as_ptr(), MASK) },
            Err(_) => -1,
        }
    }
    // inotify answers one descriptor per directory however often it is added, so a descriptor still in use stays.
    fn retire(&self, wd: c_int) {
        if !self.is_current(wd) {
            self.drop_one(wd);
        }
    }
    fn drop_one(&self, wd: c_int) {
        if let Some(&fd) = self.fd.get().filter(|&&fd| fd >= 0 && wd >= 0) {
            unsafe { inotify_rm_watch(fd, wd) };
        }
    }

    // A directory this box could have watched and did not; no inotify at all is said once on first use.
    pub fn refused(&self) -> bool {
        self.fd.get().is_some_and(|&fd| fd >= 0) && self.wd < 0
    }

    // A removed watch's own IN_IGNORED arrives late, so only a matching descriptor is the listing's, open folder or expanded child.
    pub fn is_current(&self, wd: i32) -> bool {
        wd >= 0 && (wd == self.wd || self.outline.contains(&wd))
    }

    // Only the open folder's own descriptor moves the pane when its volume goes; a child's re-lists it.
    pub fn is_open_folder(&self, wd: i32) -> bool {
        wd >= 0 && wd == self.wd
    }
}

// Sample input: wd 1, mask 0x00000100, cookie 0, len 16, then "NEWFILE.txt\0\0\0\0\0".
fn pump(fd: c_int, tx: Sender<Event>) {
    let mut buf = [0u8; BUF];
    loop {
        let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, BUF) };
        if n < 0 {
            let failure = io::Error::last_os_error();
            // A signal can cut a blocking read short, which is not the descriptor going away.
            if failure.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            eprintln!("flea: the open folder stopped following outside changes, the inotify read failed: {}", failure);
            return;
        }
        // A closed descriptor ends the thread; the loop keeps running without a watch.
        if n == 0 {
            return;
        }
        let (changed, unmounted, vanished) = classify_burst(&buf[..n as usize]);
        let events = changed.into_iter().map(Event::Changed)
            .chain(unmounted.into_iter().map(Event::Unmounted))
            .chain(vanished.into_iter().map(Event::Vanished));
        for event in events {
            if tx.send(event).is_err() {
                return;
            }
        }
        thread::sleep(COALESCE);
    }
}

// Sample input: IN_CREATE on wd 3, IN_UNMOUNT on wd 4 and IN_DELETE_SELF on wd 5 answer ([3], [4], [5]).
pub(crate) fn classify_burst(buf: &[u8]) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let (mut changed, mut unmounted, mut vanished) = (Vec::new(), Vec::new(), Vec::new());
    each_event(buf, |wd, mask| {
        let into = if mask & IN_UNMOUNT != 0 {
            &mut unmounted
        } else if mask & (IN_DELETE_SELF | IN_MOVE_SELF | IN_IGNORED) != 0 {
            // The watched folder itself went or moved, which the loop answers by its path and not by a re-list.
            &mut vanished
        } else if mask & (MASK | IN_Q_OVERFLOW) != 0 {
            &mut changed
        } else {
            return;
        };
        if !into.contains(&wd) {
            into.push(wd);
        }
    });
    // A vanished path never answers changed: IN_IGNORED follows IN_UNMOUNT on the same wd.
    vanished.retain(|wd| !unmounted.contains(wd));
    changed.retain(|wd| !unmounted.contains(wd) && !vanished.contains(wd));
    (changed, unmounted, vanished)
}

// Which watches this burst touched, each once; nothing past the descriptor is ever read.
#[cfg(test)]
fn descriptors(buf: &[u8]) -> Vec<i32> {
    classify_burst(buf).0
}

// The one unsolicited line: the listed directory is no longer what the listing answered with.
pub fn changed_line(path: &Path) -> String {
    format!(r#"{{"t":"changed","path":"{}"}}"#, escape(&path.exact_text()))
}

// Sample output: {"t":"unmounted","path":"/media/stick","parent":"/media"}; parent is nearest existing.
pub fn unmounted_line(path: &Path, parent: &Path) -> String {
    format!(r#"{{"t":"unmounted","path":"{}","parent":"{}"}}"#, escape(&path.exact_text()), escape(&parent.exact_text()))
}

// Sample output: {"t":"gone","path":"/w/p/open-me","parent":"/w"}; the folder was deleted or moved away, parent is nearest existing.
pub fn gone_line(path: &Path, parent: &Path) -> String {
    format!(r#"{{"t":"gone","path":"{}","parent":"{}"}}"#, escape(&path.exact_text()), escape(&parent.exact_text()))
}

// Climbs until exists() holds, so an ejected stick lands on its mountpoint's parent, never /.
pub fn nearest_parent(path: &Path, exists: impl Fn(&Path) -> bool) -> std::path::PathBuf {
    let mut at = path.to_path_buf();
    loop {
        if exists(&at) {
            return at;
        }
        match at.parent() {
            Some(p) if p != at => at = p.to_path_buf(),
            _ => return std::path::PathBuf::from("/"),
        }
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
