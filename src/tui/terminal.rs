use std::io::{self, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
const SIGHUP: i32 = 1;
const SIGKILL: i32 = 9;
const SIGCONT: i32 = 18;
const SIGTTOU: i32 = 22;
// The shown faults of the stty calls: raw mode at the start and the terminal handed back at the end, then the same two around the editor.
const PREPARE_FAILED: &str = "The terminal could not be prepared";
const RESTORE_FAILED: &str = "The terminal could not be restored";
const PREPARE_FOR_EDITOR_FAILED: &str = "The terminal could not be prepared for the editor";
const RESTORE_AFTER_EDITOR_FAILED: &str = "The terminal could not be restored after the editor";
#[repr(C)]
struct WindowSize {
    rows: u16,
    columns: u16,
    x: u16,
    y: u16,
}
// The editor's exit as the bulk rename sees it: a nonzero status ends the rename with the status shown.
fn editor_result(status: std::process::ExitStatus) -> io::Result<()> {
    if status.success() { Ok(()) } else { Err(io::Error::other(format!("The editor could not finish ({})", status))) }
}
pub fn launch(path: &Path, window: bool) -> io::Result<Child> {
    let path = path.canonicalize()?;
    if !path.is_dir() { return Err(io::Error::other("That is not a folder, so no terminal was opened")); }
    let mut directory = std::ffi::OsString::from("--dir=");
    directory.push(&path);
    let mut command = Command::new("xdg-terminal-exec");
    command.arg(directory);
    if window { command.arg("--").arg(std::env::current_exe()?).arg("--tui").arg(&path); }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn()
}
extern "C" {
    fn ioctl(fd: i32, request: usize, ...) -> i32;
    fn signal(sig: i32, handler: usize) -> usize;
    fn tcgetpgrp(fd: i32) -> i32;
    fn tcsetpgrp(fd: i32, group: i32) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}
// poll(2) struct pollfd, whose three fields are int, short, short in that order.
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}
const POLLIN: i16 = 1;
extern "C" fn stop(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}

pub struct Terminal {
    saved: String,
    handlers: Vec<(i32, usize)>,
    active: bool,
}
impl Terminal {
    pub fn enter() -> io::Result<Self> {
        let output = crate::backend::locale::command("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()?;
        stty_result(output.status, PREPARE_FAILED)?;
        let mut terminal = Self {
            saved: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            handlers: Vec::new(),
            active: false,
        };
        if terminal.saved.is_empty() {
            return Err(io::Error::other(PREPARE_FAILED));
        }
        raw(PREPARE_FAILED)?;
        for sig in [SIGINT, SIGTERM, SIGHUP] {
            terminal
                .handlers
                .push((sig, unsafe { signal(sig, stop as *const () as usize) }));
        }
        print!("{}", opening(&shm_query(Path::new(SHM_DIR), &shm_name()).unwrap_or_default()));
        io::stdout().flush()?;
        terminal.active = true;
        Ok(terminal)
    }
    pub fn run_editor(&mut self, editor: &str, path: &Path) -> io::Result<()> {
        self.restore(PREPARE_FOR_EDITOR_FAILED)?;
        let result = match EditorJob::start(editor, path) {
            Ok(mut editor) => {
            let result = loop {
                if self.stopped() { break Err(io::Error::other("Bulk rename editor interrupted")); }
                match editor.status() {
                    Ok(Some(status)) => break editor_result(status),
                    Err(error) => break Err(error),
                    Ok(None) => {}
                }
                const INTERRUPT_POLL: std::time::Duration = std::time::Duration::from_millis(50);
                std::thread::sleep(INTERRUPT_POLL);
            };
            editor.close()?;
            result
            }
            Err(error) => Err(error),
        };
        raw(RESTORE_AFTER_EDITOR_FAILED)?;
        screen(true)?;
        self.active = true;
        result
    }
    pub fn ready(&self) -> bool { self.active }
    fn restore(&mut self, fault: &'static str) -> io::Result<()> {
        self.active = false;
        let screen = screen(false);
        let mode = Command::new("stty").arg(&self.saved).status().and_then(|status| stty_result(status, fault));
        mode.and(screen)
    }
    pub fn stopped(&self) -> bool {
        STOP.load(Ordering::Relaxed)
    }
    // Waits up to timeout_ms for a key or a ring on wake; raw mode is min 0 time 0, so the read itself never blocks.
    pub fn read(&self, wake: i32, timeout_ms: i32) -> io::Result<Vec<u8>> {
        if !wait_input(0, wake, timeout_ms)? {
            return Ok(Vec::new());
        }
        let mut bytes = [0; 4096];
        // Straight from fd 0: std's stdin reads ahead into a buffer poll cannot see, which would strand a paste's tail.
        let mut stdin = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(0) });
        let n = match stdin.read(&mut bytes) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => 0,
            Err(e) => return Err(e),
        };
        Ok(bytes[..n].to_vec())
    }
}
// The read's poll on its own, so the hold and wire tests can time it on a socket pair; true once stdin has anything to report.
pub(super) fn wait_input(stdin: i32, wake: i32, timeout_ms: i32) -> io::Result<bool> {
    let mut fds = [PollFd { fd: stdin, events: POLLIN, revents: 0 }, PollFd { fd: wake, events: POLLIN, revents: 0 }];
    if unsafe { poll(fds.as_mut_ptr(), 2, timeout_ms) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted { return Err(error); }
    }
    Ok(fds[0].revents != 0)
}
struct EditorJob {
    child: Child,
    foreground: i32,
    handler: usize,
    finished: bool,
    restored: bool,
}
impl EditorJob {
    fn start(editor: &str, path: &Path) -> io::Result<Self> {
        let foreground = unsafe { tcgetpgrp(0) };
        if foreground < 0 { return Err(io::Error::last_os_error()); }
        const IGNORE_SIGNAL: usize = 1;
        let handler = unsafe { signal(SIGTTOU, IGNORE_SIGNAL) };
        // Sample input: nvim -f; the user's editor command is shell syntax, while the names path is a positional argument.
        let child = Command::new("/bin/sh").args(["-c", &format!("exec {} \"$1\"", editor), "flea-bulk-rename"])
            .arg(path).process_group(0).spawn();
        let child = match child {
            Ok(child) => child,
            Err(error) => { unsafe { signal(SIGTTOU, handler); } return Err(error); }
        };
        let job = Self { child, foreground, handler, finished: false, restored: false };
        if unsafe { tcsetpgrp(0, job.child.id() as i32) } != 0 { return Err(io::Error::last_os_error()); }
        // A terminal reader may stop before the parent gives its new process group the foreground.
        let _ = crate::procsignal::signal_group(job.child.id() as i32, SIGCONT);
        Ok(job)
    }
    fn status(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        const NO_HANG: i32 = 1;
        const UNTRACED: i32 = 2;
        let mut status = 0;
        let result = unsafe { waitpid(self.child.id() as i32, &mut status, NO_HANG | UNTRACED) };
        if result < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted { Ok(None) } else { Err(error) };
        }
        if result == 0 { return Ok(None); }
        let status = std::process::ExitStatus::from_raw(status);
        if status.stopped_signal().is_some() {
            return Err(io::Error::other("Bulk rename editor suspended; rename cancelled"));
        }
        self.finished = true;
        Ok(Some(status))
    }
    fn close(&mut self) -> io::Result<()> {
        self.restored = true;
        let mut cleanup = Ok(());
        if !self.finished {
            const NO_SUCH_PROCESS: i32 = 3;
            match crate::procsignal::signal_group(self.child.id() as i32, SIGKILL) {
                Err(error) if error.raw_os_error() != Some(NO_SUCH_PROCESS) => cleanup = Err(error),
                _ => cleanup = self.child.wait().map(|_| ()),
            }
        }
        let restored = if unsafe { tcsetpgrp(0, self.foreground) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) };
        unsafe { signal(SIGTTOU, self.handler); }
        cleanup.and(restored)
    }
}
impl Drop for EditorJob {
    fn drop(&mut self) {
        if !self.restored { if let Err(error) = self.close() { eprintln!("flea: editor cleanup: {}", error); } }
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Err(error) = self.restore(RESTORE_FAILED) { eprintln!("flea: {}", error); }
        // kitty unlinks the object once it reads it; a terminal that could not read it leaves it here.
        let _ = std::fs::remove_file(Path::new(SHM_DIR).join(shm_name()));
        for &(sig, handler) in &self.handlers {
            unsafe {
                signal(sig, handler);
            }
        }
    }
}
const SHM_DIR: &str = "/dev/shm";
fn shm_name() -> String {
    format!("flea-tui-shm-{}", std::process::id())
}
// kitty answers id 32 with OK only when it can read this machine's shared memory, which mpv's frames then take.
fn shm_query(dir: &Path, name: &str) -> Option<String> {
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).ok()?;
    file.write_all(&[0, 0, 0]).ok()?;
    let encoded = super::graphics::base64(format!("/{}", name).as_bytes());
    Some(format!("\x1b_Gi=32,s=1,v=1,a=q,t=s,f=24;{}\x1b\\", encoded))
}
// A stty call's exit as a shown fault: a nonzero status says what the call was for.
fn stty_result(status: std::process::ExitStatus, fault: &'static str) -> io::Result<()> {
    if status.success() { Ok(()) } else { Err(io::Error::other(fault)) }
}
fn raw(fault: &'static str) -> io::Result<()> {
    stty_result(Command::new("stty").args(["raw", "-echo", "min", "0", "time", "0"]).status()?, fault)
}
const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[>1u\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c";
const LEAVE: &str = "\x1b[<u\x1b[?1003l\x1b[?1002l\x1b[?1006l\x1b[?2004l\x1b[0m\x1b[?25h\x1b[?1049l";
// kitty answers in order, so the shared memory query goes ahead of ENTER's kitty probe and the first preview knows both.
fn opening(query: &str) -> String {
    format!("{}{}", query, ENTER)
}
fn screen(entered: bool) -> io::Result<()> {
    print!("{}", if entered { ENTER } else { LEAVE });
    io::stdout().flush()
}
fn measured() -> Option<WindowSize> {
    #[cfg(target_os = "linux")]
    const GET_WINDOW_SIZE: usize = 0x5413;
    #[cfg(not(target_os = "linux"))]
    const GET_WINDOW_SIZE: usize = 0x40087468;
    let mut size = WindowSize {
        rows: 0,
        columns: 0,
        x: 0,
        y: 0,
    };
    if unsafe { ioctl(1, GET_WINDOW_SIZE, &mut size) } == 0 && size.columns > 0 && size.rows > 0 {
        Some(size)
    } else {
        None
    }
}

pub fn size() -> (usize, usize) {
    measured()
        .map(|size| (size.columns as usize, size.rows as usize))
        .unwrap_or((80, 24))
}

pub fn cell_pixels() -> Option<(usize, usize)> {
    let size = measured()?;
    let width = usize::from(size.x) / usize::from(size.columns);
    let height = usize::from(size.y) / usize::from(size.rows);
    (width > 0 && height > 0).then_some((width, height))
}
#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
