// The run card's own classifier and launchers: a regular file with an execute bit or a script with a shebang asks once, anything else opens the way it always did.
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

#[path = "runharden.rs"]
mod runharden;

// The two answers flea --open gives beyond its 0/2/3 contract, read by ui/Opener.qml.
pub const ASK_RUN: i32 = 4;
pub const ASK_OPEN_WITH: i32 = 5;
// The run card's own failures, read by ui/RunCard.qml off the exit alone, never off parsed prose.
pub const RUN_CHANGED: i32 = 6;
pub const RUN_NOEXEC: i32 = 7;
pub const RUN_SPAWN: i32 = 8;
// The application the card named was uninstalled between the ask and the press.
pub const RUN_HANDLER_GONE: i32 = 9;
// flea --open refused a program that lacks its execute bit, so no application was asked.
pub const NOT_EXECUTABLE: i32 = 10;

use crate::open::FAILED;

// Magic and shebang sit at the head, so the classifier reads bytes and never the name.
const ELF_MAGIC: &[u8] = b"\x7fELF";
const SHEBANG: &[u8] = b"#!";
// An AppImage carries ELF magic with "AI" at offset 8 followed by 0x01 or 0x02.
const APPIMAGE_TAG: &[u8] = b"AI";
// The tag and version byte sit past the ELF magic at these offsets.
const APPIMAGE_TAG_OFF: usize = 8;
const APPIMAGE_VER_OFF: usize = 10;
const APPIMAGE_V1: u8 = 0x01;
const APPIMAGE_V2: u8 = 0x02;

// Sample head: b"#!/bin/sh\n" answers Script, b"\x7fELF..." answers Elf.
pub fn classify_header(head: &[u8]) -> Option<Kind> {
    if head.starts_with(SHEBANG) {
        return Some(Kind::Script);
    }
    if head.starts_with(ELF_MAGIC) {
        if is_appimage_bytes(head) {
            return Some(Kind::AppImage);
        }
        return Some(Kind::Elf);
    }
    None
}

// True for ELF magic plus the AppImage tag at offset 8, whatever the file is named.
fn is_appimage_bytes(head: &[u8]) -> bool {
    head.len() > APPIMAGE_VER_OFF
        && head[APPIMAGE_TAG_OFF] == APPIMAGE_TAG[0]
        && head[APPIMAGE_TAG_OFF + 1] == APPIMAGE_TAG[1]
        && (head[APPIMAGE_VER_OFF] == APPIMAGE_V1 || head[APPIMAGE_VER_OFF] == APPIMAGE_V2)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Kind {
    Script,
    Elf,
    AppImage,
}

// The wire spelling ui/js/RunFile.js reads off the ask line.
pub fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Script => "script",
        Kind::Elf => "elf",
        Kind::AppImage => "appimage",
    }
}

#[derive(Debug, PartialEq)]
pub enum Decision {
    Ask(Kind),
    // Program-shaped but missing its execute bit: never a run; src/open.rs opens a script with its handler and refuses a binary.
    MakeExecutable(Kind),
    OpenNormally,
    RefuseNoExec,
}

// The file type bits out of st_mode, so a symlink, fifo or directory never reads as a program.
const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

// A header the classifier cannot name opens the way it always did, whatever its mode.
pub fn decide(path: &Path) -> Decision {
    let body = std::fs::read("/proc/self/mountinfo").unwrap_or_default();
    let text = String::from_utf8_lossy(&body).into_owned();
    decide_with(path, &text)
}

// Sample body: "1 0 8:1 / /tmp rw,noexec - tmpfs tmpfs rw" refuses; "1 0 8:1 / / rw - ext4 /dev/a rw" allows.
pub fn decide_with(path: &Path, mounts: &str) -> Decision {
    match crate::runid::snapshot(path) {
        Some(opened) => decide_opened(&opened, path, mounts),
        None => Decision::OpenNormally,
    }
}

// The decision off one open file description, so the kind, the mode and the owner come from one fstat.
pub fn decide_opened(opened: &crate::runid::Opened, path: &Path, mounts: &str) -> Decision {
    if opened.mode & S_IFMT != S_IFREG {
        return Decision::OpenNormally;
    }
    let kind = match classify_header(&opened.head) {
        Some(k) => k,
        None => return Decision::OpenNormally,
    };
    let runnable = crate::runid::executable_by_caller(opened);
    // A script missing its bit is never run (open.rs opens MakeExecutable(Script) with its handler), so noexec has nothing to refuse.
    if !runnable && kind == Kind::Script {
        return Decision::MakeExecutable(kind);
    }
    if noexec_in(path, mounts) {
        return Decision::RefuseNoExec;
    }
    if runnable {
        Decision::Ask(kind)
    } else {
        Decision::MakeExecutable(kind)
    }
}

// Sample options: "rw,noexec" refuses, "rw" allows; an unreadable mount table allows.
fn noexec_in(path: &Path, mounts: &str) -> bool {
    match crate::backend::mountinfo::raw_mount_options_in(path, mounts) {
        Some(options) => options.split(',').any(|o| o == "noexec"),
        None => false,
    }
}

// One absolute argument, so a name with spaces, quotes, a leading dash or a newline is never read as flags by the child it names.
pub fn run_argv(target: &Path) -> Vec<OsString> {
    vec![target.as_os_str().to_owned()]
}

// Sample dir: "/home/gm/Downloads" names xdg-terminal-exec --dir=/home/gm/Downloads -e <target>, the OEM route src/terminal.rs takes and the -e spelling omarchy-launch-tui runs.
pub fn terminal_argv(dir: &Path, target: &Path) -> Vec<OsString> {
    let mut flag = OsString::from("--dir=");
    flag.push(dir);
    vec![
        OsString::from("xdg-terminal-exec"),
        flag,
        OsString::from("-e"),
        target.as_os_str().to_owned(),
    ]
}

fn canonical(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(flea::bytename::text_path(path)).ok()
}

// The card's four modes beside main.rs's --open arms: Some(code) when argv names one, malformed shapes refused through the same usage errors, None for every other mode.
pub fn dispatch(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) == Some("--open-selection") {
        return Some(crate::open::open_selected(&args[2..]));
    }
    if args.get(1).map(String::as_str) == Some("--run") {
        if args.len() == 8 {
            if let Some(id) = crate::runid::FileId::parse(&args[3], &args[4], &args[5], &args[6], &args[7]) {
                return Some(run(&args[2], &id));
            }
        }
        crate::usage("--run takes one path and its card identity");
    }
    if args.get(1).map(String::as_str) == Some("--run-terminal") {
        if args.len() == 8 {
            if let Some(id) = crate::runid::FileId::parse(&args[3], &args[4], &args[5], &args[6], &args[7]) {
                return Some(run_in_terminal(&args[2], &id));
            }
        }
        crate::usage("--run-terminal takes one path and its card identity");
    }
    if args.get(1).map(String::as_str) == Some("--open-text") {
        if args.len() == 3 {
            return Some(open_text(&args[2]));
        }
        crate::usage("--open-text takes one path");
    }
    if args.get(1).map(String::as_str) == Some("--open-handler") {
        if args.len() == 9 {
            if let Some(id) = crate::runid::FileId::parse(&args[4], &args[5], &args[6], &args[7], &args[8]) {
                return Some(open_handler(&args[2], &args[3], &id));
            }
        }
        crate::usage("--open-handler takes one path, a desktop id and the card identity");
    }
    None
}

fn parent_of(target: &Path) -> PathBuf {
    target.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"))
}

// Some(exit) when the target must not start at all, None when the card may ask for it.
fn refused(target: &Path) -> Option<i32> {
    match decide(target) {
        Decision::Ask(_) => None,
        Decision::MakeExecutable(_) => {
            eprintln!("flea: that file is not executable, so it was not run");
            Some(FAILED)
        }
        Decision::RefuseNoExec => {
            eprintln!("flea: that program is on a volume that does not allow running files");
            Some(RUN_NOEXEC)
        }
        Decision::OpenNormally => {
            eprintln!("flea: that file is not a program, so it was not run");
            Some(FAILED)
        }
    }
}

// The R3 recheck: the file behind the path is still the one the card named, and a stat the swap itself broke refuses like a mismatch.
fn rechecked(target: &Path, expect: &crate::runid::FileId) -> Option<i32> {
    match crate::runid::capture(target) {
        Some(now) if now.matches(expect) => None,
        _ => {
            eprintln!("flea: that file changed since the card asked, so it was not run");
            Some(RUN_CHANGED)
        }
    }
}

// Run starts it detached in its own folder, the way the terminal launcher does: nothing runs without the card, so this mode answers only the card's own Run button.
pub fn run(path: &str, expect: &crate::runid::FileId) -> i32 {
    let target = match canonical(path) {
        Some(p) => p,
        None => {
            eprintln!("flea: that program could not be run, check that it still exists");
            return FAILED;
        }
    };
    if let Some(exit) = refused(&target) {
        return exit;
    }
    if let Some(exit) = rechecked(&target, expect) {
        return exit;
    }
    let argv = run_argv(&target);
    let mut child = std::process::Command::new(&argv[0]);
    child.args(&argv[1..]);
    child.current_dir(parent_of(&target));
    crate::terminal::detach(&mut child);
    runharden::harden(&mut child);
    match child.spawn() {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to run that file");
            RUN_SPAWN
        }
    }
}

// Run in terminal shows the output: the same checks as Run, under the OEM terminal.
pub fn run_in_terminal(path: &str, expect: &crate::runid::FileId) -> i32 {
    let target = match canonical(path) {
        Some(p) => p,
        None => {
            eprintln!("flea: that program could not be run, check that it still exists");
            return FAILED;
        }
    };
    if let Some(exit) = refused(&target) {
        return exit;
    }
    if let Some(exit) = rechecked(&target, expect) {
        return exit;
    }
    let argv = terminal_argv(&parent_of(&target), &target);
    let mut child = std::process::Command::new(&argv[0]);
    child.args(&argv[1..]);
    crate::terminal::detach(&mut child);
    runharden::harden(&mut child);
    match child.spawn() {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to run that file in a terminal");
            RUN_SPAWN
        }
    }
}

// The ask line flea --open prints on its run answer: the kind, the desktop handler's id and own Name (empty for none), the R3 identity and the resolved target.
pub fn ask_line(kind: Kind, handler: Option<&(String, String)>, id: &crate::runid::FileId, target: &Path) -> String {
    let (handler_id, handler_name) = handler.map(|(i, n)| (i.as_str(), n.as_str())).unwrap_or_default();
    format!(
        r#"{{"ask":"run","kind":"{}","size":{},"handlerId":"{}","handlerName":"{}","dev":"{}","ino":"{}","mtimeS":{},"mtimeNs":{},"target":"{}"}}"#,
        kind_name(kind),
        id.size,
        crate::json::escape(handler_id),
        crate::json::escape(handler_name),
        id.dev,
        id.ino,
        id.mtime_s,
        id.mtime_ns,
        crate::json::escape(&flea::bytename::path_text(target))
    )
}

// Open as text sends it to that editor through gio launch, waited on the way gio open is.
pub fn open_text(path: &str) -> i32 {
    let target = match canonical(path) {
        Some(p) => p,
        None => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if target.is_dir() {
        return crate::open::IS_DIRECTORY;
    }
    let id = crate::desktopid::text_editor();
    if id.is_empty() {
        eprintln!("flea: no text editor is installed for that file");
        return FAILED;
    }
    // gio launch needs the file, never the bare id the query names.
    let desktop = match crate::desktopid::resolve(&id) {
        Some(found) => found,
        None => {
            eprintln!("flea: no text editor on this system opened that file");
            return FAILED;
        }
    };
    match launch(&desktop, &target) {
        Ok(status) if status.success() => 0,
        Ok(_) => {
            eprintln!("flea: no text editor on this system opened that file");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file as text");
            RUN_SPAWN
        }
    }
}

// The handler button hands the file to the entry the card named, by its own launchable file, after the R3 recheck; the file is an argument, never a program run here.
pub fn open_handler(path: &str, desktop_id: &str, expect: &crate::runid::FileId) -> i32 {
    let target = match canonical(path) {
        Some(p) => p,
        None => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if let Some(exit) = rechecked(&target, expect) {
        return exit;
    }
    let desktop = match crate::desktopid::named_entry(desktop_id) {
        Some((found, _)) => found,
        None => {
            eprintln!("flea: that application is no longer installed");
            return RUN_HANDLER_GONE;
        }
    };
    match launch(&desktop, &target) {
        Ok(status) if status.success() => 0,
        Ok(_) => {
            eprintln!("flea: that application did not open the file");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file");
            RUN_SPAWN
        }
    }
}

// gio launch with the entry and the file as two argv elements, waited on the way gio open is, in its own process group.
fn launcher(desktop: &Path, target: &Path) -> std::process::Command {
    let mut launcher = std::process::Command::new("gio");
    crate::open::prepare(&mut launcher);
    // A program the card starts keeps neither Flea's private variables nor an inherited descriptor.
    runharden::harden(&mut launcher);
    launcher
        .arg("launch")
        .arg(desktop)
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    launcher
}

fn launch(desktop: &Path, target: &Path) -> std::io::Result<std::process::ExitStatus> {
    launcher(desktop, target).status()
}

#[cfg(test)]
#[path = "runfile_tests.rs"]
mod tests;
