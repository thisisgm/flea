// The loop reads stdin itself; a parked helper takes over after TAKEOVER so urgent cancellation survives a slow loop.
use super::events::{feed, Event};
use super::opscancel::Live;
use super::wake::{read, wait, waking, watch, Counter, Sender, Timer, Wake, EINTR};
use crate::error::{from_io, FleaError};
use std::io;
use std::sync::atomic::{fence, AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

// Far past any ordinary request, and past the scheduler tick so arming it reprograms no clock; short beside a stuck mount.
pub(crate) const TAKEOVER: Duration = Duration::from_millis(50);
// std's stdin buffer size, so a burst of lines costs the reads the reader thread's BufReader made.
const CHUNK: usize = 8192;
pub(crate) const STDIN: i32 = 0;
const EBADF: i32 = 9;

fn lock(tail: &Mutex<Vec<u8>>) -> MutexGuard<'_, Vec<u8>> {
    tail.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// What the loop and the helper share: holding the tail's lock is the right to read the input.
struct Shared {
    input: i32,
    tail: Mutex<Vec<u8>>,
    open: AtomicBool,
    busy: AtomicBool,
    engaged: AtomicBool,
    timer: Timer,
    stop: Counter,
    tx: Sender<Event>,
    live: Arc<Live>,
    #[cfg(test)]
    takeovers: std::sync::atomic::AtomicUsize,
}

impl Shared {
    // Reads what the input holds now and sends each whole line in arrival order; it reads only once poll says it will not block.
    fn pump(&self, tail: &mut Vec<u8>, ready: bool) {
        if !self.open.load(Ordering::Relaxed) || !(ready || wait(&mut [watch(self.input)], 0)) {
            return;
        }
        let mut chunk = [0u8; CHUNK];
        let read = loop {
            let n = unsafe { read(self.input, chunk.as_mut_ptr().cast(), CHUNK) };
            if n >= 0 {
                break Ok(n as usize);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(EINTR) {
                break Err(error);
            }
        };
        let open = match read {
            // std reads a closed fd 0 as the end of input, and so does this.
            Err(error) if error.raw_os_error() == Some(EBADF) => {
                feed(tail, &[], true, &self.tx, &self.live)
            }
            Err(error) => {
                let _ = self
                    .tx
                    .send(Event::ReadError(from_io("read", "stdin", &error)));
                false
            }
            Ok(n) => feed(tail, &chunk[..n], n == 0, &self.tx, &self.live),
        };
        if !open {
            self.open.store(false, Ordering::Relaxed);
        }
    }

    // About to sleep: the takeover timer stops, and a helper that took over parks, as the loop reads the input itself again.
    fn disarm(&self) {
        self.busy.store(false, Ordering::Relaxed);
        self.timer.set(Duration::ZERO);
        fence(Ordering::SeqCst);
        if self.engaged.load(Ordering::Relaxed) {
            self.stop.ring();
        }
    }

    fn arm(&self) {
        self.busy.store(true, Ordering::Relaxed);
        self.timer.set(TAKEOVER);
    }
}

// Parked on the timer while the loop answers on time; once one busy stretch outlasts TAKEOVER it reads the input until the loop sleeps.
fn helper(shared: &Shared) {
    while shared.timer.wait() {
        shared.engaged.store(true, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        #[cfg(test)]
        shared.takeovers.fetch_add(1, Ordering::Relaxed);
        while shared.busy.load(Ordering::Relaxed) && shared.open.load(Ordering::Relaxed) {
            let mut fds = [watch(shared.stop.fd()), watch(shared.input)];
            wait(&mut fds, -1);
            if fds[0].revents != 0 {
                shared.stop.clear();
            }
            if fds[1].revents != 0 {
                shared.pump(&mut lock(&shared.tail), false);
            }
        }
        shared.engaged.store(false, Ordering::Relaxed);
    }
}

// The loop's channel and its doorbell, opened before anything can send; start adds the input once the cancel registry exists.
pub struct Pending {
    wake: Arc<Wake>,
    timer: Timer,
    stop: Counter,
}

pub fn open() -> Result<(Sender<Event>, Receiver<Event>, Pending), FleaError> {
    let wake = Wake::new(Counter::open().map_err(|error| from_io("open", "eventfd", &error))?);
    let timer = Timer::open().map_err(|error| from_io("open", "timerfd", &error))?;
    let stop = Counter::open().map_err(|error| from_io("open", "eventfd", &error))?;
    let (tx, rx) = waking(&wake);
    Ok((tx, rx, Pending { wake, timer, stop }))
}

impl Pending {
    pub fn start(self, input: i32, tx: Sender<Event>, live: Arc<Live>) -> Inbox {
        let shared = Arc::new(Shared {
            input,
            tail: Mutex::new(Vec::new()),
            open: AtomicBool::new(true),
            busy: AtomicBool::new(true),
            engaged: AtomicBool::new(false),
            timer: self.timer,
            stop: self.stop,
            tx,
            live,
            #[cfg(test)]
            takeovers: std::sync::atomic::AtomicUsize::new(0),
        });
        shared.timer.set(TAKEOVER);
        let parked = Arc::clone(&shared);
        std::thread::spawn(move || helper(&parked));
        Inbox {
            shared,
            wake: self.wake,
            handed: false,
        }
    }
}

pub struct Inbox {
    shared: Arc<Shared>,
    wake: Arc<Wake>,
    handed: bool,
}

impl Inbox {
    // The next event in arrival order, sleeping on the input and the doorbell while there is none; None once no sender is left.
    pub fn next(&mut self, rx: &Receiver<Event>) -> Option<Event> {
        match rx.try_recv() {
            // A busy loop reads the input before each event it takes, so a request queues where the reader thread put it.
            Ok(event) => {
                self.shared.pump(&mut lock(&self.shared.tail), false);
                return Some(event);
            }
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {}
        }
        loop {
            self.handed = false;
            let shared = &*self.shared;
            let mut tail = lock(&shared.tail);
            shared.disarm();
            self.wake.sleep();
            let last = rx.try_recv();
            if !matches!(last, Err(TryRecvError::Empty)) {
                self.wake.woke();
                shared.arm();
                return last.ok();
            }
            let input = if shared.open.load(Ordering::Relaxed) {
                shared.input
            } else {
                -1
            };
            let mut fds = [watch(self.wake.counter.fd()), watch(input)];
            wait(&mut fds, -1);
            self.wake.woke();
            shared.arm();
            if fds[0].revents != 0 {
                self.wake.counter.clear();
            }
            if fds[1].revents != 0 {
                shared.pump(&mut tail, true);
            }
            drop(tail);
            match rx.try_recv() {
                Ok(event) => return Some(event),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    // Search ticks never sleep, so the helper reads the input for them at once, as the reader thread did.
    pub fn hand_over(&mut self) {
        if !self.handed {
            self.handed = true;
            self.shared.busy.store(true, Ordering::Relaxed);
            self.shared.timer.set(Duration::from_nanos(1));
        }
    }

    #[cfg(test)]
    pub(crate) fn takeovers(&self) -> usize {
        self.shared.takeovers.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;
