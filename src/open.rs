use crate::gui;
use crate::runfile;
use crate::tearoff;
use crate::thp;
use crate::vulkan;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "open_selection.rs"]
mod selection;
pub use selection::open_selected;

// The exit statuses ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;
pub const IS_DIRECTORY: i32 = 3;

// Canonical, so a file named --output=/etc/x cannot be read as a flag by the child.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// The environment every program Flea starts carries: huge pages back on, Qt's own pin, driver exclusion and theme dropped, the tear-off hand-off kept to its window. runfile.rs reuses this.
pub fn prepare(child: &mut Command) {
    flea::qscrash::restore(child);
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // The display-GPU pin and the driver exclusion are Qt's alone, and only this launcher's own are dropped.
    vulkan::drop_display_pin(child);
    crate::icdexclude::drop_exclusion(child);
    // The platform theme Flea traded for its own startup is Qt's alone too, and is handed back here.
    gui::restore_platform_theme(child);
    // The tear-off hand-off belongs to the one window a tear-off starts, never to a program it opens.
    tearoff::drop_env(child);
}

// In-process callers (the TUI, the shelf) read any nonzero status as a refusal, 7 and 10 included; only the main.rs --open path asks the card, through open_ask.
pub fn open(path: &str) -> i32 {
    open_inner(path, false, &[])
}

// The window's own entrance: a program asks once on the run card instead of reaching a handler.
pub fn open_ask(path: &str) -> i32 {
    open_inner(path, true, &[])
}

// The ordered image handoff shares every first-target run decision with a single open.
pub fn open_many(paths: &[String]) -> i32 {
    match paths.first() {
        Some(path) if paths.len() == 1 => open_ask(path),
        Some(path) => open_inner(path, true, paths),
        None => FAILED,
    }
}

// gio open is the OEM route: it asks the desktop database, so Terminal=true is honoured; see AGENTS.md "Opening a file".
fn open_inner(path: &str, ask: bool, images: &[String]) -> i32 {
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
    // A FIFO, socket or device is never a program and never blocks here: it skips the snapshot and the run decision and opens the way it always did.
    if !target.is_file() {
        return open_normally(&target, ask, &[]);
    }
    // One open for the kind and the identity together, so a swap between them cannot pair two files.
    let opened = match crate::runid::snapshot(&target) {
        Some(o) => o,
        None => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            return FAILED;
        }
    };
    // A program asks once on the run card instead of reaching a handler, and a file no application claims asks Open with.
    let body = std::fs::read("/proc/self/mountinfo").unwrap_or_default();
    let mounts = String::from_utf8_lossy(&body).into_owned();
    open_snapshot(&target, &opened, ask, &mounts, images)
}

// The run decision and its answer off one snapshot; the mount table is the caller's, so a test can name a noexec volume the box does not hold.
fn open_snapshot(target: &Path, opened: &crate::runid::Opened, ask: bool, mounts: &str, images: &[String]) -> i32 {
    match runfile::decide_opened(opened, target, mounts) {
        runfile::Decision::Ask(kind) => {
            if !ask {
                eprintln!("flea: that file is a program, so it was not opened");
                return FAILED;
            }
            let handler = if matches!(kind, runfile::Kind::AppImage) {
                crate::desktopid::handler(target)
            } else {
                None
            };
            println!(
                "{}",
                runfile::ask_line(kind, handler.as_ref(), &opened.id, target)
            );
            return runfile::ASK_RUN;
        }
        // A script missing its bit opens with its handler the way it always did; the menu's Make executable is the one route to the bit.
        runfile::Decision::MakeExecutable(runfile::Kind::Script) => {}
        // A binary missing its bit is refused for every caller, since an AppImage launcher would run it; its own status names that cause, not a missing application.
        runfile::Decision::MakeExecutable(_) => {
            eprintln!("flea: that file is a program, so it was not opened");
            return runfile::NOT_EXECUTABLE;
        }
        // A program on a noexec volume answers the run card's own status, so the window names the volume and not a missing application.
        runfile::Decision::RefuseNoExec => {
            eprintln!("flea: that program is on a volume that does not allow running files");
            return runfile::RUN_NOEXEC;
        }
        runfile::Decision::OpenNormally => {}
    }
    open_normally(target, ask, images)
}

// Past every run decision: handed to GIO and waited on, the way the opener always did.
fn open_normally(target: &Path, ask: bool, images: &[String]) -> i32 {
    // corner: waited for, not detached, and on an archive that wait is a cold handler start; see AGENTS.md "Opening a file".
    let mut launcher = crate::imageopen::launcher(target, images)
        .or_else(|| empty_by_name(target))
        .unwrap_or_else(|| {
            let mut single = Command::new("gio");
            single.arg("open").arg(target);
            single
        });
    prepare(&mut launcher);
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
            // A file no application claims opens the approved Open with dialog instead, on the window's path alone.
            if ask && crate::desktopid::unclaimed(target) {
                println!(r#"{{"ask":"openWith"}}"#);
                return runfile::ASK_OPEN_WITH;
            }
            eprintln!("flea: no application on this system opened that file");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file");
            FAILED
        }
    }
}

// GIO types every empty file application/x-zerosize, which nothing claims, so an empty notes.md opens with its name's type's default.
fn empty_by_name(target: &Path) -> Option<Command> {
    if !std::fs::metadata(target).is_ok_and(|meta| meta.is_file() && meta.len() == 0) {
        return None;
    }
    let name = target.file_name()?.to_str()?;
    let mime = crate::backend::mime::Db::load().lookup(name)?.to_string();
    let app = crate::desktopid::resolve(&crate::desktopid::default_handler_gio(&mime))?;
    let mut launcher = Command::new("gio");
    launcher.arg("launch").arg(app).arg(target);
    Some(launcher)
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
