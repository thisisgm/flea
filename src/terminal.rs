use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
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
        Some(p) if p.is_dir() => Some(p),
        // A file row, a dangling link included, opens in the folder that lists it, not its link target's.
        _ if Path::new(path).symlink_metadata().is_ok() => listing_folder(path),
        _ => None,
    };
    let Some(target) = target.filter(|p| p.is_dir()) else {
        // The reason is elided, never shown raw, and the path is the user's own input.
        eprintln!(
            "flea: that folder could not be opened in a terminal; check that it still exists"
        );
        return FAILED;
    };
    // An OsString and not a format!, because Path::display would substitute U+FFFD for a byte that is not UTF-8.
    let mut dir = OsString::from("--dir=");
    dir.push(&target);
    // corner: spawn and not exec, because the terminal outlives us; see AGENTS.md "Opening a file".
    let mut terminal = match resolved_command(&dir) {
        Some(argv) => {
            let mut command = Command::new(&argv[0]);
            // The launcher's own cd, for an entry that names no directory option.
            command.args(&argv[1..]).current_dir(&target);
            command
        }
        None => {
            let mut command = Command::new("xdg-terminal-exec");
            command.arg(&dir);
            command
        }
    };
    detach(&mut terminal);
    match terminal.spawn() {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open a terminal there");
            FAILED
        }
    }
}

// A bare file name is listed in the folder flea was started in.
fn listing_folder(path: &str) -> Option<PathBuf> {
    match Path::new(path).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.canonicalize().ok(),
        _ => resolved("."),
    }
}

// A running single-instance Ghostty drops --working-directory, so its own launch is asked for; #373.
fn resolved_command(dir: &OsStr) -> Option<Vec<OsString>> {
    let printed = Command::new("xdg-terminal-exec")
        .arg("--print-cmd=\\0")
        .arg(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let mut argv = command_from(&printed.stdout)?;
    if Path::new(&argv[0]).file_name() == Some(OsStr::new("ghostty")) {
        argv.push("--gtk-single-instance=false".into());
    }
    Some(argv)
}

// Sample input: b"ghostty\0--gtk-single-instance=true\0--working-directory=/home/u/src"
fn command_from(printed: &[u8]) -> Option<Vec<OsString>> {
    let argv: Vec<OsString> = printed
        .split(|b| *b == 0)
        .map(|word| OsStr::from_bytes(word).to_os_string())
        .collect();
    (!argv[0].is_empty()).then_some(argv)
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
    terminal
        .arg(dir)
        .arg("--")
        .args(flea::bytename::args(argv))
        .spawn()
        .map(|_| ())
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
