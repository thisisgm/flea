use crate::thp;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// The exit statuses ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;
pub const IS_DIRECTORY: i32 = 3;

// Canonical, so a file named --output=/etc/x cannot be read as a flag by the child.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// gio open is the OEM route: it asks the desktop database, so a handler declaring Terminal=true is
// run inside the terminal glib picks, which xdg-open does not do; see AGENTS.md "Opening a file".
pub fn open(path: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        // The reason is elided, never shown raw, and the path is the user's own input.
        None => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if target.is_dir() {
        return IS_DIRECTORY;
    }
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // corner: waited for and not detached, because gio open launches the handler and returns at once;
    // measured at 10 ms on this box against a handler that then ran for five seconds of its own.
    let finished = Command::new("gio")
        .arg("open")
        .arg(&target)
        // The handler outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the opened program.
        .process_group(0)
        .status();
    match finished {
        Ok(status) if status.success() => 0,
        // A launcher that refused, which a spawn nobody waited on used to report as a clean handoff.
        Ok(_) => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file");
            FAILED
        }
    }
}
