//! The machine's one AirDrop radio: the window that turned it on owns it under a lock in XDG_RUNTIME_DIR, and every other window reads it there.
use super::airdrop::Radio;
use std::fs::{DirBuilder, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

// flock(2) operations from Linux sys/file.h, the same on both supported architectures.
const LOCK_SH: i32 = 1;
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK: &str = "airdrop.lock";
// The owner's token, its radio and its Wi-Fi card, one per line.
const STATE: &str = "airdrop.state";
// Another window's Receiving Off: the owner's token, so it never stops a later owner.
const STOP: &str = "airdrop.stop";
const CLAIM_TRIES: u32 = 5;
// How long a quit waits for the stops still being written, and how often it looks.
const SETTLE: Duration = Duration::from_secs(1);
const SETTLE_SLICE: Duration = Duration::from_millis(10);
// Stops asked or withdrawn in this process and not yet written.
static UNWRITTEN: AtomicUsize = AtomicUsize::new(0);

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

pub(crate) struct Shared {
    // None keeps AirDrop to this window, as unit tests and a session without XDG_RUNTIME_DIR do.
    dir: Option<PathBuf>,
    // Held while this window owns the radio; the kernel lets it go when the process ends, however it ends.
    lock: Option<File>,
    // A lock the look before a Receiving On took, until that On takes it or gives it back.
    offered: Option<Claim>,
    token: String,
    // The process's count a quit waits on; a unit test's window counts on its own.
    unwritten: &'static AtomicUsize,
    // What this window changed in the machine's state, written off its thread in the order it changed.
    writes: Vec<Write>,
    // Closes once the window's last look, ask or write has run, so the next one waits for it.
    pub(super) io: Option<async_channel::Receiver<()>>,
    // A unit test's window writes each change at once, as its chain would before another window looks.
    #[cfg(test)]
    inline: bool,
}

// What one look at the machine's state found: another window's radio and card, or this owner's stop asked elsewhere.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) radio: Radio,
    pub(crate) card: String,
    // The owner token a stop named, read only for a window that owns the radio.
    stop: Option<String>,
}

// The reads and asks the window's thread never makes: a probe carries what they need to the background executor.
pub(crate) struct Probe {
    dir: Option<PathBuf>,
    // Some with this window's token while it holds the lock, so its look is for a stop asked of it.
    owner: Option<String>,
}

impl Default for Shared {
    fn default() -> Self {
        // Unit tests run many windows in one process, so each names its own directory through at().
        let dir = if cfg!(test) { None } else { runtime_dir() };
        Shared {
            dir,
            lock: None,
            offered: None,
            token: String::new(),
            unwritten: &UNWRITTEN,
            writes: Vec::new(),
            io: None,
            #[cfg(test)]
            inline: false,
        }
    }
}

// FLEA_AIRDROP_DIR isolates one test run when set; otherwise the session's own flea dir under XDG_RUNTIME_DIR.
fn runtime_dir() -> Option<PathBuf> {
    flea::userfile::env_dir("FLEA_AIRDROP_DIR")
        .or_else(|| flea::userfile::env_dir("XDG_RUNTIME_DIR").map(|run| run.join("flea")))
}

fn word(radio: Radio) -> &'static str {
    match radio {
        Radio::On => "on",
        _ => "starting",
    }
}

fn open_lock(dir: &Path) -> Option<File> {
    let made = DirBuilder::new().recursive(true).mode(0o700).create(dir);
    let opened = made.and_then(|()| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(dir.join(LOCK))
    });
    opened
        .inspect_err(|error| eprintln!("flea: AirDrop keeps to this window: {error}"))
        .ok()
}

fn try_flock(file: &File, operation: i32) -> bool {
    // SAFETY: flock takes the open file's descriptor and an operation; the lock lives as long as the File.
    unsafe { flock(file.as_raw_fd(), operation | LOCK_NB) == 0 }
}

// Sample input: "4242-1731100000000000000\non\nIntel AX210\n", the owner's token, its radio and its card.
fn state(dir: &Path) -> Option<(String, Radio, String)> {
    let body = std::fs::read_to_string(dir.join(STATE)).ok()?;
    let mut lines = body.lines();
    let token = lines.next().filter(|token| !token.is_empty())?.to_string();
    let radio = match lines.next() {
        Some("on") => Radio::On,
        _ => Radio::Starting,
    };
    Some((token, radio, lines.next().unwrap_or_default().to_string()))
}

// Sample input: "4242-1731100000000000000", the token of the owner another window asked to stop.
fn stop_names(dir: &Path, token: &str) -> bool {
    std::fs::read_to_string(dir.join(STOP)).is_ok_and(|asked| asked.trim() == token)
}

impl Shared {
    #[cfg(test)]
    pub(crate) fn at(dir: PathBuf) -> Self {
        Shared {
            dir: Some(dir),
            lock: None,
            offered: None,
            token: String::new(),
            unwritten: Box::leak(Box::new(AtomicUsize::new(0))),
            writes: Vec::new(),
            io: None,
            inline: true,
        }
    }

    // A unit test's window whose writes wait, as in the product, until its chain runs them.
    #[cfg(test)]
    pub(crate) fn queued(dir: PathBuf) -> Self {
        let mut shared = Shared::at(dir);
        shared.inline = false;
        shared
    }

    #[cfg(test)]
    pub(crate) fn unwritten(&self) -> &'static AtomicUsize {
        self.unwritten
    }

    // Taken when a stop is asked or withdrawn, and dropped once its file is written; a SIGTERM waits for it too.
    pub(crate) fn hold(&self) -> Unwritten {
        self.unwritten.fetch_add(1, Ordering::SeqCst);
        super::airdrop_pids::arm();
        Unwritten(self.unwritten)
    }

    // Other windows can see this window's radio and ask it to stop.
    pub(crate) fn machine_wide(&self) -> bool {
        self.dir.is_some()
    }

    // Taken on the window's thread and run off it, so the look sees the lock as this window held it when asked.
    pub(crate) fn probe(&self) -> Probe {
        Probe {
            dir: self.dir.clone(),
            owner: self.lock.as_ref().map(|_| self.token.clone()),
        }
    }

    // The lock a look took for a Receiving On, held until that On takes it or the next writes give it back.
    pub(crate) fn offer(&mut self, lock: Option<Claim>) {
        if let Some(Claim::Lock(spare)) = std::mem::replace(&mut self.offered, lock) {
            self.push(Write::Release(spare));
        }
    }

    fn push(&mut self, write: Write) {
        self.writes.push(write);
        #[cfg(test)]
        if self.inline {
            let writes = self.drain();
            self.run(writes);
        }
    }

    // The radio's run has exited: this window's lock is offered to a Receiving On asked meanwhile.
    pub(crate) fn hand_over(&mut self) {
        let lock = self.lock.take();
        self.token.clear();
        self.offer(lock.map(Claim::Lock));
    }

    // True when this window now owns the radio under the lock offered to it, or when there is no shared state to own.
    pub(crate) fn adopt(&mut self, card: &str) -> bool {
        if self.lock.is_some() || self.dir.is_none() {
            return true;
        }
        let Some(lock) = self.offered.take() else {
            return false;
        };
        let Claim::Lock(lock) = lock else {
            return true;
        };
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        self.token = format!("{}-{since}", std::process::id());
        self.lock = Some(lock);
        self.state(Radio::Starting, card, true);
        true
    }

    fn state(&mut self, radio: Radio, card: &str, fresh: bool) {
        if self.lock.is_some() {
            let body = format!("{}\n{}\n{card}\n", self.token, word(radio));
            self.push(Write::State { body, fresh });
        }
    }

    // The owner's radio, written whole under a temporary name, so a reader never sees half a state.
    pub(crate) fn publish(&mut self, radio: Radio, card: &str) {
        self.state(radio, card, false);
    }

    // The radio is going down: every window reads Off from here, while the lock stays held until owly has exited.
    pub(crate) fn withdraw(&mut self) {
        if self.lock.is_none() {
            return;
        }
        self.push(Write::Withdraw);
        self.token.clear();
    }

    // owly has exited: the state goes first, then the lock, so the next owner never meets this one's state.
    pub(crate) fn release(&mut self) {
        self.hand_over();
        self.offer(None);
    }

    // The writes this window's thread queued, with an offered lock no On took given back last.
    pub(crate) fn writes(&mut self) -> Option<(PathBuf, Vec<Write>)> {
        self.offer(None);
        self.drain()
    }

    fn drain(&mut self) -> Option<(PathBuf, Vec<Write>)> {
        let writes = std::mem::take(&mut self.writes);
        let dir = self.dir.clone()?;
        (!writes.is_empty()).then_some((dir, writes))
    }

    // A unit test writes what the window's chain would, in the same order, on its own thread.
    #[cfg(test)]
    pub(crate) fn flush(&mut self) {
        let writes = self.writes();
        self.run(writes);
    }

    #[cfg(test)]
    fn run(&self, writes: Option<(PathBuf, Vec<Write>)>) {
        if let Some((dir, writes)) = writes {
            writes.into_iter().for_each(|write| write.run(&dir));
        }
    }

    // A unit test's Receiving On: the look's claim, then the window's own step, then its writes.
    #[cfg(test)]
    pub(crate) fn claim(&mut self, card: &str) -> bool {
        if self.lock.is_none() {
            self.offer(self.probe().claim());
        }
        let owned = self.adopt(card);
        self.flush();
        owned
    }

    // The owner's look: another window asked this owner, and not an earlier one, to stop.
    pub(crate) fn stop_asked(&self, seen: &Seen) -> bool {
        self.lock.is_some()
            && !self.token.is_empty()
            && seen.stop.as_deref() == Some(self.token.as_str())
    }
}

impl Probe {
    // The radio another window holds, with its card; an owner sees only a stop asked of it.
    pub(crate) fn seen(&self) -> Seen {
        let Some(dir) = self.dir.as_deref() else {
            return Seen::default();
        };
        if let Some(token) = &self.owner {
            let stop = (!token.is_empty() && stop_names(dir, token)).then(|| token.clone());
            return Seen {
                stop,
                ..Seen::default()
            };
        }
        let Some(lock) = open_lock(dir) else {
            return Seen::default();
        };
        // Free for a shared lock means no owner is alive, whatever an old state file says.
        if try_flock(&lock, LOCK_SH) {
            return Seen::default();
        }
        // No state is an owner letting go, and a stop asked of this owner already reads Off in every window.
        match state(dir) {
            Some((token, radio, card)) if !stop_names(dir, &token) => Seen {
                radio,
                card,
                stop: None,
            },
            _ => Seen::default(),
        }
    }

    // Before a Receiving On, with no radio up: the machine's lock, which the window's thread then takes or gives back.
    pub(crate) fn claim(&self) -> Option<Claim> {
        let Some(lock) = self.dir.as_deref().and_then(open_lock) else {
            return Some(Claim::Local);
        };
        // A reader's look holds a shared lock for microseconds, so a few quick tries tell it from a live owner.
        let mut tries = 0;
        while !try_flock(&lock, LOCK_EX) {
            tries += 1;
            if tries == CLAIM_TRIES {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // An owner that died left its state behind, which no window may read as this one's.
        let _ = std::fs::remove_file(self.dir.as_deref()?.join(STATE));
        Some(Claim::Lock(lock))
    }

    // Receiving set to Off in a window that does not own the radio: the owner stops it on its next look.
    pub(crate) fn ask_stop(&self) {
        let Some(dir) = self.dir.as_deref() else {
            return;
        };
        if let Some((token, ..)) = state(dir) {
            let _ = std::fs::write(dir.join(STOP), token);
        }
    }

    // Receiving turned back on before the owner looked: the ask to stop is withdrawn.
    pub(crate) fn keep(&self) {
        if let Some(dir) = self.dir.as_deref() {
            let _ = std::fs::remove_file(dir.join(STOP));
        }
    }
}

pub(crate) struct Unwritten(&'static AtomicUsize);

impl Drop for Unwritten {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

// A stop asked just before the last window closes lands before Flea exits; an atomic, the clock and nanosleep only, so the SIGTERM handler waits the same way.
pub fn settle_stops() {
    wait_written(&UNWRITTEN, SETTLE);
}

fn wait_written(unwritten: &AtomicUsize, within: Duration) {
    let until = Instant::now() + within;
    while unwritten.load(Ordering::SeqCst) > 0 && Instant::now() < until {
        std::thread::sleep(SETTLE_SLICE);
    }
}

#[path = "airdrop_writes.rs"]
mod writes;
pub(crate) use writes::{Claim, Write};

#[cfg(test)]
#[path = "airdrop_shared_tests.rs"]
mod tests;
