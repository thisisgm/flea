use crate::thp;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
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

// A +x ELF or AppImage has no desktop handler, so gio open refuses it; other file managers run it.
fn is_runnable(path: &Path) -> bool {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !meta.is_file() {
        return false;
    }
    if meta.permissions().mode() & 0o111 == 0 {
        return false;
    }
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"))
    {
        return true;
    }
    let mut hdr = [0u8; 4];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut hdr)) {
        Ok(()) => hdr == *b"\x7fELF",
        Err(_) => false,
    }
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
    if is_runnable(&target) {
        // The file is the application: waiting would hold flea --open for its whole life; see AGENTS.md "Opening a file".
        let mut command = Command::new(&target);
        if let Some(dir) = target.parent() {
            command.current_dir(dir);
        }
        let started = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
        return match started {
            Ok(_) => 0,
            Err(_) => {
                eprintln!("flea: nothing on this system could be asked to open that file");
                FAILED
            }
        };
    }
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
