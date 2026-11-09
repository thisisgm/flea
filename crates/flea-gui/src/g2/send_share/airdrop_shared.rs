//! The machine's one AirDrop radio: the window that turned it on owns it under a lock in XDG_RUNTIME_DIR, and every other window reads it there.
use super::airdrop::Radio;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;

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

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

pub(crate) struct Shared {
    // None keeps AirDrop to this window, as unit tests and a session without XDG_RUNTIME_DIR do.
    dir: Option<PathBuf>,
    // Held while this window owns the radio; the kernel lets it go when the process ends, however it ends.
    lock: Option<File>,
    token: String,
}

impl Default for Shared {
    fn default() -> Self {
        // Unit tests run many windows in one process, so each names its own directory through at().
        let dir = if cfg!(test) { None } else { runtime_dir() };
        Shared {
            dir,
            lock: None,
            token: String::new(),
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

impl Shared {
    #[cfg(test)]
    pub(crate) fn at(dir: PathBuf) -> Self {
        Shared {
            dir: Some(dir),
            lock: None,
            token: String::new(),
        }
    }

    // Other windows can see this window's radio and ask it to stop.
    pub(crate) fn machine_wide(&self) -> bool {
        self.dir.is_some()
    }

    fn file(&self, name: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(name))
    }

    fn open_lock(&self) -> Option<File> {
        let dir = self.dir.as_ref()?;
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

    // The radio another window holds, with its card; Off when no window owns one.
    pub(crate) fn read(&self) -> (Radio, String) {
        if self.lock.is_some() {
            return (Radio::Off, String::new());
        }
        let Some(lock) = self.open_lock() else {
            return (Radio::Off, String::new());
        };
        // Free for a shared lock means no owner is alive, whatever an old state file says.
        if Self::try_flock(&lock, LOCK_SH) {
            return (Radio::Off, String::new());
        }
        // No state is an owner letting go, and a stop asked of this owner already reads Off in every window.
        match self.state() {
            Some((token, radio, card)) if !self.stop_names(&token) => (radio, card),
            _ => (Radio::Off, String::new()),
        }
    }

    // Sample input: "4242-1731100000000000000\non\nIntel AX210\n", the owner's token, its radio and its card.
    fn state(&self) -> Option<(String, Radio, String)> {
        let body = std::fs::read_to_string(self.file(STATE)?).ok()?;
        let mut lines = body.lines();
        let token = lines.next().filter(|token| !token.is_empty())?.to_string();
        let radio = match lines.next() {
            Some("on") => Radio::On,
            _ => Radio::Starting,
        };
        Some((token, radio, lines.next().unwrap_or_default().to_string()))
    }

    // Sample input: "4242-1731100000000000000", the token of the owner another window asked to stop.
    fn stop_names(&self, token: &str) -> bool {
        self.file(STOP)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .is_some_and(|asked| asked.trim() == token)
    }

    // True when this window now owns the radio, or when there is no shared state to own.
    pub(crate) fn claim(&mut self, card: &str) -> bool {
        if self.dir.is_none() || self.lock.is_some() {
            return true;
        }
        let Some(lock) = self.open_lock() else {
            return true;
        };
        // A reader's look holds a shared lock for microseconds, so a few quick tries tell it from a live owner.
        let mut tries = 0;
        while !Self::try_flock(&lock, LOCK_EX) {
            tries += 1;
            if tries == CLAIM_TRIES {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        self.token = format!("{}-{since}", std::process::id());
        self.lock = Some(lock);
        if let Some(stop) = self.file(STOP) {
            let _ = std::fs::remove_file(stop);
        }
        self.publish(Radio::Starting, card);
        true
    }

    // The owner writes its radio whole under a temporary name, so a reader never sees half a state.
    pub(crate) fn publish(&self, radio: Radio, card: &str) {
        let (Some(_), Some(path), Some(temp)) =
            (&self.lock, self.file(STATE), self.file("airdrop.state.new"))
        else {
            return;
        };
        let body = format!("{}\n{}\n{card}\n", self.token, word(radio));
        let written = File::create(&temp)
            .and_then(|mut f| f.write_all(body.as_bytes()))
            .and_then(|()| std::fs::rename(&temp, &path));
        if let Err(error) = written {
            eprintln!("flea: AirDrop could not share its state: {error}");
        }
    }

    // The radio is going down: every window reads Off from here, while the lock stays held until owly has exited.
    pub(crate) fn withdraw(&mut self) {
        if self.lock.is_none() {
            return;
        }
        for name in [STATE, STOP] {
            if let Some(path) = self.file(name) {
                let _ = std::fs::remove_file(path);
            }
        }
        self.token.clear();
    }

    // owly has exited: the state goes first, then the lock, so the next owner never meets this one's state.
    pub(crate) fn release(&mut self) {
        self.withdraw();
        self.lock = None;
    }

    // Receiving set to Off in a window that does not own the radio: the owner stops it on its next look.
    pub(crate) fn ask_stop(&self) {
        if let (Some((token, ..)), Some(path)) = (self.state(), self.file(STOP)) {
            let _ = std::fs::write(path, token);
        }
    }

    // Receiving turned back on before the owner looked: the ask to stop is withdrawn.
    pub(crate) fn keep(&self) {
        if let Some(path) = self.file(STOP) {
            let _ = std::fs::remove_file(path);
        }
    }

    // The owner's look: another window asked it to stop.
    pub(crate) fn stop_asked(&self) -> bool {
        self.lock.is_some() && !self.token.is_empty() && self.stop_names(&self.token)
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
#[path = "airdrop_shared_tests.rs"]
mod tests;
