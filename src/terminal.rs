use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// The exit status ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;

// Canonical, so a relative path and a symlink both name the one real directory the terminal sits in.
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
            eprintln!("flea: that folder could not be opened in a terminal; check that it still exists");
            return FAILED;
        }
    };
    if !target.is_dir() {
        eprintln!("flea: that folder could not be opened in a terminal; check that it still exists");
        return FAILED;
    }
    // An OsString and not a format!, because Path::display would substitute U+FFFD for a byte that is not UTF-8.
    let mut dir = OsString::from("--dir=");
    dir.push(&target);
    // corner: spawn and not exec, because the terminal outlives us; see AGENTS.md "Opening a file".
    let mut terminal = Command::new("xdg-terminal-exec");
    detach(&mut terminal);
    let started = terminal.arg(&dir).spawn();
    match started {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open a terminal there");
            FAILED
        }
    }
}

// The same OEM launcher with a command, passed as argv after the option terminator.
pub(crate) fn open_argv(directory: &std::path::Path, argv: &[String]) -> std::io::Result<()> {
    if argv.is_empty() {
        return Err(std::io::Error::other("empty terminal action"));
    }
    let target = directory.canonicalize()?;
    let mut dir = OsString::from("--dir=");
    dir.push(target);
    let mut terminal = Command::new("xdg-terminal-exec");
    detach(&mut terminal);
    terminal.arg(dir).arg("--").args(argv).spawn().map(|_| ())
}

// The guards every program Flea starts and does not wait for carries; src/update.rs hands its updater the same.
pub fn detach(child: &mut Command) {
    // One helper owns the environment, so a new guard reaches every launcher at once; see src/open.rs.
    crate::open::prepare(child);
    // The child outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
    child.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Its own process group, so nothing that later kills Flea's group reaches the child.
    child.process_group(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn a_detached_child_loses_every_tear_off_variable() {
        let mut child = Command::new(std::env::current_exe().unwrap());
        for name in crate::tearoff::ENV {
            child.env(name, "stale");
        }
        detach(&mut child);
        for name in crate::tearoff::ENV {
            let entry = child.get_envs().find(|(key, _)| *key == OsStr::new(name));
            assert_eq!(entry, Some((OsStr::new(name), None)), "{name} survived detach");
        }
    }
}
