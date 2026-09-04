use crate::thp;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// The exit status ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;

// Canonical, so a directory named --output=/etc/x cannot be read as a flag by the child.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// xdg-terminal-exec is the OEM route: `omarchy default terminal` configures what it reads,
// and --dir= names the working directory without taking a command.
pub fn open_terminal(path: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        // The reason is elided, never shown raw, and the path is the user's own input.
        None => {
            eprintln!("flea: that terminal could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if !target.is_dir() {
        eprintln!("flea: that terminal could not be opened, check that it still exists");
        return FAILED;
    }
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // corner: spawn and not exec, because the terminal outlives us; see AGENTS.md "Opening a file".
    let started = Command::new("xdg-terminal-exec")
        .arg(format!("--dir={}", target.display()))
        // The terminal outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the terminal.
        .process_group(0)
        .spawn();
    match started {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open a terminal there");
            FAILED
        }
    }
}
