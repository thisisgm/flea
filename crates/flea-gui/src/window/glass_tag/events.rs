//! Hyprland's event socket, which runs one pass at start and one per config reload or own window opening.
use super::retags;
use super::run::{warn_once, Sources, Tagger};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// The process's one tagger, so the window can ask it for a pass when its glass inputs change.
static TAGGER: OnceLock<Tagger> = OnceLock::new();

// Hyprland names its event socket this way inside its instance directory.
const SOCKET_NAME: &str = ".socket2.sock";

/// Hyprland's event socket, placed under XDG_RUNTIME_DIR, or under /tmp when that is unset.
pub(crate) fn socket2_path(runtime: Option<&OsStr>, signature: &OsStr) -> PathBuf {
    let base = runtime
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    base.join("hypr").join(signature).join(SOCKET_NAME)
}

/// Starts the tagger when this process runs under Hyprland; a headless run has no compositor to ask.
pub fn start(inputs: crate::window::glass::Inputs) {
    let Some(signature) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") else {
        return;
    };
    let socket = socket2_path(std::env::var_os("XDG_RUNTIME_DIR").as_deref(), &signature);
    let tagger = TAGGER
        .get_or_init(|| {
            Tagger::new(
                std::process::id(),
                OsString::from("hyprctl"),
                Sources::from_env(),
            )
        })
        .clone();
    tagger.follow(inputs);
    let _ = std::thread::Builder::new()
        .name("flea-glass-events".into())
        .spawn(move || listen(&tagger, &socket));
}

#[cfg(test)]
thread_local! {
    // The inputs this thread's window last handed over, so a test sees the hand-off without Hyprland.
    pub(crate) static HANDED: std::cell::Cell<Option<crate::window::glass::Inputs>> =
        const { std::cell::Cell::new(None) };
}

/// Hands the window's applied glass inputs to the tagger; nothing happens off Hyprland, where none was started.
pub fn follow(inputs: crate::window::glass::Inputs) {
    #[cfg(test)]
    HANDED.with(|handed| handed.set(Some(inputs)));
    if let Some(tagger) = TAGGER.get() {
        tagger.follow(inputs);
    }
}

/// Removes this process's Glassy rule after the application's event loop returns normally.
pub fn stop() {
    if let Some(tagger) = TAGGER.get() {
        tagger.stop();
    }
}

// The first pass covers a window already open; after that each config reload or own window opening asks again.
fn listen(tagger: &Tagger, socket: &Path) {
    let stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(error) => {
            warn_once(&format!(
                "no Hyprland event socket at {}: {error}",
                socket.display()
            ));
            return;
        }
    };
    tagger.retag();
    read_events(BufReader::new(stream), socket, || tagger.retag());
}

/// Asks for a pass on each line that retags; a title Hyprland passed as raw bytes decodes lossily, and only a read error stops.
pub(crate) fn read_events(reader: impl BufRead, socket: &Path, mut retag: impl FnMut()) {
    for line in reader.split(b'\n') {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!(
                    "flea-gui: window transparency tag stopped: reading {}: {error}",
                    socket.display()
                );
                return;
            }
        };
        if retags(&String::from_utf8_lossy(&line)) {
            retag();
        }
    }
}
