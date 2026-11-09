use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub fn terminal(path: &Path, window: bool) -> io::Result<Child> {
    let path = path.canonicalize()?;
    if !path.is_dir() { return Err(super::words::error("tui.launch.that_is_not_a_folder_so_no_terminal_was_opened")); }
    let mut directory = std::ffi::OsString::from("--dir=");
    directory.push(&path);
    let mut command = Command::new("xdg-terminal-exec");
    command.arg(directory);
    if window { command.arg("--").arg(std::env::current_exe()?).arg("--tui").arg(&path); }
    detached(command)
}

pub fn agent(path: &Path) -> io::Result<Child> {
    // One argv for the menu row, the A key and the TUI: ui_logic_custom_actions owns it.
    let argv = flea::ui_logic_custom_actions::agent::argv();
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]).current_dir(path);
    detached(command)
}

fn detached(mut command: Command) -> io::Result<Child> {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn()
        .map_err(|error| missing(error, &command))
}

// A program that is not there is named in a sentence, never left as the bare not-found cause.
pub(super) fn missing(error: io::Error, command: &Command) -> io::Error {
    if error.kind() != io::ErrorKind::NotFound { return error; }
    let program = command.get_program().to_string_lossy().into_owned();
    io::Error::other(super::words::args("tui.launch.program_not_found", vec![program.into()]))
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
