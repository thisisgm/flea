use crate::gui;
use crate::thp;
use crate::vulkan;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// The exit status ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;

/// Resolve a relative path or symlink to the directory the terminal should use.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// Open a terminal in `path`, preferring the desktop's `xdg-terminal-exec`.
/// If that launcher is missing, try Alacritty, Kitty, then xterm in the resolved directory.
/// Return [`FAILED`] when the path is invalid or no terminal can be launched.
pub fn open_terminal(path: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        // The reason is elided, never shown raw, and the path is the user's own input.
        None => {
            eprintln!("flea: that directory could not be opened in a terminal, check that it still exists");
            return FAILED;
        }
    };
    if !target.is_dir() {
        eprintln!("flea: that directory could not be opened in a terminal, check that it still exists");
        return FAILED;
    }
    // An OsString and not a format!, because Path::display would substitute U+FFFD for a byte that is not UTF-8.
    let mut dir = OsString::from("--dir=");
    dir.push(&target);
    // corner: spawn and not exec, because the terminal outlives us; see AGENTS.md "Opening a file".
    let mut terminal = Command::new("xdg-terminal-exec");
    detach(&mut terminal);
    let started = terminal.arg(&dir).spawn().or_else(|error| {
        if error.kind() != std::io::ErrorKind::NotFound { return Err(error); }
        // Desktop setups without xdg-terminal-exec can still use a local emulator.
        let mut alacritty = Command::new("alacritty");
        alacritty.arg("--working-directory").arg(&target);
        let mut kitty = Command::new("kitty");
        kitty.arg("--directory").arg(&target);
        let mut xterm = Command::new("xterm");
        xterm.current_dir(&target);
        for candidate in [&mut alacritty, &mut kitty, &mut xterm] {
            detach(candidate);
            match candidate.spawn() {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                result => return result,
            }
        }
        Err(error)
    });
    match started {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open a terminal there");
            FAILED
        }
    }
}

/// Apply the process guards to a child that Flea starts without waiting for it.
/// The updater in `src/update.rs` uses these guards too.
pub fn detach(child: &mut Command) {
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // The display-GPU pin is Qt's alone, and only this launcher's own pin is dropped.
    vulkan::drop_display_pin(child);
    // The platform theme Flea traded for its own startup is Qt's alone too, and is handed back here.
    gui::restore_platform_theme(child);
    // The child outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
    child.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Its own process group, so nothing that later kills Flea's group reaches the child.
    child.process_group(0);
}
