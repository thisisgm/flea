//! Wakes a worker when a folder holding one of its inputs changes, so an idle window sleeps.
//!
//! While inotify watches every folder the thread blocks in `poll` with no timeout. A folder that is
//! not there yet is watched through its nearest existing parent, which sees it arrive, and the next
//! wake arms the folder itself. The worker's old poll, once per `tick`, remains only while inotify
//! is unavailable or refuses a watch (the per-user watch limit, a permission error).
use std::{
    ffi::{c_char, c_int, c_uint, c_void, CString},
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

unsafe extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn eventfd(initval: c_uint, flags: c_int) -> c_int;
    fn poll(fds: *mut PollFd, count: usize, timeout: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

// IN_NONBLOCK and IN_CLOEXEC share O_NONBLOCK's and O_CLOEXEC's values, as do EFD_NONBLOCK and EFD_CLOEXEC.
const NONBLOCK: c_int = 0o4000;
const CLOEXEC: c_int = 0o2_000_000;
const POLLIN: i16 = 1;
const ENOTDIR: i32 = 20;
const IN_MODIFY: u32 = 0x2;
const IN_ATTRIB: u32 = 0x4;
const IN_CLOSE_WRITE: u32 = 0x8;
const IN_MOVED_FROM: u32 = 0x40;
const IN_MOVED_TO: u32 = 0x80;
const IN_CREATE: u32 = 0x100;
const IN_DELETE: u32 = 0x200;
const IN_DELETE_SELF: u32 = 0x400;
const IN_MOVE_SELF: u32 = 0x800;
const IN_DONT_FOLLOW: u32 = 0x0200_0000;
// Every way Omarchy or an editor replaces a file, and the folder itself going, so the next wake re-arms.
const MASK: u32 = IN_MODIFY
    | IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_CREATE
    | IN_DELETE
    | IN_DELETE_SELF
    | IN_MOVE_SELF;
// Events are drained, never parsed; read fails with EINVAL below one event with a NAME_MAX name, 272 bytes.
const DRAIN_BYTES: usize = 4096;

struct Fd(c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe { close(self.0) };
        }
    }
}

/// Stops the wake thread when dropped; the worker that owns it holds it for as long as it wants wakes.
#[must_use = "dropping the handle stops the wakes"]
pub(crate) struct DirWake {
    stop: Arc<Fd>,
}

impl Drop for DirWake {
    fn drop(&mut self) {
        if self.stop.0 >= 0 {
            let one = 1u64;
            unsafe { write(self.stop.0, (&raw const one).cast(), size_of::<u64>()) };
        }
    }
}

// True when every folder is watched, itself or through its nearest existing parent. `held` is the
// previous arm's watches; those this arm no longer places are removed.
fn arm(
    inotify: c_int,
    name: &str,
    dirs: &[PathBuf],
    told: &mut [bool],
    held: &mut Vec<c_int>,
) -> bool {
    let mut all = true;
    let mut placed = Vec::new();
    for (dir, told) in dirs.iter().zip(told) {
        let mut at: &Path = dir;
        loop {
            let Ok(path) = CString::new(at.as_os_str().as_bytes()) else {
                all = false;
                break;
            };
            let watch = unsafe { inotify_add_watch(inotify, path.as_ptr(), MASK) };
            if watch >= 0 {
                placed.push(watch);
                // A watch follows a link to its target, so the link is watched too, to see it repointed.
                if fs::symlink_metadata(at).is_ok_and(|meta| meta.file_type().is_symlink()) {
                    let link = MASK | IN_DONT_FOLLOW;
                    placed.push(unsafe { inotify_add_watch(inotify, path.as_ptr(), link) });
                }
                break;
            }
            let error = io::Error::last_os_error();
            let absent =
                error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(ENOTDIR);
            if let (true, Some(parent)) = (absent, at.parent()) {
                at = parent;
                continue;
            }
            all = false;
            if !std::mem::replace(told, true) {
                eprintln!(
                    "flea-gui: {name} wake: watch {}: {error}; its changes wait for the poll",
                    dir.display()
                );
            }
            break;
        }
    }
    // A parent watched only until its missing folder appeared would wake on every unrelated change.
    for stale in held.iter().filter(|watch| !placed.contains(watch)) {
        unsafe { inotify_rm_watch(inotify, *stale) };
    }
    *held = placed;
    all
}

/// Calls `wake` after each change in `dirs`, and once per `tick` only while inotify cannot watch
/// them all; the thread ends when `wake` returns false or the handle is dropped.
pub(crate) fn start(
    name: &'static str,
    dirs: Vec<PathBuf>,
    tick: Duration,
    wake: impl FnMut() -> bool + Send + 'static,
) -> DirWake {
    start_with(name, dirs, tick, wake, true)
}

// `inotify` false stands in for a kernel without it, so a test can drive the fallback.
fn start_with(
    name: &'static str,
    dirs: Vec<PathBuf>,
    tick: Duration,
    mut wake: impl FnMut() -> bool + Send + 'static,
    inotify: bool,
) -> DirWake {
    let inotify = Fd(if inotify {
        unsafe { inotify_init1(NONBLOCK | CLOEXEC) }
    } else {
        -1
    });
    let stop = Arc::new(Fd(unsafe { eventfd(0, NONBLOCK | CLOEXEC) }));
    let (mut told, mut held) = (vec![false; dirs.len()], Vec::new());
    if inotify.0 < 0 {
        eprintln!(
            "flea-gui: {name} wake: inotify: {}; changes wait for the poll",
            io::Error::last_os_error()
        );
    }
    // Armed before this returns, so a change made right after start is never missed.
    let mut armed = inotify.0 >= 0 && arm(inotify.0, name, &dirs, &mut told, &mut held);
    let timeout = c_int::try_from(tick.as_millis()).unwrap_or(c_int::MAX);
    let stopping = stop.clone();
    let spawned = thread::Builder::new()
        .name(format!("flea-{name}-wake"))
        .spawn(move || {
            // Whole bindings: the closure reads only `.0`, and capturing that field alone would leave
            // each `Fd` behind to close its descriptor as soon as `start` returns.
            let (inotify, stopping) = (inotify, stopping);
            let mut buf = [0u8; DRAIN_BYTES];
            loop {
                // Without the eventfd a drop cannot interrupt the wait, so the tick doubles as the exit check.
                let wait = if armed && stopping.0 >= 0 {
                    -1
                } else {
                    timeout
                };
                // poll skips a negative fd, so a missing inotify or eventfd only leaves the other.
                let mut fds = [
                    PollFd {
                        fd: inotify.0,
                        events: POLLIN,
                        revents: 0,
                    },
                    PollFd {
                        fd: stopping.0,
                        events: POLLIN,
                        revents: 0,
                    },
                ];
                if inotify.0 < 0 && stopping.0 < 0 {
                    thread::sleep(tick);
                } else {
                    unsafe { poll(fds.as_mut_ptr(), fds.len(), wait) };
                }
                if fds[1].revents != 0 {
                    return;
                }
                if inotify.0 >= 0 {
                    armed = arm(inotify.0, name, &dirs, &mut told, &mut held);
                    // Drained after the arm, so a removed watch's IN_IGNORED does not wake again; the
                    // wake below covers anything else drained here.
                    while unsafe { read(inotify.0, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
                }
                if !wake() {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        eprintln!("flea-gui: {name} wake: {error}");
    }
    DirWake { stop }
}

#[cfg(test)]
#[path = "dirwake_tests.rs"]
mod tests;
