//! The Open in Agent spawn (Omarchy's agent argv, detached, in the folder): the same detached shape as the TUI's `tui::launch::agent` (null stdio, its own process group, the folder as working directory, no shell); the core crate owns the argv, this module owns the process handle the core cannot hold.
use flea::ui_logic_mn2::agent;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

// Open in Agent in `folder` with the argv the menu row, the A key and the TUI share, reaped off the UI thread.
pub fn launch_agent_reaped(folder: &Path) -> io::Result<()> {
    launch_reaped(&agent::argv(), folder)
}

// The reaper thread starts first, so a thread that cannot start is an error to say, never a zombie child.
pub fn launch_reaped(argv: &[String], folder: &Path) -> io::Result<()> {
    let (send, receive) = mpsc::channel::<Child>();
    std::thread::Builder::new()
        .name("flea-agent".into())
        .spawn(move || {
            if let Ok(mut child) = receive.recv() {
                let _ = child.wait();
            }
        })?;
    send.send(launch_argv(argv, folder)?)
        .map_err(|_| io::Error::other("the agent reaper stopped"))
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
