use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::time::Instant;
use super::child::{wait_for_exit, Woke};

const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_ERROR: usize = 16 * 1024;
const READ_BUFFER: usize = 8192;
const MAX_READ_TURN: usize = 64 * 1024;
const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 0o4000;
const SIGKILL: i32 = 9;

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(test)]
thread_local! {
    pub(super) static SIGNALLED_UNREAPED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

fn signal_group(child: &Child) {
    #[cfg(test)]
    SIGNALLED_UNREAPED.with(|seen| {
        if seen.get().is_some() {
            seen.set(Some(std::path::Path::new(&format!("/proc/{}", child.id())).exists()));
        }
    });
    unsafe { kill(-(child.id() as i32), SIGKILL); }
}

// Nonblocking pipes keep a descendant holding stdout open from extending the discovery deadline.
fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) struct Tool {
    child: Option<Child>,
    stdout: ChildStdout,
    stderr: ChildStderr,
    pub service: Option<&'static str>,
    output: Vec<u8>,
    error: Vec<u8>,
    pub problem: Option<String>,
    ended: bool,
}

impl Tool {
    pub fn spawn(program: &str, args: &[&str], service: Option<&'static str>, path: Option<&Path>) -> Result<Self, String> {
        let mut command = Command::new(program);
        command.args(args).env("LC_ALL", "C").process_group(0)
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(path) = path { command.env("PATH", path); }
        let mut child = command.spawn().map_err(|e| format!("{program} could not start: {e}"))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let mut tool = Self { child: Some(child), stdout, stderr, service, output: Vec::new(), error: Vec::new(), problem: None, ended: false };
        if let Err(error) = nonblocking(&tool.stdout).and_then(|_| nonblocking(&tool.stderr)) {
            tool.terminate();
            return Err(format!("{program} output could not be read: {error}"));
        }
        Ok(tool)
    }

    pub fn poll(&mut self) {
        if self.ended { return; }
        let eof = match self.drain() {
            Ok(eof) => eof,
            Err(error) => {
                self.problem = Some(format!("Discovery output could not be read: {error}"));
                self.terminate();
                self.ended = true;
                return;
            }
        };
        let Some(child) = self.child.as_mut() else { self.ended = eof; return; };
        match wait_for_exit(child, Instant::now()) {
            Woke::Exited => {
                // The unreaped leader reserves its group ID until every descendant has been signalled.
                signal_group(child);
                match child.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() { self.problem = Some(format!("Discovery tool failed ({status})")); }
                        self.ended = eof;
                        self.child = None;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.problem = Some(format!("Discovery tool could not be waited for: {error}"));
                        self.terminate();
                        self.ended = true;
                    }
                }
            }
            Woke::Deadline => {}
            Woke::Failed(error) => {
                self.problem = Some(format!("Discovery tool could not be waited for: {error}"));
                self.terminate();
                self.ended = true;
            }
        }
    }

    pub fn ended(&self) -> bool { self.ended }

    pub fn finish(&mut self, timed_out: bool) -> (String, Option<String>) {
        if timed_out {
            if !self.ended { self.problem = Some(format!("Discovery tool timed out after {} seconds", super::nearby::LIMIT.as_secs_f64())); }
            if let Err(error) = self.drain() { self.problem = Some(format!("Discovery output could not be read: {error}")); }
        }
        self.terminate();
        let detail = String::from_utf8_lossy(&self.error).trim().replace(['\r', '\n'], " ");
        let problem = self.problem.take().map(|problem| {
            if detail.is_empty() { format!("{problem}.") } else { format!("{problem}: {detail}") }
        });
        let mut output = String::from_utf8_lossy(&self.output).into_owned();
        if self.service.is_some() && !output.ends_with('\n') {
            output.truncate(output.rfind('\n').map_or(0, |at| at + 1));
        }
        (output, problem)
    }

    fn drain(&mut self) -> io::Result<bool> {
        // Give both nonblocking pipes one capped turn even if the first exceeds its total limit.
        let stdout = read_pipe(&mut self.stdout, &mut self.output, MAX_OUTPUT);
        let stderr = read_pipe(&mut self.stderr, &mut self.error, MAX_ERROR);
        let stdout = stdout?;
        let stderr = stderr?;
        Ok(stdout && stderr)
    }

    fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            signal_group(&child);
            // An uninterruptible helper must not hold the dialog or the three second deadline.
            std::thread::spawn(move || { let _ = child.wait(); });
        }
    }
}

impl Drop for Tool {
    fn drop(&mut self) { self.terminate(); }
}

fn read_pipe(pipe: &mut impl Read, output: &mut Vec<u8>, limit: usize) -> io::Result<bool> {
    let mut buffer = [0; READ_BUFFER];
    // Each source gets one capped turn, so an infinite writer cannot starve cancellation or siblings.
    let mut read = 0;
    while read < MAX_READ_TURN {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                if output.len() + count > limit { return Err(io::Error::other("output exceeded its limit")); }
                output.extend_from_slice(&buffer[..count]);
                read += count;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}
