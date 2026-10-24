//! Hyprland's event socket, which runs one pass at start and one per config reload or own window opening.
use super::retags;
use super::run::{warn_once, Sources, Tagger};
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

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
pub fn start() {
    let Some(signature) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") else {
        return;
    };
    let socket = socket2_path(std::env::var_os("XDG_RUNTIME_DIR").as_deref(), &signature);
    let tagger = Tagger::new(
        std::process::id(),
        OsString::from("hyprctl"),
        Sources::from_env(),
    );
    let _ = std::thread::Builder::new()
        .name("flea-glass-events".into())
        .spawn(move || listen(&tagger, &socket));
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
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else {
            return;
        };
        if retags(&line) {
            tagger.retag();
        }
    }
}
