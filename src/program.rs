use crate::thp;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// The exit statuses ui/Opener.qml reads. 0 is a started program and needs no name.
pub const FAILED: i32 = 2;
pub const IS_DIRECTORY: i32 = 3;
pub const NOT_EXECUTABLE: i32 = 4;

// The same three bits ui/js/Format.js reads off the listing's mode, so what the window offers to
// start and what this mode agrees to start cannot drift apart.
const ANY_EXECUTE_BIT: u32 = 0o111;

// Canonical, so a symlink starts its target rather than itself, and the absolute result cannot be
// read as a flag by the child.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// Starting a program is not opening a file, and it is the one thing gio open cannot do here: the
// desktop database has no handler for an AppImage on a stock box, so the desktop's own route
// answers "no application took it" for the file the operator has already marked executable. This
// carries the same three guards src/open.rs does, and spawns rather than waits, because the program
// outlives Flea the way src/terminal.rs's terminal does. See AGENTS.md "Running a program".
pub fn run(path: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        // The reason is elided, never shown raw, and the path is the user's own input.
        None => {
            eprintln!("flea: that program could not be started, check that it still exists");
            return FAILED;
        }
    };
    // One stat for both refusals, taken after canonicalize so it is the target's own and not a link's.
    let mode = match std::fs::metadata(&target) {
        Ok(meta) if meta.is_dir() => return IS_DIRECTORY,
        // A device or a socket carrying an execute bit is not a program, so the kind is asked first.
        Ok(meta) if meta.is_file() => meta.permissions().mode(),
        Ok(_) => {
            eprintln!("flea: that is not a program, so Flea will not run it");
            return NOT_EXECUTABLE;
        }
        Err(_) => {
            eprintln!("flea: that program could not be started, check that it still exists");
            return FAILED;
        }
    };
    // The execute bit is the whole permission to run: Flea never sets one to satisfy itself, because
    // marking a download executable is the operator's decision and the Permissions dialog is where
    // they make it.
    if mode & ANY_EXECUTE_BIT == 0 {
        eprintln!("flea: that file carries no execute bit, so Flea will not run it");
        return NOT_EXECUTABLE;
    }
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // Its own folder, because a program dropped in a directory looks for what sits beside it, and
    // Flea's working directory is wherever Flea was started from, which is nothing to do with it.
    let folder = target.parent().unwrap_or(Path::new("/")).to_path_buf();
    // corner: spawn and not exec, because the program outlives us; see AGENTS.md "Opening a file".
    let started = Command::new(&target)
        .current_dir(&folder)
        // The program outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the program.
        .process_group(0)
        .spawn();
    match started {
        Ok(_) => 0,
        // A file with the bit set that the kernel still refuses: a script with no shebang, or a
        // binary for another architecture. The status is the same one a missing file gets, because
        // the window says the same sentence for both and neither is the operator's mistake to fix.
        Err(_) => {
            eprintln!("flea: that program could not be started");
            FAILED
        }
    }
}
