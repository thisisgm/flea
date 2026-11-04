//! A chooser-owned read-only child: its lines in, its whole lines out, and a quit that kills because nothing it does needs a drain.
use async_channel::{Receiver, Sender};
use flea::json::field_str;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::Duration;

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
const SIGKILL: i32 = 9;
const REAP_POLL: Duration = Duration::from_millis(5);
pub(super) const CHECKS: &[&str] = &["picker", "formats", "mkdir"];
pub(super) const LISTING: &[&str] = &[
    "list",
    "listpaths",
    "window",
    "filter",
    "peek",
    "locate",
    "sort",
    "paths",
    "jump",
    "thumb",
    "thumbcancel",
    "fsinfo",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Reply(String),
    // The child could not start or left while it was still wanted.
    Failed(String),
    // The child is reaped; it is sent exactly once, after every reply it wrote.
    Stopped,
}

// The two roles a chooser child plays; the listing worker's owner supplies its own implementation.
pub trait PickerChild {
    fn send(&self, line: &str) -> bool;
    fn quit(&self);
    fn lines(&self) -> Receiver<Line>;
}

struct Reaper {
    child: Option<Child>,
    reaped: bool,
}

pub struct PickerProcess {
    allowed: &'static [&'static str],
    requests: Option<mpsc::Sender<String>>,
    quitting: Arc<AtomicBool>,
    reaper: Arc<Mutex<Reaper>>,
    lines: Receiver<Line>,
}

impl PickerProcess {
    pub fn checks() -> Self {
        Self::backend(CHECKS)
    }

    // The listing worker takes only the read-only listing and path lines the chooser draws from.
    pub fn listing() -> Self {
        Self::backend(LISTING)
    }

    fn backend(allowed: &'static [&'static str]) -> Self {
        let mut command = Command::new(crate::window::startup::binary());
        command.arg("--backend");
        let mut process = Self::start(command);
        process.allowed = allowed;
        process
    }

    // A start that fails still answers: one failure and one stop, so the lifetime never waits on it.
    pub fn start(mut command: Command) -> Self {
        let (sender, lines) = async_channel::unbounded();
        let quitting = Arc::new(AtomicBool::new(false));
        let spawned = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                let _ = sender.try_send(Line::Failed(error.to_string()));
                let _ = sender.try_send(Line::Stopped);
                return Self {
                    allowed: CHECKS,
                    requests: None,
                    quitting,
                    reaper: Arc::new(Mutex::new(Reaper {
                        child: None,
                        reaped: true,
                    })),
                    lines,
                };
            }
        };
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let reaper = Arc::new(Mutex::new(Reaper {
            child: Some(child),
            reaped: false,
        }));
        let (requests, queue) = mpsc::channel();
        if let Some(stdin) = stdin {
            let quitting = quitting.clone();
            let _ = thread::Builder::new()
                .name("flea-picker-in".into())
                .spawn(move || write_lines(stdin, &queue, &quitting));
        }
        let (reaper_out, quitting_out) = (reaper.clone(), quitting.clone());
        let _ = thread::Builder::new()
            .name("flea-picker-out".into())
            .spawn(move || read_lines(stdout, &sender, &reaper_out, &quitting_out));
        Self {
            allowed: CHECKS,
            requests: Some(requests),
            quitting,
            reaper,
            lines,
        }
    }

    fn kill(&self) {
        let reaper = self
            .reaper
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Reaping happens only under this lock, so the pid cannot have been recycled.
        if let (false, Some(child)) = (reaper.reaped, &reaper.child) {
            if let Ok(pid) = i32::try_from(child.id()) {
                unsafe { kill(pid, SIGKILL) };
            }
        }
    }
}

impl PickerChild for PickerProcess {
    // Each child takes only its own role's lines: checks picker and formats, the listing its read-only listing lines.
    fn send(&self, line: &str) -> bool {
        if self.quitting.load(Ordering::Acquire) || !admits(self.allowed, line) {
            return false;
        }
        self.requests
            .as_ref()
            .is_some_and(|requests| requests.send(line.to_owned()).is_ok())
    }

    fn quit(&self) {
        if !self.quitting.swap(true, Ordering::AcqRel) {
            self.kill();
        }
    }

    fn lines(&self) -> Receiver<Line> {
        self.lines.clone()
    }
}

// A line goes to a child only when its command is one the child's role takes.
pub(super) fn admits(allowed: &[&str], line: &str) -> bool {
    field_str(line, "c").is_some_and(|command| allowed.contains(&command.as_str()))
}

impl Drop for PickerProcess {
    fn drop(&mut self) {
        self.quit();
    }
}

fn write_lines(mut stdin: ChildStdin, queue: &mpsc::Receiver<String>, quitting: &AtomicBool) {
    while let Ok(line) = queue.recv() {
        if quitting.load(Ordering::Acquire)
            || stdin.write_all(format!("{line}\n").as_bytes()).is_err()
        {
            return;
        }
    }
}

fn read_lines(
    stdout: Option<std::process::ChildStdout>,
    sender: &Sender<Line>,
    reaper: &Mutex<Reaper>,
    quitting: &AtomicBool,
) {
    if let Some(stdout) = stdout {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                // A line the child never finished is stale, so it is dropped rather than parsed.
                Ok(_) if buffer.last() != Some(&b'\n') => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buffer[..buffer.len() - 1]).into_owned();
                    let _ = sender.try_send(Line::Reply(text));
                }
            }
        }
    }
    loop {
        let mut state = reaper
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let exited = !matches!(state.child.as_mut().map(Child::try_wait), Some(Ok(None)));
        if exited {
            state.reaped = true;
            break;
        }
        drop(state);
        thread::sleep(REAP_POLL);
    }
    if !quitting.load(Ordering::Acquire) {
        let _ = sender.try_send(Line::Failed("exited".into()));
    }
    let _ = sender.try_send(Line::Stopped);
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
