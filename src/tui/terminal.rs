use std::io::{self, Read, Write};
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
#[repr(C)]
struct WindowSize {
    rows: u16,
    columns: u16,
    x: u16,
    y: u16,
}
pub fn launch(path: &Path, window: bool) -> io::Result<Child> {
    let path = path.canonicalize()?;
    if !path.is_dir() { return Err(io::Error::other("Terminal destination is not a directory")); }
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
    fn kill(pid: i32, signal: i32) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
}
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
        let output = Command::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("stty could not read terminal mode"));
        }
        let mut terminal = Self {
            saved: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            handlers: Vec::new(),
            active: false,
        };
        if terminal.saved.is_empty() {
            return Err(io::Error::other("stty returned an empty terminal mode"));
        }
        raw()?;
        for sig in [SIGINT, SIGTERM, SIGHUP] {
            terminal
                .handlers
                .push((sig, unsafe { signal(sig, stop as *const () as usize) }));
        }
        screen(true)?;
        terminal.active = true;
        Ok(terminal)
    }
    pub fn run_editor(&mut self, editor: &str, path: &Path) -> io::Result<()> {
        self.restore()?;
        let result = match EditorJob::start(editor, path) {
            Ok(mut editor) => {
            let result = loop {
                if self.stopped() { break Err(io::Error::other("Bulk rename editor interrupted")); }
                match editor.status() {
                    Ok(Some(status)) => {
                        break if status.success() { Ok(()) } else { Err(io::Error::other(format!("Bulk rename editor failed ({})", status))) };
                    }
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
        raw()?;
        screen(true)?;
        self.active = true;
        result
    }
    pub fn ready(&self) -> bool { self.active }
    fn restore(&mut self) -> io::Result<()> {
        self.active = false;
        let screen = screen(false);
        let mode = Command::new("stty").arg(&self.saved).status().and_then(|status| {
            if status.success() { Ok(()) } else { Err(io::Error::other("stty could not restore terminal mode")) }
        });
        mode.and(screen)
    }
    pub fn stopped(&self) -> bool {
        STOP.load(Ordering::Relaxed)
    }
    pub fn read(&self) -> io::Result<Vec<u8>> {
        let mut bytes = [0; 256];
        let n = match io::stdin().read(&mut bytes) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => 0,
            Err(e) => return Err(e),
        };
        Ok(bytes[..n].to_vec())
    }
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
        unsafe { kill(-(job.child.id() as i32), SIGCONT); }
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
            if unsafe { kill(-(self.child.id() as i32), SIGKILL) } != 0 && io::Error::last_os_error().raw_os_error() != Some(NO_SUCH_PROCESS) {
                cleanup = Err(io::Error::last_os_error());
            } else { cleanup = self.child.wait().map(|_| ()); }
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
        if let Err(error) = self.restore() { eprintln!("flea: {}", error); }
        for &(sig, handler) in &self.handlers {
            unsafe {
                signal(sig, handler);
            }
        }
    }
}
fn raw() -> io::Result<()> {
    if !Command::new("stty").args(["raw", "-echo", "min", "0", "time", "1"]).status()?.success() {
        return Err(io::Error::other("stty could not enter raw mode"));
    }
    Ok(())
}
fn screen(entered: bool) -> io::Result<()> {
    print!("{}", if entered { "\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[>1u\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c" }
        else { "\x1b[<u\x1b[?1003l\x1b[?1002l\x1b[?1006l\x1b[?2004l\x1b[0m\x1b[?25h\x1b[?1049l" });
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
