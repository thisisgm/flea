// One cancellable pidfd wait slot per watching connection; an exit asks a fresh get, never guesses none.
use super::{control, watch, wire::Conn};
use crate::backend::opsreq::OpMsg;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::raw::{c_int, c_void};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc, Mutex,
};

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

extern "C" {
    fn pidfd_open(pid: c_int, flags: u32) -> c_int;
    fn eventfd(value: u32, flags: c_int) -> c_int;
    fn poll(fds: *mut PollFd, count: usize, timeout: c_int) -> c_int;
    fn read(fd: c_int, bytes: *mut c_void, count: usize) -> isize;
    fn write(fd: c_int, bytes: *const c_void, count: usize) -> isize;
}

const CLOEXEC_NONBLOCK: c_int = 0o2000000 | 0o4000;
const POLLIN: i16 = 1;
const ESRCH: i32 = 3;
const POLL_NOW: c_int = 0;
const POLL_FOREVER: c_int = -1;
const NO_FD: c_int = -1;

enum OwnerFd {
    Live(OwnedFd),
    Ended,
}

struct Job {
    fd: Option<OwnedFd>,
    token: String,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct Mail {
    pending: Option<Option<Job>>,
    active: Option<Arc<AtomicBool>>,
    tracked_token: Option<String>,
    stopped: bool,
}

pub(super) struct OwnerEnd {
    mail: Arc<Mutex<Mail>>,
    wake: Option<Arc<OwnedFd>>,
    worker: Option<std::thread::JoinHandle<()>>,
    replies: Sender<OpMsg>,
    state: watch::Shared,
    socket: Option<PathBuf>,
    eventfd_failed: bool,
    spawn_failed: bool,
}

impl OwnerEnd {
    pub fn new(replies: Sender<OpMsg>, state: watch::Shared, socket: Option<PathBuf>) -> Self {
        Self {
            mail: Arc::new(Mutex::new(Mail::default())),
            wake: None,
            worker: None,
            replies,
            state,
            socket,
            eventfd_failed: false,
            spawn_failed: false,
        }
    }

    pub fn tracks(&self, token: &str) -> bool {
        self.mail
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tracked_token
            .as_deref()
            == Some(token)
    }

    pub fn track(&mut self, pid: Option<u32>, token: &str) {
        let owner = pid.and_then(owner_fd);
        if owner.is_some() && self.wake.is_none() {
            let raw = unsafe { eventfd(0, CLOEXEC_NONBLOCK) };
            if raw < 0 {
                if !self.eventfd_failed {
                    eprintln!(
                        "flea: clipboard owner-end eventfd failed: {}",
                        std::io::Error::last_os_error()
                    );
                    self.eventfd_failed = true;
                }
                return;
            }
            self.eventfd_failed = false;
            let wake = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
            let (mail, signal, replies, state, socket) = (
                self.mail.clone(),
                wake.clone(),
                self.replies.clone(),
                self.state.clone(),
                self.socket.clone(),
            );
            let worker = std::thread::Builder::new()
                .name("flea-clip-end".into())
                .spawn(move || wait(mail, signal, replies, state, socket));
            let worker = match worker {
                Ok(worker) => worker,
                Err(error) => {
                    if !self.spawn_failed {
                        eprintln!("flea: clipboard owner-end thread spawn failed: {}", error);
                        self.spawn_failed = true;
                    }
                    return;
                }
            };
            self.spawn_failed = false;
            self.wake = Some(wake);
            self.worker = Some(worker);
        }
        if let Some(wake) = &self.wake {
            replace(&self.mail, wake, owner, token);
        }
    }
}

impl Drop for OwnerEnd {
    fn drop(&mut self) {
        let Some(wake) = &self.wake else {
            return;
        };
        {
            let mut mail = self.mail.lock().unwrap_or_else(|e| e.into_inner());
            mail.stopped = true;
            mail.pending = None;
            if let Some(active) = mail.active.take() {
                active.store(true, Ordering::Release);
            }
        }
        signal(wake);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn signal(wake: &OwnedFd) {
    let value = 1u64.to_ne_bytes();
    loop {
        if unsafe { write(wake.as_raw_fd(), value.as_ptr().cast(), value.len()) } >= 0
            || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
        {
            return;
        }
    }
}

fn replace(mail: &Mutex<Mail>, wake: &OwnedFd, owner: Option<OwnerFd>, token: &str) {
    let mut mail = mail.lock().unwrap_or_else(|e| e.into_inner());
    if mail.stopped {
        return;
    }
    if let Some(active) = mail.active.take() {
        active.store(true, Ordering::Release);
    }
    let job = owner.map(|owner| {
        let cancelled = Arc::new(AtomicBool::new(false));
        mail.active = Some(cancelled.clone());
        let fd = match owner {
            OwnerFd::Live(fd) => Some(fd),
            OwnerFd::Ended => None,
        };
        Job {
            fd,
            token: token.to_string(),
            cancelled,
        }
    });
    // Keep the token after its one exit read, and update it when that read arms another owner.
    mail.tracked_token = job.as_ref().map(|job| job.token.clone());
    mail.pending = Some(job);
    signal(wake);
}

// Test only: how long a stopped waiter lingers before it returns, so a test tells a joined waiter from one left running.
#[cfg(test)]
pub(crate) static STOP_HOLD_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn wait(
    mail: Arc<Mutex<Mail>>,
    wake: Arc<OwnedFd>,
    replies: Sender<OpMsg>,
    state: watch::Shared,
    socket: Option<PathBuf>,
) {
    let mut current: Option<Job> = None;
    loop {
        let ended = current.as_ref().is_some_and(|job| job.fd.is_none());
        let mut fds = [
            PollFd {
                fd: wake.as_raw_fd(),
                events: POLLIN,
                revents: 0,
            },
            PollFd {
                fd: current
                    .as_ref()
                    .and_then(|job| job.fd.as_ref())
                    .map_or(NO_FD, AsRawFd::as_raw_fd),
                events: POLLIN,
                revents: 0,
            },
        ];
        let timeout = if ended { POLL_NOW } else { POLL_FOREVER };
        if unsafe { poll(fds.as_mut_ptr(), fds.len(), timeout) } < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            eprintln!("flea: clipboard owner-end poll failed: {}", error);
            return;
        }
        if fds[0].revents != 0 {
            let mut value = [0u8; 8];
            unsafe {
                read(wake.as_raw_fd(), value.as_mut_ptr().cast(), value.len());
            }
            let mut mail = mail.lock().unwrap_or_else(|e| e.into_inner());
            if mail.stopped {
                drop(mail);
                #[cfg(test)]
                std::thread::sleep(std::time::Duration::from_millis(
                    STOP_HOLD_MS.load(Ordering::Relaxed),
                ));
                return;
            }
            if let Some(job) = mail.pending.take() {
                current = job;
            }
            continue;
        }
        if ended || fds[1].revents & POLLIN != 0 {
            if let Some(job) = current.take() {
                let Job {
                    fd,
                    token,
                    cancelled,
                } = job;
                drop(fd);
                if cancelled.load(Ordering::Acquire) {
                    continue;
                }
                let mut connection = None;
                watch::reread(
                    &replies,
                    &state,
                    &token,
                    || {
                        connection = Some(Conn::open(socket.as_deref())?);
                        control::get_files_on(connection.as_mut().unwrap())
                    },
                    || cancelled.load(Ordering::Acquire),
                    |selection| {
                        replace(
                            &mail,
                            &wake,
                            selection.owner_pid.and_then(owner_fd),
                            &selection.token,
                        );
                    },
                );
            }
        }
    }
}

// Readiness around identity validation distinguishes a gone pid from a live foreign process.
fn owner_fd(pid: u32) -> Option<OwnerFd> {
    if pid == 0 || pid > i32::MAX as u32 {
        return None;
    }
    let raw = unsafe { pidfd_open(pid as c_int, 0) };
    if raw < 0 {
        return (std::io::Error::last_os_error().raw_os_error() == Some(ESRCH))
            .then_some(OwnerFd::Ended);
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    if owner_ended(&fd)? {
        return Some(OwnerFd::Ended);
    }
    let valid = is_owner(pid);
    if owner_ended(&fd)? {
        return Some(OwnerFd::Ended);
    }
    valid.then_some(OwnerFd::Live(fd))
}

fn owner_ended(fd: &OwnedFd) -> Option<bool> {
    let mut polled = [PollFd {
        fd: fd.as_raw_fd(),
        events: POLLIN,
        revents: 0,
    }];
    (unsafe { poll(polled.as_mut_ptr(), polled.len(), POLL_NOW) } >= 0)
        .then(|| polled[0].revents & POLLIN != 0)
}

fn is_owner(pid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(own) = std::fs::metadata("/proc/self/exe") else {
        return false;
    };
    let Ok(other) = std::fs::metadata(format!("/proc/{}/exe", pid)) else {
        return false;
    };
    if (own.dev(), own.ino()) != (other.dev(), other.ino()) {
        return false;
    }
    let Ok(command) = std::fs::read(format!("/proc/{}/cmdline", pid)) else {
        return false;
    };
    // Sample input: /usr/bin/flea\0--clip-own\0.
    let Some(command) = command.strip_suffix(&[0]) else {
        return false;
    };
    let args: Vec<_> = command.split(|byte| *byte == 0).collect();
    owner_args(&args)
}

fn owner_args(args: &[&[u8]]) -> bool {
    if matches!(args, [_, b"--clip-own"]) {
        return true;
    }
    cfg!(test)
        && matches!(
            args,
            [
                _,
                b"--exact",
                b"clip::watch::tests::end::stand_in_owner",
                b"--nocapture"
            ]
        )
}

#[cfg(test)]
#[path = "end_tests.rs"]
mod tests;
