// A send rings the loop's doorbell only while it sleeps in poll beside stdin; see src/backend/inbox.rs.
use super::child::{poll, PollFd};
use std::ffi::{c_long, c_void};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{fence, AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SendError};
use std::sync::Arc;
use std::time::Duration;

// eventfd(2) EFD_CLOEXEC, so no child the backend starts inherits the doorbell.
pub(crate) const EFD_CLOEXEC: i32 = 0o2000000;
const POLLIN: i16 = 1;
pub(crate) const EINTR: i32 = 4;
const CLOCK_MONOTONIC: i32 = 1;
// timerfd_create(2) TFD_CLOEXEC is O_CLOEXEC, the same bit as EFD_CLOEXEC.
const TFD_CLOEXEC: i32 = EFD_CLOEXEC;

// std already links the system libc, so the three symbols are declared here rather than taking a crate.
extern "C" {
    fn eventfd(initval: u32, flags: i32) -> i32;
    pub(crate) fn read(fd: i32, buf: *mut c_void, count: usize) -> isize;
    fn write(fd: i32, buf: *const c_void, count: usize) -> isize;
}

// One eventfd counter; poll reports it readable once any ring landed.
pub(crate) struct Counter(OwnedFd);

impl Counter {
    pub(crate) fn open() -> io::Result<Counter> {
        match unsafe { eventfd(0, EFD_CLOEXEC) } {
            fd if fd >= 0 => Ok(Counter(unsafe { OwnedFd::from_raw_fd(fd) })),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub(crate) fn fd(&self) -> i32 {
        self.0.as_raw_fd()
    }

    pub(crate) fn ring(&self) {
        let one = 1u64;
        let _ = unsafe { write(self.fd(), (&one as *const u64).cast(), 8) };
    }

    // Called only after poll reported the counter readable, so the read never blocks.
    pub(crate) fn clear(&self) {
        let mut count = 0u64;
        let _ = unsafe { read(self.fd(), (&mut count as *mut u64).cast(), 8) };
    }
}

// timerfd_settime(2) struct itimerspec: two struct timespec of time_t and long, both C long here.
#[repr(C)]
struct TimeSpec {
    sec: c_long,
    nsec: c_long,
}

#[repr(C)]
struct TimerSpec {
    interval: TimeSpec,
    value: TimeSpec,
}

extern "C" {
    fn timerfd_create(clock: i32, flags: i32) -> i32;
    fn timerfd_settime(fd: i32, flags: i32, new: *const TimerSpec, old: *mut TimerSpec) -> i32;
}

// A one-shot monotonic timer the helper blocks on; the loop arms it on waking and disarms it before sleeping.
pub(crate) struct Timer(OwnedFd);

impl Timer {
    pub(crate) fn open() -> io::Result<Timer> {
        match unsafe { timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC) } {
            fd if fd >= 0 => Ok(Timer(unsafe { OwnedFd::from_raw_fd(fd) })),
            _ => Err(io::Error::last_os_error()),
        }
    }

    // A zero duration disarms it.
    pub(crate) fn set(&self, after: Duration) {
        let value = TimeSpec {
            sec: after.as_secs() as c_long,
            nsec: after.subsec_nanos() as c_long,
        };
        let spec = TimerSpec {
            interval: TimeSpec { sec: 0, nsec: 0 },
            value,
        };
        let _ = unsafe { timerfd_settime(self.0.as_raw_fd(), 0, &spec, std::ptr::null_mut()) };
    }

    // Blocks until it fires; a settime resets an expiry not yet read, so a disarm in between keeps the helper parked.
    pub(crate) fn wait(&self) -> bool {
        loop {
            let mut count = 0u64;
            if unsafe { read(self.0.as_raw_fd(), (&mut count as *mut u64).cast(), 8) } == 8 {
                return true;
            }
            if io::Error::last_os_error().raw_os_error() != Some(EINTR) {
                return false;
            }
        }
    }
}

pub(crate) fn wait(fds: &mut [PollFd], timeout: i32) -> bool {
    loop {
        fds.iter_mut().for_each(|fd| fd.revents = 0);
        let n = unsafe { poll(fds.as_mut_ptr(), fds.len(), timeout) };
        if n >= 0 {
            return n > 0;
        }
        if io::Error::last_os_error().raw_os_error() != Some(EINTR) {
            return false;
        }
    }
}

pub(crate) fn watch(fd: i32) -> PollFd {
    PollFd {
        fd,
        events: POLLIN,
        revents: 0,
    }
}

// The counter and whether the loop is past its last look at the channel, so a busy loop costs a send no syscall.
pub struct Wake {
    pub(crate) counter: Counter,
    sleeping: AtomicBool,
}

impl Wake {
    pub(crate) fn new(counter: Counter) -> Arc<Wake> {
        Arc::new(Wake {
            counter,
            sleeping: AtomicBool::new(false),
        })
    }

    // Marked before the loop's last look at the channel, so a send either lands before that look or sees the mark and rings.
    pub(crate) fn sleep(&self) {
        self.sleeping.store(true, Ordering::Relaxed);
        fence(Ordering::SeqCst);
    }

    pub(crate) fn woke(&self) {
        self.sleeping.store(false, Ordering::Relaxed);
    }

    fn ring(&self) {
        fence(Ordering::SeqCst);
        if self.sleeping.load(Ordering::Relaxed) {
            self.counter.ring();
        }
    }
}

// std's sender plus the loop's doorbell; a channel no loop sleeps on carries none.
pub struct Sender<T> {
    tx: mpsc::Sender<T>,
    wake: Option<Arc<Wake>>,
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.tx.send(value)?;
        if let Some(wake) = &self.wake {
            wake.ring();
        }
        Ok(())
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            tx: self.tx.clone(),
            wake: self.wake.clone(),
        }
    }
}

#[cfg(test)]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (Sender { tx, wake: None }, rx)
}

pub(crate) fn waking<T>(wake: &Arc<Wake>) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (
        Sender {
            tx,
            wake: Some(Arc::clone(wake)),
        },
        rx,
    )
}
