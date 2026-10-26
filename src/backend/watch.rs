// The listed directory, watched so an outside change reaches the client; see docs/protocol.md.
use crate::backend::events::Event;
use crate::backend::inotifyburst::each_event;
use crate::json::escape;
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
// IN_IGNORED on the open watch re-lists; IN_Q_OVERFLOW arrives on wd -1 and is_current drops it.
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

// One directory at a time plus the one a scan is reading; a negative descriptor is nothing watched.
pub struct Watch {
    fd: std::sync::OnceLock<c_int>,
    events: Sender<Event>,
    wd: c_int,
    incoming: c_int,
    mount: std::path::PathBuf,
    incoming_mount: std::path::PathBuf,
}

// Sample mountinfo: "30 1 8:17 / /media/stick rw - vfat /dev/sdb1 rw" keys "/media/stick".
pub(crate) fn live_mount_of(path: &Path) -> std::path::PathBuf {
    super::iomount::mount_key_plain(path, &super::iomount::mount_body())
}

impl Watch {
    // A box with no inotify still gets a file manager, as live as 0.1.4's was, and is told once.
    pub fn start(tx: Sender<Event>) -> Watch {
        Watch { fd: std::sync::OnceLock::new(), events: tx, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() }
    }

    // Armed beside the current watch once a pending one goes, so a scan that fails costs the open folder nothing.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn begin(&mut self, path: &Path) {
        self.abandon();
        self.set_incoming(self.add(path), live_mount_of(path));
    }

    // A re-list answers the descriptor the folder already had, so dropping it would unwatch it.
    pub fn commit(&mut self) {
        if self.incoming != self.wd {
            self.drop_one(self.wd);
        }
        self.wd = self.incoming;
        self.incoming = -1;
        if !self.incoming_mount.as_os_str().is_empty() {
            self.mount = std::mem::take(&mut self.incoming_mount);
        }
    }

    // The scan failed, so the listing did not move and neither does its watch, aliased or not.
    pub fn abandon(&mut self) {
        if self.incoming != self.wd {
            self.drop_one(self.incoming);
        }
        self.incoming = -1;
        self.incoming_mount = std::path::PathBuf::new();
    }

    // A failed scan armed its watch on the worker, so the loop removes that descriptor too; an aliased re-list stays.
    pub fn abandon_wd(&mut self, wd: c_int) {
        if wd >= 0 && wd != self.wd {
            self.drop_one(wd);
        }
        self.abandon();
    }

    pub fn stop(&mut self) {
        self.drop_one(self.wd);
        if self.incoming != self.wd {
            self.drop_one(self.incoming);
        }
        self.wd = -1;
        self.incoming = -1;
        self.mount = std::path::PathBuf::new();
        self.incoming_mount = std::path::PathBuf::new();
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
    // The worker's descriptor becomes the incoming watch the commit takes over.
    pub(crate) fn set_incoming(&mut self, wd: c_int, mount: std::path::PathBuf) {
        self.drop_one(self.incoming);
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
    fn drop_one(&self, wd: c_int) {
        if let Some(&fd) = self.fd.get().filter(|&&fd| fd >= 0 && wd >= 0) {
            unsafe { inotify_rm_watch(fd, wd) };
        }
    }

    // A directory this box could have watched and did not; no inotify at all is said once on first use.
    pub fn refused(&self) -> bool {
        self.fd.get().is_some_and(|&fd| fd >= 0) && self.wd < 0
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
        let (changed, unmounted) = classify_burst(&buf[..n as usize]);
        for wd in changed {
            if tx.send(Event::Changed(wd)).is_err() {
                return;
            }
        }
        for wd in unmounted {
            if tx.send(Event::Unmounted(wd)).is_err() {
                return;
            }
        }
        thread::sleep(COALESCE);
    }
}

// Sample input: one IN_CREATE on wd 3 plus one IN_UNMOUNT on wd 4 answers ([3], [4]).
pub(crate) fn classify_burst(buf: &[u8]) -> (Vec<i32>, Vec<i32>) {
    let mut changed: Vec<i32> = Vec::new();
    let mut unmounted: Vec<i32> = Vec::new();
    each_event(buf, |wd, mask| {
        if mask & IN_UNMOUNT != 0 {
            if !unmounted.contains(&wd) {
                unmounted.push(wd);
            }
        } else if mask & (MASK | IN_DELETE_SELF | IN_Q_OVERFLOW | IN_IGNORED) != 0 && !changed.contains(&wd) {
            changed.push(wd);
        }
    });
    // A vanished path never answers changed: IN_IGNORED follows IN_UNMOUNT on the same wd.
    changed.retain(|wd| !unmounted.contains(wd));
    (changed, unmounted)
}

// Which watches this burst touched, each once; nothing past the descriptor is ever read.
#[cfg(test)]
fn descriptors(buf: &[u8]) -> Vec<i32> {
    classify_burst(buf).0
}

// The one unsolicited line: the listed directory is no longer what the listing answered with.
pub fn changed_line(path: &Path) -> String {
    format!(r#"{{"t":"changed","path":"{}"}}"#, escape(&path.to_string_lossy()))
}

// Sample output: {"t":"unmounted","path":"/media/stick","parent":"/media"}; parent is nearest existing.
pub fn unmounted_line(path: &Path, parent: &Path) -> String {
    format!(r#"{{"t":"unmounted","path":"{}","parent":"{}"}}"#, escape(&path.to_string_lossy()), escape(&parent.to_string_lossy()))
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
mod tests {
    use super::*;
    use crate::backend::inotifyburst::EVENT_HEADER;

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

    // Sample input: wd 7 mask IN_UNMOUNT answers unmounted [7] and changed [].
    fn unmount_event(wd: i32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&wd.to_ne_bytes());
        out.extend_from_slice(&IN_UNMOUNT.to_ne_bytes());
        out.extend_from_slice(&0u32.to_ne_bytes());
        out.extend_from_slice(&0u32.to_ne_bytes());
        out
    }

    #[test]
    fn an_unmount_never_becomes_a_changed_relist() {
        assert_eq!(descriptors(&unmount_event(7)), Vec::<i32>::new());
        assert_eq!(classify_burst(&unmount_event(7)), (Vec::new(), vec![7]));
    }

    #[test]
    fn a_removed_watch_still_answers_a_changed_relist() {
        let mut out = Vec::new();
        out.extend_from_slice(&7i32.to_ne_bytes());
        out.extend_from_slice(&IN_IGNORED.to_ne_bytes());
        out.extend_from_slice(&0u32.to_ne_bytes());
        out.extend_from_slice(&0u32.to_ne_bytes());
        assert_eq!(classify_burst(&out), (vec![7], Vec::new()));
    }

    #[test]
    fn an_ejected_folder_lands_on_its_nearest_existing_parent() {
        use std::collections::HashSet;
        let live: HashSet<&str> = ["/media", "/"].into_iter().collect();
        let at = nearest_parent(Path::new("/media/stick/photos"), |p| live.contains(p.to_str().unwrap_or("")));
        assert_eq!(at, Path::new("/media"));
        assert_eq!(unmounted_line(Path::new("/media/stick"), &at), r#"{"t":"unmounted","path":"/media/stick","parent":"/media"}"#);
    }

    #[test]
    fn nothing_is_current_before_a_directory_is_followed() {
        let w = Watch { fd: (-1).into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        assert!(!w.is_current(-1));
        assert!(!w.is_current(1));
    }

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
        let mut w = Watch { fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
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

    // A second begin before any commit answers the pending descriptor again, which it must not then remove.
    #[test]
    fn a_second_begin_of_one_folder_keeps_the_watch_it_arms() {
        let sandbox = crate::backend::testdir::TestDir::new("watch-rebegin");
        let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
        assert!(fd >= 0, "this box has no inotify to test with");
        let mut w = Watch { fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        w.begin(sandbox.path());
        w.begin(sandbox.path());
        w.commit();
        assert!(w.wd >= 0, "the sandbox could not be watched");
        sandbox.file("after-a-second-begin.txt", "x");
        assert!(saw_a_create(fd, w.wd), "the second begin removed the watch it had just armed");
    }

    // A removal queues IN_IGNORED synchronously, so one nonblocking read observes it with no wait.
    const IN_IGNORED: u32 = 0x0000_8000;
    // Sample input: wd 1, mask 0x00000100, cookie 0, len 16, then "NEWFILE.txt\0\0\0\0\0".
    fn ignored_wds(fd: c_int) -> Vec<c_int> {
        let mut buf = [0u8; BUF];
        let mut out = Vec::new();
        loop {
            let n = unsafe { read(fd, buf.as_mut_ptr() as *mut c_void, BUF) };
            if n <= 0 {
                break;
            }
            let mut at = 0;
            while at + EVENT_HEADER <= n as usize {
                let wd = i32::from_ne_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
                let mask = u32::from_ne_bytes([buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]]);
                let len = u32::from_ne_bytes([buf[at + 12], buf[at + 13], buf[at + 14], buf[at + 15]]) as usize;
                if mask & IN_IGNORED != 0 && !out.contains(&wd) {
                    out.push(wd);
                }
                at += EVENT_HEADER + len;
            }
        }
        out
    }

    // A late worker hands its descriptor back instead of removing it, so a re-list of the open folder keeps its watch.
    #[test]
    fn a_late_re_list_of_the_open_folder_keeps_its_watch() {
        let sandbox = crate::backend::testdir::TestDir::new("watch-late");
        let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
        assert!(fd >= 0, "this box has no inotify to test with");
        let mut w = Watch { fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        w.begin(sandbox.path());
        w.commit();
        let live = w.wd;
        assert!(live >= 0, "the sandbox could not be watched");
        let late = Watch::add_raw(fd, sandbox.path());
        assert_eq!(late, live, "a re-list of one inode aliases onto the descriptor it has");
        w.abandon_wd(late);
        sandbox.file("after-a-late-relist.txt", "x");
        assert!(saw_a_create(fd, live), "the late re-list took the open folder's watch with it");
    }

    // A failed scan's descriptor goes, but a failed re-list aliases onto the live one, which stays.
    #[test]
    fn abandon_wd_removes_only_a_watch_that_is_not_live() {
        let a = crate::backend::testdir::TestDir::new("abandon-a");
        let b = crate::backend::testdir::TestDir::new("abandon-b");
        let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
        assert!(fd >= 0, "this box has no inotify to test with");
        let mut w = Watch { fd: fd.into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        w.begin(a.path());
        w.commit();
        let live = w.wd;
        assert!(live >= 0, "the sandbox could not be watched");
        let other = Watch::add_raw(fd, b.path());
        assert!(other >= 0 && other != live, "the second sandbox arms its own descriptor");
        assert!(ignored_wds(fd).is_empty(), "arming queues no removal");
        w.abandon_wd(other);
        assert_eq!(ignored_wds(fd), vec![other], "the failed scan's descriptor is removed");
        assert!(w.is_current(live), "the open folder is still followed");
        w.abandon_wd(live);
        assert!(ignored_wds(fd).is_empty(), "a failed re-list aliases onto the live watch, which stays");
    }

    // No descriptor in these two, so they pin the bookkeeping alone; the one above pins the syscall.
    #[test]
    fn an_abandoned_scan_leaves_the_current_watch_alone() {
        let mut w = Watch { fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        w.begin(Path::new("/tmp"));
        w.abandon();
        assert!(w.is_current(7));
    }

    // And one that succeeds hands the listing over to the descriptor the scan was armed with.
    #[test]
    fn a_committed_scan_takes_over_from_the_old_watch() {
        let mut w = Watch { fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: 9, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        w.commit();
        assert!(w.is_current(9));
        assert!(!w.is_current(7));
    }

    // Production arms through set_incoming plus commit, with the mount the list worker computed.
    #[test]
    fn set_incoming_captures_the_mount_commit_takes() {
        let mut w = Watch { fd: (-1).into(), events: crate::backend::events::channel().0, wd: -1, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        w.set_incoming(7, Path::new("/media/stick").to_path_buf());
        w.commit();
        assert!(w.is_current(7));
        assert_eq!(w.mount, Path::new("/media/stick"), "the watch reports against a dead mountpoint");
    }

    #[test]
    fn an_unmount_burst_never_answers_changed_for_a_vanished_path() {
        // Sample input: IN_UNMOUNT plus IN_IGNORED on wd 7 answers changed [] and unmounted [7].
        fn burst(wd: i32, masks: &[u32]) -> Vec<u8> {
            let mut out = Vec::new();
            for mask in masks {
                out.extend_from_slice(&wd.to_ne_bytes());
                out.extend_from_slice(&mask.to_ne_bytes());
                out.extend_from_slice(&0u32.to_ne_bytes());
                out.extend_from_slice(&0u32.to_ne_bytes());
            }
            out
        }
        assert_eq!(classify_burst(&burst(7, &[IN_UNMOUNT, IN_IGNORED])), (Vec::new(), vec![7]));
        assert_eq!(classify_burst(&burst(7, &[IN_CREATE, IN_UNMOUNT])), (Vec::new(), vec![7]));
    }

    #[test]
    fn an_ejected_mountpoint_lands_outside_the_volume() {
        // Sample input: open "/media/stick/photos" with mount "/media/stick" starts "/media".
        use std::collections::HashSet;
        let w = Watch { fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: -1, mount: Path::new("/media/stick").to_path_buf(), incoming_mount: std::path::PathBuf::new() };
        assert_eq!(w.unmount_start(Path::new("/media/stick/photos")), Path::new("/media"));
        assert_eq!(w.unmount_start(Path::new("/media/stick")), Path::new("/media"));
        let live: HashSet<&str> = ["/media", "/"].into_iter().collect();
        let start = w.unmount_start(Path::new("/media/stick/photos"));
        let at = nearest_parent(&start, |p| live.contains(p.to_str().unwrap_or("")));
        assert_eq!(at, Path::new("/media"), "a surviving mountpoint is never the parent");
        let bare = Watch { fd: (-1).into(), events: crate::backend::events::channel().0, wd: 7, incoming: -1, mount: std::path::PathBuf::new(), incoming_mount: std::path::PathBuf::new() };
        assert_eq!(bare.unmount_start(Path::new("/media/stick")), Path::new("/media"));
    }

    #[test]
    fn the_changed_line_names_its_directory() {
        assert_eq!(
            changed_line(Path::new("/tmp/a \"b\"")),
            r#"{"t":"changed","path":"/tmp/a \"b\""}"#
        );
    }
}
