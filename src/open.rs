use crate::thp;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// The exit statuses ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;
pub const IS_DIRECTORY: i32 = 3;

// Canonical, so a file named --output=/etc/x cannot be read as a flag by the child.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// gio open is the OEM route: it asks the desktop database, so Terminal=true is honoured; see AGENTS.md "Opening a file".
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
    // corner: waited for, not detached, and on an archive that wait is a cold handler start; see AGENTS.md "Opening a file".
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
            eprintln!("flea: gio open refused that file, so no application on this system took it");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file");
            FAILED
        }
    }
}

// The Open With handoff: gio launch runs the desktop entry the context menu named, once, and writes
// no default anywhere; the operator picks an app and the desktop's own registry is untouched.
pub fn open_with(path: &str, desktop: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        None => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if target.is_dir() {
        return IS_DIRECTORY;
    }
    // The backend hands the menu absolute paths it resolved from the applications dirs itself, so a
    // relative one is a client bug rather than input to resolve: gio would read it against this
    // process's cwd, which no caller knows.
    if !Path::new(desktop).is_absolute() {
        eprintln!("flea: the application to open with must be named by an absolute desktop entry path");
        return FAILED;
    }
    thp::enable();
    let finished = Command::new("gio")
        .arg("launch")
        .arg(desktop)
        .arg(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .status();
    match finished {
        Ok(status) if status.success() => 0,
        Ok(_) => {
            eprintln!("flea: gio launch refused that application, so nothing was opened with it");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file");
            FAILED
        }
    }
}
