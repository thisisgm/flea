use crate::gui;
use crate::runfile;
use crate::tearoff;
use crate::thp;
use crate::vulkan;
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

// The environment every program Flea starts carries: huge pages back on, Qt's own pin, driver exclusion and theme dropped, the tear-off hand-off kept to its window. runfile.rs reuses this.
pub fn prepare(child: &mut Command) {
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

// In-process callers (the TUI, the shelf) stay on 0/2/3: an executable is refused with its sentence, and only the main.rs --open path asks the card, through open_all.
pub fn open(path: &str) -> i32 {
    open_inner(&[path.to_string()], false)
}

// The one-path spelling of the window's entrance the open tests drive; the binary itself always
// reaches open_all, whose variadic tail takes one path or more.
#[cfg(test)]
pub fn open_ask(path: &str) -> i32 {
    open_all(&[path.to_string()])
}

// The window's entrance for one path or more, which asks the run card; in-process callers use open::open.
pub fn open_all(paths: &[String]) -> i32 {
    open_inner(paths, true)
}

// gio open is the OEM route: it asks the desktop database, so Terminal=true is honoured; see AGENTS.md "Opening a file".
fn open_inner(paths: &[String], ask: bool) -> i32 {
    if paths.is_empty() {
        return FAILED;
    }
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        match resolved(path) {
            Some(p) => targets.push(p),
            // The reason is elided, never shown raw, and the path is the user's own input.
            None => {
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
    // Every target takes the per-file run decision, so a batch is refused whole for the same
    // reasons one path is. A FIFO, socket or device is never a program and never blocks here: it
    // skips the snapshot and the run decision and opens the way it always did.
    for target in &targets {
        if !target.is_file() {
            continue;
        }
        // One open for the kind and the identity together, so a swap between them cannot pair two files.
        let opened = match crate::runid::snapshot(target) {
            Some(o) => o,
            None => {
                eprintln!("flea: that file could not be opened, check that it still exists");
                return FAILED;
            }
        };
        // A program asks once on the run card instead of reaching a handler, and a file no application claims asks Open with.
        let body = std::fs::read("/proc/self/mountinfo").unwrap_or_default();
        let mounts = String::from_utf8_lossy(&body).into_owned();
        match runfile::decide_opened(&opened, target, &mounts) {
            runfile::Decision::Ask(kind) => {
                // The card is single-flight and ask_line carries one identity, so only a lone path asks it.
                if ask && paths.len() == 1 {
                    let handler = if matches!(kind, runfile::Kind::AppImage) { crate::desktopid::handler(target) } else { None };
                    println!("{}", runfile::ask_line(kind, handler.as_ref(), &opened.id, target));
                    return runfile::ASK_RUN;
                }
                // A batch names the batch, the way the directory refusal above does.
                if paths.len() == 1 {
                    eprintln!("flea: that file is a program, so it was not opened");
                } else {
                    eprintln!("flea: a program opens on its own, one at a time, so nothing in that selection was opened");
                }
                return FAILED;
            }
            // A script missing its bit opens with its handler the way it always did; the menu's Make executable is the one route to the bit.
            runfile::Decision::MakeExecutable(runfile::Kind::Script) => {}
            // A binary missing its bit is refused for every caller, since an AppImage launcher would run it.
            runfile::Decision::MakeExecutable(_) => {
                eprintln!("flea: that file is a program, so it was not opened");
                return FAILED;
            }
            runfile::Decision::RefuseNoExec => {
                eprintln!("flea: that program is on a volume that does not allow running files");
                return FAILED;
            }
            runfile::Decision::OpenNormally => {}
        }
    }
    // More than one target is launched grouped by handler; a type with no handler leaves the whole
    // batch on the gio open route rather than opening part of a selection.
    if targets.len() > 1 {
        if let Some(plan) = batch_plan(&targets) {
            return launch_groups(&plan);
        }
    }
    open_normally(&targets, ask)
}

// Past every run decision: handed to gio open and waited on, the way the opener always did.
fn open_normally(targets: &[PathBuf], ask: bool) -> i32 {
    // corner: waited for, not detached, and on an archive that wait is a cold handler start; see AGENTS.md "Opening a file".
    let mut launcher = Command::new("gio");
    prepare(&mut launcher);
    let finished = launcher
        .arg("open")
        .args(targets)
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
            if ask && targets.len() == 1 && crate::desktopid::unclaimed(&targets[0]) {
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

// Each group is one gio launch of the desktop file the database named and the files that share it,
// and a refusal anywhere refuses the whole selection's answer.
fn launch_groups(groups: &[(PathBuf, Vec<PathBuf>)]) -> i32 {
    let mut refused = false;
    for (desktop, files) in groups {
        let mut args: Vec<&Path> = Vec::with_capacity(files.len() + 1);
        args.push(desktop);
        for file in files {
            args.push(file);
        }
        if run_gio("launch", &args) != 0 {
            refused = true;
        }
    }
    if refused {
        eprintln!("flea: gio launch refused part of that selection, so some of it did not open");
        return FAILED;
    }
    0
}

// gio open starts the handler once per file, so a selection of N opened N programs where
// Nautilus opened one. It groups the selection by default application in make_activation_parameters
// (src/nautilus-files-view.c) and launches each group once, so a player whose Exec line takes a
// URI list gets the whole selection as one playlist. The desktop database still decides the
// handler; Flea only asks which one through xdg-mime and hands gio the desktop file it named.
// Nothing is parsed out of the file. A single file keeps the gio open route, which is the one
// that honours Terminal=true; see AGENTS.md "Opening a file".
fn batch_plan(targets: &[PathBuf]) -> Option<Vec<(PathBuf, Vec<PathBuf>)>> {
    let db = crate::backend::mime::Db::load();
    // Grouped by the handler, not by the type: two types can share one application, and
    // Nautilus hashes on g_app_info_get_id for that reason. Grouping by type would start that
    // application twice where Nautilus starts it once.
    let mut groups: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    // One lookup per distinct type, not one per file: a hundred-row selection of a single
    // type is one subprocess here, where asking per file spent a hundred.
    let mut asked: Vec<(String, PathBuf)> = Vec::new();
    for target in targets {
        let mime = db.lookup(&target.to_string_lossy())?.to_string();
        // A type with no handler, or one whose desktop file is not installed, leaves the whole
        // batch on the gio open route rather than opening part of a selection.
        let desktop = match asked.iter().find(|(name, _)| *name == mime) {
            Some((_, path)) => path.clone(),
            None => {
                let path = desktop_file(&mime)?;
                asked.push((mime, path.clone()));
                path
            }
        };
        match groups.iter_mut().find(|(name, _)| *name == desktop) {
            Some((_, files)) => files.push(target.clone()),
            None => groups.push((desktop, vec![target.clone()])),
        }
    }
    Some(groups)
}

// The desktop file the database names for a type, or None when it names none this box holds.
// Asked through gio itself so the handoff stays the one program open.rs is allowed to name.
fn desktop_file(mime: &str) -> Option<PathBuf> {
    let asked = Command::new("gio").arg("mime").arg(mime).output().ok()?;
    // "Default application for "audio/mpeg": mpv.desktop" — the name is what follows the last colon.
    let line = String::from_utf8(asked.stdout).ok()?;
    let app = line.lines().next()?.rsplit_once(':')?.1.trim().to_string();
    if app.is_empty() {
        return None;
    }
    // Each variable carries its own default, per the XDG spec, so a box that exports neither still
    // finds a handler the user installed in ~/.local/share/applications.
    let mut dirs: Vec<PathBuf> = Vec::new();
    match std::env::var("XDG_DATA_HOME") {
        Ok(home) if !home.is_empty() => dirs.push(PathBuf::from(home)),
        _ => {
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(PathBuf::from(home).join(".local/share"));
            }
        }
    }
    let system = match std::env::var("XDG_DATA_DIRS") {
        Ok(v) if !v.is_empty() => v,
        _ => "/usr/local/share:/usr/share".to_string(),
    };
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.iter().map(|d| d.join("applications").join(&app)).find(|p| p.is_file())
}

// Every route is one gio invocation, so the guards Flea hands back and the exit status live here.
fn run_gio(verb: &str, args: &[&Path]) -> i32 {
    let mut launcher = Command::new("gio");
    // The huge page setting, the display-GPU pin, the driver exclusion, the theme trade and the
    // tear-off hand-off are the one set of guards every program Flea starts carries, in prepare().
    prepare(&mut launcher);
    launcher.arg(verb);
    launcher.args(args);
    // corner: waited for, not detached, and on an archive that wait is a cold handler start; see AGENTS.md "Opening a file".
    match launcher
        // The handler outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the opened program.
        .process_group(0)
        .status()
    {
        Ok(status) if status.success() => 0,
        // A launcher that refused, which a spawn nobody waited on used to report as a clean handoff.
        _ => {
            eprintln!("flea: gio {} refused that file, so no application on this system took it", verb);
            FAILED
        }
    }
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
