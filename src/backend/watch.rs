// The listed directory, watched so an outside change reaches the client; see docs/protocol.md.
use crate::backend::events::Event;
use crate::json::escape;
use std::io;
use std::ffi::{c_char, c_int, c_void, CString};
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
}

impl Watch {
    // A box with no inotify still gets a file manager, as live as 0.1.4's was, and is told once.
    pub fn start(tx: Sender<Event>) -> Watch {
        let fd = unsafe { inotify_init1(IN_CLOEXEC) };
        if fd < 0 {
            eprintln!("flea: the open folder will not follow outside changes, inotify is unavailable");
            return Watch { fd: -1, wd: -1, incoming: -1 };
        }
        thread::spawn(move || pump(fd, tx));
        Watch { fd, wd: -1, incoming: -1 }
    }

    // Armed beside the current watch, so a scan that fails costs the open folder nothing.
    pub fn begin(&mut self, path: &Path) {
        self.drop_one(self.incoming);
        self.incoming = self.add(path);
    }

    // A re-list answers the descriptor the folder already had, so dropping it would unwatch it.
    pub fn commit(&mut self) {
        if self.incoming != self.wd {
            self.drop_one(self.wd);
        }
        self.wd = self.incoming;
        self.incoming = -1;
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
        for wd in descriptors(&buf[..n as usize]) {
            if tx.send(Event::Changed(wd)).is_err() {
                return;
            }
        }
        thread::sleep(COALESCE);
    }
}

// Which watches this burst touched, each once; nothing past the descriptor is ever read.
fn descriptors(buf: &[u8]) -> Vec<i32> {
    let mut out: Vec<i32> = Vec::new();
    let mut at = 0;
    while at + EVENT_HEADER <= buf.len() {
        let wd = i32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
        let len = u32::from_ne_bytes([buf[at + 12], buf[at + 13], buf[at + 14], buf[at + 15]]) as usize;
        if !out.contains(&wd) {
            out.push(wd);
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
mod tests {
    use super::*;

    // Sample input: two events on watch 3, one carrying a 16 byte name and one carrying none.
    fn event(wd: i32, name: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&wd.to_ne_bytes());
        out.extend_from_slice(&IN_CREATE.to_ne_bytes());
        out.extend_from_slice(&0u32.to_ne_bytes());
        out.extend_from_slice(&(name.len() as u32).to_ne_bytes());
        out.extend_from_slice(name);
        out
    }

    #[test]
    fn one_event_names_its_watch() {
        assert_eq!(descriptors(&event(3, b"a.txt\0\0\0")), vec![3]);
    }

    #[test]
    fn a_burst_names_each_watch_once() {
        let mut buf = event(3, b"a.txt\0\0\0");
        buf.extend(event(3, b""));
        buf.extend(event(4, b"b.txt\0\0\0"));
        assert_eq!(descriptors(&buf), vec![3, 4]);
    }

    // Not a shape inotify produces: it pins the bound, so a length running past the slice cannot panic.
    #[test]
    fn a_truncated_tail_ends_the_walk() {
        let mut buf = event(3, b"a.txt\0\0\0");
        buf.extend(event(4, b"this name did not fit"));
        buf.truncate(buf.len() - 4);
        assert_eq!(descriptors(&buf), vec![3, 4]);
    }

    #[test]
    fn a_short_buffer_names_nothing() {
        assert_eq!(descriptors(&[0u8; 8]), Vec::<i32>::new());
    }

    #[test]
    fn nothing_is_current_before_a_directory_is_followed() {
        let w = Watch { fd: -1, wd: -1, incoming: -1 };
        assert!(!w.is_current(-1));
        assert!(!w.is_current(1));
    }

    // O_NONBLOCK, so a watch this test killed fails it by answering nothing rather than by hanging.
    const IN_NONBLOCK: c_int = 0x800;

    // Sample input: wd 1, mask 0x00000100, cookie 0, len 16, then "NEWFILE.txt\0\0\0\0\0".
    fn carries_a_create(buf: &[u8], wd: c_int) -> bool {
        let mut at = 0;
        while at + EVENT_HEADER <= buf.len() {
            let this = i32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
            let mask = u32::from_ne_bytes([buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]]);
            let len = u32::from_ne_bytes([buf[at + 12], buf[at + 13], buf[at + 14], buf[at + 15]]) as usize;
            // The mask and not the descriptor, because a removed watch's own IN_IGNORED carries it too.
            if this == wd && (mask & IN_CREATE) != 0 {
                return true;
            }
            at += EVENT_HEADER + len;
        }
        false
    }

    // Two seconds all told, which is the kernel queueing being slow rather than the watch being gone.
    const TRIES: usize = 200;
    const BETWEEN_TRIES: Duration = Duration::from_millis(10);

    // The kernel queues on its own schedule, so this reads until the create lands or the tries run out.
    fn saw_a_create(fd: c_int, wd: c_int) -> bool {
        let mut buf = [0u8; BUF];
        for _ in 0..TRIES {
            let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, BUF) };
            if n > 0 && carries_a_create(&buf[..n as usize], wd) {
                return true;
            }
            thread::sleep(BETWEEN_TRIES);
        }
        false
    }

    // inotify_add_watch answers the descriptor the folder already holds, so an abandoned re-list of the
    // directory on screen must not remove it. This one carries a real descriptor because the two below
    // run at fd -1, where drop_one makes no syscall and the guard therefore has nothing to show.
    #[test]
    fn an_abandoned_re_list_of_the_same_folder_keeps_its_watch() {
        let sandbox = crate::backend::testdir::TestDir::new("watch-abandon");
        let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
        assert!(fd >= 0, "this box has no inotify to test with");
        let mut w = Watch { fd, wd: -1, incoming: -1 };
        w.begin(sandbox.path());
        w.commit();
        let live = w.wd;
        assert!(live >= 0, "the sandbox could not be watched");

        w.begin(sandbox.path());
        assert_eq!(w.incoming, live, "a re-list of one inode aliases onto the descriptor it has");
        w.abandon();

        sandbox.file("after-an-abandoned-relist.txt", "x");
        assert!(saw_a_create(fd, live), "the abandoned re-list took the open folder's watch with it");
    }

    // No descriptor in these two, so they pin the bookkeeping alone; the one above pins the syscall.
    #[test]
    fn an_abandoned_scan_leaves_the_current_watch_alone() {
        let mut w = Watch { fd: -1, wd: 7, incoming: -1 };
        w.begin(Path::new("/tmp"));
        w.abandon();
        assert!(w.is_current(7));
    }

    // And one that succeeds hands the listing over to the descriptor the scan was armed with.
    #[test]
    fn a_committed_scan_takes_over_from_the_old_watch() {
        let mut w = Watch { fd: -1, wd: 7, incoming: 9 };
        w.commit();
        assert!(w.is_current(9));
        assert!(!w.is_current(7));
    }

    #[test]
    fn the_changed_line_names_its_directory() {
        assert_eq!(
            changed_line(Path::new("/tmp/a \"b\"")),
            r#"{"t":"changed","path":"/tmp/a \"b\""}"#
        );
    }
}
