//! The Open in Agent spawn (Omarchy's agent argv, detached, in the folder): the same detached shape as the TUI's `tui::launch::agent` (null stdio, its own process group, the folder as working directory, no shell); the core crate owns the argv, this module owns the process handle the core cannot hold.
use flea::ui_logic_mn2::agent;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

// Open in Agent in `folder`: the one argv the menu row, the A key and the TUI share.
pub fn launch_agent(folder: &Path) -> io::Result<Child> {
    launch_argv(&agent::argv(), folder)
}

// The detached spawn over one argv: no shell, and the folder as the working directory.
pub fn launch_argv(argv: &[String], folder: &Path) -> io::Result<Child> {
    let Some((program, args)) = argv.split_first() else {
        return Err(io::Error::other("no program to launch"));
    };
    Command::new(program)
        .args(args)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}
