use std::io::{self, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);
static JOB_CONTROL: AtomicU8 = AtomicU8::new(0);
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
const SIGHUP: i32 = 1;
const SIGKILL: i32 = 9;
const SIGCONT: i32 = 18;
const SIGSTOP: i32 = 19;
const SIGTSTP: i32 = 20;
const SIGWINCH: i32 = 28;
const SIGTTOU: i32 = 22;
const INPUT_READ_BYTES: usize = 4096;
// The shown faults of the stty calls: raw mode at the start and the terminal handed back at the end, then the same two around the editor.
const PREPARE_FAILED: &str = "tui.terminal.the_terminal_could_not_be_prepared";
const RESTORE_FAILED: &str = "tui.terminal.the_terminal_could_not_be_restored";
const PREPARE_FOR_EDITOR_FAILED: &str = "tui.terminal.the_terminal_could_not_be_prepared_for_the_editor";
const RESTORE_AFTER_EDITOR_FAILED: &str = "tui.terminal.the_terminal_could_not_be_restored_after_the_editor";
// Any editor handoff ends with this when a stop lands inside the editor or while either stty around it runs.
const EDITOR_INTERRUPTED: &str = "tui.terminal.editor_interrupted";
#[repr(C)]
struct WindowSize {
    rows: u16,
    columns: u16,
    x: u16,
    y: u16,
}
// The editor's exit as the bulk rename sees it: a nonzero status ends the rename with the status shown.
fn editor_result(status: std::process::ExitStatus) -> io::Result<()> {
    if status.success() { Ok(()) } else { Err(io::Error::other(super::words::args("tui.terminal.editor_status", vec![status.to_string().into()]))) }
}
pub use super::launch::terminal as launch;
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

// Installing this handler makes poll return on resize; the loop then measures the new grid.
extern "C" fn resized(_: i32) {}
extern "C" fn job_control(sig: i32) {
    JOB_CONTROL.store(if sig == SIGTSTP { 1 } else { 2 }, Ordering::Relaxed);
}

pub struct Terminal {
    saved: String,
    handlers: Vec<(i32, usize)>,
    active: bool,
    keyboard_pushed: bool,
    resumed: bool,
    trace: bool,
    transport: super::tmux::Transport,
    pub decoder: super::input::Decoder,
    shm_directory: std::path::PathBuf,
}
impl Terminal {
    pub fn enter() -> io::Result<Self> {
        Self::enter_at(Path::new(SHM_DIR))
    }
    fn enter_at(shm_directory: &Path) -> io::Result<Self> {
        Self::enter_observed(shm_directory, |_| {})
    }
    fn enter_observed(shm_directory: &Path, mut installed: impl FnMut(i32)) -> io::Result<Self> {
        let output = crate::backend::locale::command("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()?;
        stty_result(output.status, PREPARE_FAILED)?;
        let mut terminal = Self {
            saved: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            handlers: Vec::new(),
            active: false,
            keyboard_pushed: false,
            resumed: false,
            trace: std::env::var("FLEA_TUI_CAPABILITIES").as_deref() == Ok("1"),
            transport: super::tmux::Transport::discover(),
            decoder: super::input::Decoder::default(),
            shm_directory: shm_directory.to_owned(),
        };
        if terminal.saved.is_empty() {
            return Err(super::words::error(PREPARE_FAILED));
        }
        raw(PREPARE_FAILED)?;
        STOP.store(false, Ordering::Relaxed);
        JOB_CONTROL.store(0, Ordering::Relaxed);
        for sig in [SIGINT, SIGTERM, SIGHUP, SIGWINCH, SIGTSTP, SIGCONT] {
            let handler = match sig {
                SIGWINCH => resized as *const () as usize,
                SIGTSTP | SIGCONT => job_control as *const () as usize,
                _ => stop as *const () as usize,
            };
            terminal.handlers.push((sig, unsafe { signal(sig, handler) }));
            installed(sig);
        }
        terminal.start_screen()?;
        Ok(terminal)
    }
    pub fn run_editor(&mut self, editor: &str, path: &Path) -> io::Result<()> {
        // The bulk editor is shell syntax; keep filename bytes in $1, outside the shell program.
        let mut command = Command::new("/bin/sh");
        command.args(["-c", &format!("exec {} \"$1\"", editor), "flea-bulk-rename"]).arg(path);
        self.run_command(command)
    }
    pub fn run_command(&mut self, command: Command) -> io::Result<()> {
        // A stop that lands while either stty runs ends the edit as interrupted, and Drop restores the terminal.
        if let Err(error) = self.restore(PREPARE_FOR_EDITOR_FAILED) {
            return Err(if self.stopped() { super::words::error(EDITOR_INTERRUPTED) } else { error });
        }
        let result = match EditorJob::start(command) {
            Ok(mut editor) => {
            let result = loop {
                if self.stopped() { break Err(super::words::error(EDITOR_INTERRUPTED)); }
                match editor.status() {
                    Ok(Some(status)) => break editor_result(status),
                    Err(error) => break Err(error),
                    Ok(None) => {}
                }
                const INTERRUPT_POLL: std::time::Duration = std::time::Duration::from_millis(50);
                std::thread::sleep(INTERRUPT_POLL);
            };
            result.and(editor.close())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = raw(RESTORE_AFTER_EDITOR_FAILED) {
            return Err(if self.stopped() { super::words::error(EDITOR_INTERRUPTED) } else { error });
        }
        self.start_screen()?;
        result
    }
    fn start_screen(&mut self) -> io::Result<()> {
        let ids = query_ids();
        let shared = if self.transport == super::tmux::Transport::Direct {
            shm_query(&self.shm_directory, &shm_name(), ids.1).unwrap_or_default()
        } else { String::new() };
        let negotiation = super::capabilities::Negotiation::begin(self.transport, ids, !shared.is_empty(), std::time::Instant::now());
        let bytes = opening(&shared, &negotiation);
        self.decoder = super::input::Decoder::new(negotiation);
        self.transport.publish();
        self.active = true;
        let mut stdout = io::stdout().lock();
        stdout.write_all(&bytes)?;
        stdout.flush()?;
        self.trace_capabilities();
        Ok(())
    }
    fn trace_capabilities(&self) {
        if self.trace { let _ = writeln!(io::stderr().lock(), "flea: terminal capabilities {:?}", self.decoder.negotiation.detected); }
    }
    pub fn decode(&mut self, bytes: &[u8], idle: bool) -> io::Result<Vec<super::input::Key>> {
        let previous = self.decoder.negotiation.detected;
        let keys = self.decoder.feed(bytes, idle);
        if self.decoder.negotiation.take_keyboard_enable() {
            self.keyboard_pushed = true;
            let mut stdout = io::stdout().lock();
            stdout.write_all(super::capabilities::KEYBOARD_ENABLE)?;
            stdout.flush()?;
        }
        if previous != self.decoder.negotiation.detected { self.trace_capabilities(); }
        Ok(keys)
    }
    pub fn request_suspend(&mut self) { JOB_CONTROL.store(1, Ordering::Relaxed); }
    pub fn take_resumed(&mut self) -> bool { std::mem::take(&mut self.resumed) }
    fn service_job_control(&mut self) -> io::Result<bool> {
        if JOB_CONTROL.swap(0, Ordering::Relaxed) != 1 || self.stopped() { return Ok(false); }
        // Keep asynchronous graphics writers off the shell's screen until the alternate screen returns.
        let _output = io::stdout().lock();
        // A stop that lands while stty runs ends the transition without a fault, and Drop restores the terminal.
        if let Err(error) = self.restore(RESTORE_FAILED) {
            return if self.stopped() { Ok(true) } else { Err(error) };
        }
        if JOB_CONTROL.swap(0, Ordering::Relaxed) != 2 && !self.stopped() {
            crate::procsignal::signal_process(std::process::id() as i32, SIGSTOP)?;
        }
        JOB_CONTROL.compare_exchange(2, 0, Ordering::Relaxed, Ordering::Relaxed).ok();
        if !self.stopped() {
            if let Err(error) = raw(PREPARE_FAILED) {
                return if self.stopped() { Ok(true) } else { Err(error) };
            }
            self.start_screen()?;
            self.resumed = true;
        }
        Ok(true)
    }
    pub fn ready(&self) -> bool { self.active }
    fn restore(&mut self, fault: &'static str) -> io::Result<()> {
        self.active = false;
        let screen = leave_screen(self.keyboard_pushed);
        self.keyboard_pushed = false;
        let mode = Command::new("stty").arg(&self.saved).status().and_then(|status| stty_result(status, fault));
        mode.and(screen)
    }
    pub fn stopped(&self) -> bool {
        STOP.load(Ordering::Relaxed)
    }
    // Waits up to timeout_ms for a key or a ring on wake; raw mode is min 0 time 0, so the read itself never blocks.
    pub fn read(&mut self, wake: i32, timeout_ms: i32) -> io::Result<Vec<u8>> {
        if self.service_job_control()? { return Ok(Vec::new()); }
        let ready = wait_input(0, wake, timeout_ms)?;
        if self.service_job_control()? || !ready {
            return Ok(Vec::new());
        }
        let mut bytes = [0; INPUT_READ_BYTES];
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
#[cfg(test)]
#[path = "terminal_adv_tests.rs"]
mod adv_tests;
// The read's poll on its own, so the hold and wire tests can time it on a socket pair; true once stdin has anything to report.
pub(super) fn wait_input(stdin: i32, wake: i32, timeout_ms: i32) -> io::Result<bool> {
    let mut fds = [PollFd { fd: stdin, events: POLLIN, revents: 0 }, PollFd { fd: wake, events: POLLIN, revents: 0 }];
    if unsafe { poll(fds.as_mut_ptr(), 2, timeout_ms) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted { return Err(error); }
    }
    Ok(fds[0].revents != 0)
}
#[path = "terminal_editor.rs"]
mod editor_job;
use editor_job::EditorJob;
impl Drop for Terminal {
    fn drop(&mut self) {
        // A second stop signal can kill this stty too, and nothing restores the terminal after Drop, so it gets one retry.
        let restored = self.restore(RESTORE_FAILED).or_else(|_| self.restore(RESTORE_FAILED));
        if let Err(error) = restored { eprintln!("flea: {}", error); }
        // kitty unlinks the object once it reads it; a terminal that could not read it leaves it here.
        let _ = std::fs::remove_file(self.shm_directory.join(shm_name()));
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
// Kitty answers this query with OK only when it can read the private pixel from shared memory.
fn shm_query(dir: &Path, name: &str, id: u32) -> Option<String> {
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).ok()?;
    file.write_all(&[0, 0, 0]).ok()?;
    let encoded = super::graphics::base64(format!("/{}", name).as_bytes());
    Some(format!("\x1b_Gi={id},s=1,v=1,a=q,t=s,f=24;{}\x1b\\", encoded))
}
// A stty call's exit as a shown fault: a nonzero status says what the call was for.
fn stty_result(status: std::process::ExitStatus, fault: &'static str) -> io::Result<()> {
    // A stop signal sent to the job kills its stty too, so that death marks the stop before any thread's handler runs.
    if matches!(status.signal(), Some(SIGINT | SIGTERM | SIGHUP)) { STOP.store(true, Ordering::Relaxed); }
    if status.success() { Ok(()) } else { Err(super::words::error(fault)) }
}
fn raw(fault: &'static str) -> io::Result<()> {
    stty_result(Command::new("stty").args(["raw", "-echo", "min", "0", "time", "0"]).status()?, fault)
}
const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[?2004h";
const LEAVE: &[u8] = b"\x1b[?1003l\x1b[?1002l\x1b[?1006l\x1b[?2004l\x1b[0m\x1b[0 q\x1b[?25h\x1b[?1049l";
fn query_ids() -> (u32, u32) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let id = std::process::id().wrapping_mul(1024).wrapping_add(NEXT.fetch_add(2, Ordering::Relaxed)).max(100);
    (id, id + 1)
}
fn opening(query: &str, negotiation: &super::capabilities::Negotiation) -> Vec<u8> {
    let mut bytes = ENTER.to_vec();
    bytes.extend(negotiation.queries(query));
    bytes
}
fn closing(keyboard_pushed: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if keyboard_pushed { bytes.extend_from_slice(super::capabilities::KEYBOARD_DISABLE); }
    bytes.extend_from_slice(LEAVE);
    bytes
}
fn leave_screen(keyboard_pushed: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(&closing(keyboard_pushed))?;
    stdout.flush()
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
// PTY fixtures model a foreground shell, which unblocks job-control signals before starting its command.
#[cfg(test)]
fn pty_test_command() -> Command {
    let mut command = Command::new("env");
    command.arg("--default-signal=TSTP,TTIN,TTOU").arg(std::env::current_exe().unwrap());
    command
}
#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
