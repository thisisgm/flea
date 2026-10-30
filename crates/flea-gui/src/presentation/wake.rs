//! Wakes the presentation worker as soon as a folder holding one of its inputs changes, and at least once per tick.
use std::{
    ffi::{c_char, c_int, c_void, CString},
    io,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::mpsc::Sender,
    thread,
    time::Duration,
};

unsafe extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
    fn poll(fds: *mut PollFd, count: usize, timeout: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

const IN_NONBLOCK: c_int = 0o4000;
const IN_CLOEXEC: c_int = 0o2_000_000;
const POLLIN: i16 = 1;
const IN_MODIFY: u32 = 0x2;
const IN_ATTRIB: u32 = 0x4;
const IN_CLOSE_WRITE: u32 = 0x8;
const IN_MOVED_FROM: u32 = 0x40;
const IN_MOVED_TO: u32 = 0x80;
const IN_CREATE: u32 = 0x100;
const IN_DELETE: u32 = 0x200;
// Every way Omarchy or an editor replaces a file.
const MASK: u32 =
    IN_MODIFY | IN_ATTRIB | IN_CLOSE_WRITE | IN_MOVED_FROM | IN_MOVED_TO | IN_CREATE | IN_DELETE;
// Events are drained, never parsed; read fails with EINVAL below one event with a NAME_MAX name, 272 bytes.
const DRAIN_BYTES: usize = 4096;

pub(super) enum Signal {
    Wake,
    Stop,
}

struct Inotify(c_int);

impl Inotify {
    // Re-arming watches a folder created since, or the new one a theme switch moved in; a folder not there yet is normal.
    fn arm(&self, dirs: &[CString], told: &mut [bool]) {
        for (dir, told) in dirs.iter().zip(told) {
            if unsafe { inotify_add_watch(self.0, dir.as_ptr(), MASK) } >= 0 {
                continue;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::NotFound && !std::mem::replace(told, true) {
                eprintln!(
                    "flea-gui: presentation wake: watch {}: {error}; its changes wait for the stat tick",
                    dir.to_string_lossy()
                );
            }
        }
    }
}

impl Drop for Inotify {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe { close(self.0) };
        }
    }
}

/// Sends a wake per change and per quiet `tick`, and ends once the worker has gone; without inotify only the ticks remain.
pub(super) fn start(dirs: Vec<PathBuf>, tick: Duration, wake: Sender<Signal>) {
    let dirs: Vec<CString> = dirs
        .into_iter()
        .filter_map(|dir| CString::new(dir.as_os_str().as_bytes()).ok())
        .collect();
    let mut told = vec![false; dirs.len()];
    let inotify = Inotify(unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) });
    if inotify.0 < 0 {
        eprintln!(
            "flea-gui: presentation wake: inotify: {}; theme changes wait for the stat tick",
            io::Error::last_os_error()
        );
    } else {
        // Armed before this returns, so a change made right after start is never missed.
        inotify.arm(&dirs, &mut told);
    }
    let timeout = c_int::try_from(tick.as_millis()).unwrap_or(c_int::MAX);
    let spawned = thread::Builder::new()
        .name("flea-presentation-wake".into())
        .spawn(move || {
            let mut buf = [0u8; DRAIN_BYTES];
            loop {
                if inotify.0 < 0 {
                    thread::sleep(tick);
                } else {
                    let mut fd = PollFd {
                        fd: inotify.0,
                        events: POLLIN,
                        revents: 0,
                    };
                    unsafe { poll(&mut fd, 1, timeout) };
                    while unsafe { read(inotify.0, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
                    inotify.arm(&dirs, &mut told);
                }
                if wake.send(Signal::Wake).is_err() {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        eprintln!("flea-gui: presentation wake: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{start, Signal};
    use crate::presentation::{fixture::Fixture, source::Sources};
    use std::{sync::mpsc, time::Duration};

    // A tick no test run reaches, so only an inotify event can send the wake.
    const UNREACHED_TICK: Duration = Duration::from_secs(3600);
    // Turns a dead inotify path into a failure instead of a hang.
    const GUARD: Duration = Duration::from_secs(10);

    // Omarchy swaps the theme folder by renaming it into place; that rename alone must wake the worker (THEME-GLASS, native2 GPUI 475 ms, v0.3.12 27 ms).
    #[test]
    fn theme_glass_a_theme_folder_rename_wakes_the_worker_without_a_tick() {
        let fixture = Fixture::new("wake");
        let dir = fixture.path().to_path_buf();
        let current = dir.join("current");
        let theme = current.join("theme");
        let inputs = [
            theme.join("colors.toml"),
            theme.join("shell.toml"),
            dir.join("config/shell.toml"),
            dir.join("fontconfig/fonts.conf"),
            dir.join("fontconfig/conf.d/im.conf"),
            dir.join("toggles/window-opaque.lua"),
            dir.join("toggles/window-glassy.lua"),
        ];
        fixture.mkdir(&current.join("next"));
        let (wake, woken) = mpsc::channel();
        start(
            Sources::at(inputs, dir.join("fc-match")).dirs(),
            UNREACHED_TICK,
            wake,
        );
        std::fs::rename(current.join("next"), &theme).unwrap();
        assert!(
            matches!(woken.recv_timeout(GUARD), Ok(Signal::Wake)),
            "renaming the theme folder into a watched folder must wake the worker"
        );
        drop(woken);
        fixture.cleanup();
    }
}
