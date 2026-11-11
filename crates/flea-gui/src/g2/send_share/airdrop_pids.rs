//! Each AirDrop long run under its generation, held by a pidfd, so Stop signals only the run still current.
use super::airdrop::Proc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::{Duration, Instant};

// How long a closing window, a quit or a SIGTERM waits for owly to give Wi-Fi back before Flea goes on.
pub const TEARDOWN: Duration = Duration::from_secs(5);
pub(super) const SIGTERM: i32 = 15;
const SIGHUP: i32 = 1;
const SIGINT: i32 = 2;
const SIG_DFL: usize = 0;
// pidfd_open and pidfd_send_signal carry these numbers on every architecture Flea ships for.
const SYS_PIDFD_SEND_SIGNAL: i64 = 424;
const SYS_PIDFD_OPEN: i64 = 434;
const POLLIN: i16 = 1;
const SLICE_MS: i32 = 50;
const SLOTS: usize = 8;

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn syscall(number: i64, ...) -> i64;
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
    fn dup(fd: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn signal(signum: i32, handler: usize) -> usize;
    fn raise(signum: i32) -> i32;
    fn kill(pid: i32, signum: i32) -> i32;
}

// Every live run's pidfd, for the signal handler, which may take no lock.
static LIVE: [AtomicI32; SLOTS] = [const { AtomicI32::new(-1) }; SLOTS];
static HANDLER: Once = Once::new();

// A run's pidfd once its thread has spawned it, and whether a Stop or a closing window came first.
#[derive(Default)]
pub(crate) struct Run {
    pidfd: Option<i32>,
    // Only on a kernel without pidfd_open: the pid, until the run's thread reaps it.
    pid: Option<u32>,
    stopped: bool,
}

pub(crate) type Held = Arc<Mutex<Run>>;

#[derive(Default)]
pub(crate) struct Pids([Option<(u64, Held)>; Proc::ALL.len()]);

impl Pids {
    // Claimed before the spawn, so a Stop that comes first still reaches the run.
    pub(super) fn claim(&mut self, proc: Proc, generation: u64) -> Held {
        let run = Held::default();
        self.0[proc as usize] = Some((generation, run.clone()));
        run
    }

    pub(super) fn stop(&mut self, proc: Proc) {
        if let Some((_, run)) = self.0[proc as usize].take() {
            let mut run = run.lock().unwrap_or_else(PoisonError::into_inner);
            run.stopped = true;
            if let Some(pidfd) = run.pidfd {
                terminate(pidfd);
            } else if let Some(pid) = run.pid {
                terminate_pid(pid);
            }
        }
    }

    pub(super) fn ended(&mut self, proc: Proc, generation: u64) {
        if self.0[proc as usize]
            .as_ref()
            .is_some_and(|(held, _)| *held == generation)
        {
            self.0[proc as usize] = None;
        }
    }
}

// The spawning thread records its child here before it can reap it; a run stopped before it started is terminated at once.
pub(super) fn started(run: &Held, pid: u32) {
    let mut run = run.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: pidfd_open takes a pid and no flags and returns a new descriptor or -1.
    let fd = unsafe { syscall(SYS_PIDFD_OPEN, i64::from(pid), 0i64) };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        eprintln!("flea: AirDrop holds owly's process {pid} by its pid alone: {error}");
        run.pid = Some(pid);
        if run.stopped {
            terminate_pid(pid);
        }
        return;
    }
    let fd = fd as i32;
    run.pidfd = Some(fd);
    let _ = LIVE.iter().any(|slot| {
        slot.compare_exchange(-1, fd, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    });
    arm();
    if run.stopped {
        terminate(fd);
    }
}

// The run's thread reaped its child; the pidfd closes under the lock, so no later Stop can reach anything.
pub(super) fn reaped(run: &Held) {
    let mut run = run.lock().unwrap_or_else(PoisonError::into_inner);
    run.pid = None;
    if let Some(fd) = run.pidfd.take() {
        forget(fd);
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if let Some(fd) = self.pidfd.take() {
            forget(fd);
        }
    }
}

fn forget(fd: i32) {
    for slot in &LIVE {
        let _ = slot.compare_exchange(fd, -1, Ordering::SeqCst, Ordering::SeqCst);
    }
    // SAFETY: the descriptor is this run's own pidfd, closed once.
    unsafe { close(fd) };
}

// SIGTERM lets `owly radio up` give Wi-Fi back; a pidfd never names a reused pid, and a process already gone answers ESRCH.
fn terminate(pidfd: i32) {
    // SAFETY: pidfd_send_signal takes the descriptor, the signal, a null info and no flags.
    unsafe {
        syscall(
            SYS_PIDFD_SEND_SIGNAL,
            i64::from(pidfd),
            i64::from(SIGTERM),
            0i64,
            0i64,
        )
    };
}

fn terminate_pid(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if pid > 1 {
        // SAFETY: kill takes a pid above 1 and a signal; the pid is this run's own child, not yet reaped.
        unsafe { kill(pid, SIGTERM) };
    }
}

// Polls the pidfds until each has exited or the time is up; syscalls only, so the signal handler can wait too.
fn wait_all(fds: &[i32; SLOTS], within: Duration) {
    let deadline = Instant::now() + within;
    let mut set = [const {
        PollFd {
            fd: -1,
            events: POLLIN,
            revents: 0,
        }
    }; SLOTS];
    for (entry, fd) in set.iter_mut().zip(fds) {
        entry.fd = *fd;
    }
    loop {
        // An exited run stays readable until reaped, and a closed descriptor answers POLLNVAL: either is done.
        if set.iter().all(|entry| entry.fd < 0) {
            return;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        let slice = (left.as_millis() as i32).clamp(1, SLICE_MS);
        // SAFETY: set is SLOTS live entries; poll skips negative descriptors.
        unsafe { poll(set.as_mut_ptr(), SLOTS, slice) };
        for entry in &mut set {
            if entry.revents != 0 {
                entry.fd = -1;
                entry.revents = 0;
            }
        }
    }
}

// Every live run in the process gets SIGTERM, then a bounded wait for owly's teardown.
pub fn teardown(within: Duration) {
    let mut fds = [-1; SLOTS];
    for (fd, slot) in fds.iter_mut().zip(&LIVE) {
        *fd = slot.load(Ordering::SeqCst);
        if *fd >= 0 {
            terminate(*fd);
        }
    }
    wait_all(&fds, within);
}

// Flea stopped by a signal writes the stops still pending and tears its owly runs down first, then dies by that same signal.
extern "C" fn on_signal(signum: i32) {
    super::airdrop_shared::settle_stops();
    teardown(TEARDOWN);
    // SAFETY: signal and raise are async-signal-safe; the default action ends the process as the signal meant to.
    unsafe {
        signal(signum, SIG_DFL);
        raise(signum);
    }
}

// The handler goes in once an owly run starts, a stop is asked or Receiving is set to On, whichever comes first.
pub(super) fn arm() {
    HANDLER.call_once(install);
}

// Only a signal still at its default is taken, so a handler or an ignore set elsewhere stays in place.
fn install() {
    for signum in [SIGTERM, SIGINT, SIGHUP] {
        // SAFETY: on_signal is an extern "C" fn(i32) doing only async-signal-safe work.
        let before = unsafe { signal(signum, on_signal as *const () as usize) };
        if before != SIG_DFL {
            // SAFETY: puts back the handler signal() just returned.
            unsafe { signal(signum, before) };
        }
    }
}

impl Drop for Pids {
    // A window that closes with AirDrop on hands the radio back and waits, bounded, until owly has done so.
    fn drop(&mut self) {
        let mut mine = [-1; SLOTS];
        for (fd, (_, run)) in mine.iter_mut().zip(self.0.iter().flatten()) {
            let run = run.lock().unwrap_or_else(PoisonError::into_inner);
            // SAFETY: a copy of the run's pidfd, so its thread closing the original never pulls it from under the wait.
            *fd = run.pidfd.map_or(-1, |pidfd| unsafe { dup(pidfd) });
        }
        for proc in Proc::ALL {
            self.stop(proc);
        }
        wait_all(&mine, TEARDOWN);
        for fd in mine.into_iter().filter(|fd| *fd >= 0) {
            // SAFETY: the copies made above, closed once.
            unsafe { close(fd) };
        }
    }
}

#[cfg(test)]
#[path = "airdrop_pids_tests.rs"]
mod tests;
