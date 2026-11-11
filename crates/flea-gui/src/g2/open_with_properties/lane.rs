//! The opener lane: every launch a card asks for runs off the UI thread, and bulk rename's editor works over a private scratch file.
use flea::ui_logic_open_with_properties::bulkrename::{self as bulk, is_scratch_dir};
use gpui::{App, Task};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::Path;
use std::process::{Command, Stdio};

/// A finished launch: its status, None when it never started, and what it printed.
pub type Exit = (Option<i32>, String);

/// Blocking; call it only from the background executor.
pub fn run(argv: &[String]) -> Exit {
    let Some((program, args)) = argv.split_first() else {
        return (None, String::new());
    };
    // A path argument's exact text goes out as its own bytes, see docs/architecture/exact-names.md.
    match Command::new(program)
        .args(flea::bytename::args(args))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(out) => (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        ),
        Err(_) => (None, String::new()),
    }
}

/// The launch on the background executor; the card reads the exit when the task resolves.
pub fn spawn(cx: &App, argv: Vec<String>) -> Task<Exit> {
    cx.background_executor().spawn(async move { run(&argv) })
}

/// A test's stand-in editor, scratch base and terminal: the process environment is every parallel test's, so a test never sets TMPDIR, PATH or EDITOR.
#[cfg(test)]
#[derive(Clone, Debug)]
pub struct Stand {
    pub editor: String,
    pub base: String,
    pub terminal: String,
}

#[cfg(test)]
pub static STAND: std::sync::Mutex<Option<Stand>> = std::sync::Mutex::new(None);

#[cfg(test)]
fn stand() -> Option<Stand> {
    STAND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// The terminal a test stands in for xdg-terminal-exec, which leads every editor launch.
#[cfg(test)]
fn stood_in(mut argv: Vec<String>) -> Vec<String> {
    if let Some(stand) = stand() {
        argv[0] = stand.terminal;
    }
    argv
}

/// The editor the session names, VISUAL before EDITOR, as v0.3.12 read them.
pub fn session_editor() -> Option<String> {
    #[cfg(test)]
    if let Some(stand) = stand() {
        return Some(stand.editor);
    }
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.trim().is_empty())
}

/// The scratch base, TMPDIR or /tmp.
pub fn scratch_base() -> String {
    #[cfg(test)]
    if let Some(stand) = stand() {
        return stand.base;
    }
    std::env::var("TMPDIR")
        .ok()
        .filter(|dir| dir.starts_with('/') && dir.len() > 1)
        .map_or_else(
            || "/tmp".into(),
            |dir| dir.trim_end_matches('/').to_string(),
        )
}

/// Bulk rename's Edit in Terminal: names into a 0600 file in a 0700 flea-rename dir, the editor over it, the text read back, the dir removed.
pub fn edit_names(
    base: &str,
    folder: &str,
    editor: Option<&str>,
    names: &[String],
) -> Result<String, &'static str> {
    let editor = editor.ok_or("dialogs.file.bulk.err_editor")?.to_string();
    edit_names_with(base, names, |file| {
        let argv = bulk::editor_argv(folder, &editor, file);
        #[cfg(test)]
        let argv = stood_in(argv);
        run(&argv).0 == Some(0)
    })
}

/// The same lane with the launch handed in, so a test drives it without a terminal.
pub fn edit_names_with(
    base: &str,
    names: &[String],
    launch: impl FnOnce(&str) -> bool,
) -> Result<String, &'static str> {
    let dir = scratch_dir(base).ok_or("dialogs.file.bulk.err_dir")?;
    let file = format!("{dir}/names");
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .and_then(|mut out| out.write_all(bulk::edit_text(names).as_bytes()));
    let outcome = match written {
        Err(_) => Err("dialogs.file.bulk.err_file"),
        Ok(()) if !launch(&file) => Err("dialogs.file.bulk.err_editor"),
        Ok(()) => fs::read(&file)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|_| "dialogs.file.bulk.err_file"),
    };
    // Only the dir this call made is removed, and only when it is still the base plus one flea-rename leaf.
    if dir.starts_with('/') && is_scratch_dir(base, &dir) && Path::new(&dir).is_dir() {
        let _ = fs::remove_dir_all(&dir);
    }
    outcome
}

fn scratch_dir(base: &str) -> Option<String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (0..16).find_map(|attempt| {
        let dir = format!(
            "{base}/{}{}-{stamp:x}-{attempt}",
            bulk::SCRATCH_STEM,
            std::process::id()
        );
        (is_scratch_dir(base, &dir) && DirBuilder::new().mode(0o700).create(&dir).is_ok())
            .then_some(dir)
    })
}
