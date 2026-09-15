// The listed directory, watched so an outside change reaches the client; see docs/protocol.md.
use crate::backend::events::Event;
use crate::json::escape;
use super::watchattrs::AttributeCache;
use std::io;
use std::ffi::{c_char, c_int, c_void, CString, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

// Exactly the events that change what a listing draws; a file still being written is not one.
const IN_ATTRIB: u32 = 0x0000_0004;
const IN_CLOSE_WRITE: u32 = 0x0000_0008;
const IN_MOVED_FROM: u32 = 0x0000_0040;
const IN_MOVED_TO: u32 = 0x0000_0080;
const IN_CREATE: u32 = 0x0000_0100;
const IN_DELETE: u32 = 0x0000_0200;
// Not IN_DELETE_SELF: the watch's own removal is reported whatever the mask holds, measured.
const IN_MOVE_SELF: u32 = 0x0000_0800;
const MASK: u32 = IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_CREATE
    | IN_DELETE
    | IN_MOVE_SELF;

// IN_CLOEXEC is O_CLOEXEC, so no thumbnailer child inherits this descriptor.
const IN_CLOEXEC: c_int = 0x0008_0000;
// One inotify_event is a watch descriptor, a mask, a cookie and a name length, then the name.
const EVENT_HEADER: usize = 16;
// One burst says one thing to a client that re-reads it all, so a thousand writes cost one line.
const COALESCE: Duration = Duration::from_millis(100);
// A batch size and not a limit: the kernel's own drop is at max_queued_events, which this misses.
const BUF: usize = 8192;

// A burst can touch hidden names only, or include an entry the normal listing shows.
#[derive(Debug, PartialEq)]
pub struct Change {
    wd: i32,
    visible: bool,
    hidden: bool,
    attributes: Vec<OsString>,
}

impl Change {
    fn new(wd: i32) -> Self { Self { wd, visible: false, hidden: false, attributes: Vec::new() } }
}

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
}

// One directory at a time plus the one a scan is reading; a negative descriptor is nothing watched.
pub struct Watch {
    fd: c_int,
    wd: c_int,
    incoming: c_int,
    hidden: bool,
    attributes: AttributeCache,
}

impl Watch {
    // A box with no inotify still gets a file manager, as live as 0.1.4's was, and is told once.
    pub fn start(tx: Sender<Event>) -> Watch {
        let fd = unsafe { inotify_init1(IN_CLOEXEC) };
        if fd < 0 {
            eprintln!("flea: the open folder will not follow outside changes, inotify is unavailable");
            return Watch::new(-1);
        }
        thread::spawn(move || pump(fd, tx));
        Watch::new(fd)
    }

    fn new(fd: c_int) -> Self {
        Self { fd, wd: -1, incoming: -1, hidden: false, attributes: AttributeCache::default() }
    }

    // Armed beside the current watch, so a scan that fails costs the open folder nothing.
    pub fn begin(&mut self, path: &Path) {
        self.drop_one(self.incoming);
        self.incoming = self.add(path);
    }

    // A re-list answers the descriptor the folder already had, so dropping it would unwatch it.
    pub fn commit(&mut self, hidden: bool) {
        if self.incoming != self.wd {
            self.drop_one(self.wd);
            self.attributes = AttributeCache::default();
        }
        self.wd = self.incoming;
        self.incoming = -1;
        self.hidden = hidden;
    }

    // The scan failed, so the listing did not move and neither does its watch, aliased or not.
    pub fn abandon(&mut self) {
        if self.incoming != self.wd {
            self.drop_one(self.incoming);
        }
        self.incoming = -1;
    }

    pub fn stop(&mut self) {
        self.drop_one(self.wd);
        if self.incoming != self.wd {
            self.drop_one(self.incoming);
        }
        self.wd = -1;
        self.incoming = -1;
        self.attributes = AttributeCache::default();
    }

    // A directory that cannot be watched is not an error the client can act on: it listed fine.
    fn add(&self, path: &Path) -> c_int {
        if self.fd < 0 {
            return -1;
        }
        match CString::new(path.as_os_str().as_encoded_bytes()) {
            Ok(c) => unsafe { inotify_add_watch(self.fd, c.as_ptr(), MASK) },
            Err(_) => -1,
        }
    }

    fn drop_one(&self, wd: c_int) {
        if self.fd >= 0 && wd >= 0 {
            unsafe { inotify_rm_watch(self.fd, wd) };
        }
    }

    // A directory this box could have watched and did not; no inotify at all is said once at startup.
    pub fn refused(&self) -> bool {
        self.fd >= 0 && self.wd < 0
    }

    // A removed watch's own IN_IGNORED arrives late, so only a matching descriptor is this folder's.
    pub fn is_current(&self, wd: i32) -> bool {
        self.wd >= 0 && wd == self.wd
    }

    pub fn should_refresh(&mut self, change: &Change, directory: &Path) -> bool {
        if !self.is_current(change.wd) { return false; }
        let mut refresh = change.visible || (self.hidden && change.hidden);
        for name in &change.attributes {
            if self.hidden || !name.as_bytes().starts_with(b".") {
                refresh |= self.attributes.changed(directory, name);
            }
        }
        refresh
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
        for change in descriptors(&buf[..n as usize]) {
            if tx.send(Event::Changed(change)).is_err() {
                return;
            }
        }
        thread::sleep(COALESCE);
    }
}

// Each watch once; one visible name in a mixed burst must survive hidden-name filtering.
fn descriptors(buf: &[u8]) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    let mut at = 0;
    while at + EVENT_HEADER <= buf.len() {
        let wd = i32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
        let mask = u32::from_ne_bytes([buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]]);
        let len = u32::from_ne_bytes([buf[at + 12], buf[at + 13], buf[at + 14], buf[at + 15]]) as usize;
        if !out.iter().any(|change| change.wd == wd) { out.push(Change::new(wd)); }
        let change = out.iter_mut().find(|change| change.wd == wd).unwrap();
        let name = buf.get(at + EVENT_HEADER..at + EVENT_HEADER + len).unwrap_or(&[]);
        let name = name.split(|byte| *byte == 0).next().unwrap_or(&[]);
        // An empty attribute name means the watched directory itself; other self-events refresh.
        if mask & MASK == IN_ATTRIB {
            let name = std::ffi::OsStr::from_bytes(name).to_owned();
            if !change.attributes.contains(&name) { change.attributes.push(name); }
        } else if name.is_empty() {
            change.visible = true;
        } else if name.starts_with(b".") {
            change.hidden = true;
        } else {
            change.visible = true;
        }
        // The condition above is the bound that keeps this indexing inside the slice.
        at += EVENT_HEADER + len;
    }
    out
}

// The one unsolicited line: the listed directory is no longer what the listing answered with.
pub fn changed_line(path: &Path) -> String {
    format!(r#"{{"t":"changed","path":"{}"}}"#, escape(&path.to_string_lossy()))
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
