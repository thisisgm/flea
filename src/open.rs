use crate::gui;
use crate::tearoff;
use crate::thp;
use crate::vulkan;
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

// gio open is the OEM route: it asks the desktop database, so Terminal=true is honoured; see AGENTS.md "Opening a file".
pub fn open(path: &str) -> i32 {
    open_all(&[path.to_string()])
}

pub fn open_all(paths: &[String]) -> i32 {
    if paths.is_empty() {
        return FAILED;
    }
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        match resolved(path) {
            Some(p) => targets.push(p),
            None => {
                // The reason is elided, never shown raw, and the path is the user's own input.
                eprintln!("flea: that file could not be opened, check that it still exists");
                return FAILED;
            }
        }
    }
    if targets.len() == 1 && targets[0].is_dir() {
        return IS_DIRECTORY;
    }
    if targets.iter().any(|t| t.is_dir()) {
        eprintln!("flea: directories cannot be opened alongside files");
        return FAILED;
    }
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // corner: waited for, not detached, and on an archive that wait is a cold handler start; see AGENTS.md "Opening a file".
    let mut launcher = Command::new("gio");
    // The display-GPU pin is Qt's alone, and only this launcher's own pin is dropped.
    vulkan::drop_display_pin(&mut launcher);
    // The platform theme Flea traded for its own startup is Qt's alone too, and is handed back here.
    gui::restore_platform_theme(&mut launcher);
    // The tear-off hand-off belongs to the one window a tear-off starts, never to a program it opens.
    tearoff::drop_env(&mut launcher);
    launcher.arg("open");
    for target in &targets {
        launcher.arg(target);
    }
    let finished = launcher
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_all_rejects_empty_paths() {
        assert_eq!(open_all(&[]), FAILED);
    }

    #[test]
    fn open_all_rejects_nonexistent_file() {
        assert_eq!(open_all(&["/nonexistent_file_path_xyz123".to_string()]), FAILED);
    }

    #[test]
    fn open_all_single_directory_returns_is_directory() {
        assert_eq!(open_all(&["/".to_string()]), IS_DIRECTORY);
    }

    #[test]
    fn open_all_multiple_with_directory_fails() {
        assert_eq!(open_all(&["/".to_string(), "/etc".to_string()]), FAILED);
    }
}
