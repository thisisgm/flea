// Rclone children own a process group and bounded nonblocking output, including during discovery.
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_LINE: usize = 64 * 1024;
// Bytes one read takes from a stream.
const READ_CHUNK: usize = 8192;
// Reads per stream between exit checks, so a chatty stream cannot delay cancellation.
const READS_PER_WAIT: usize = 8;
// How long one exit check waits before both streams are drained again.
const EXIT_POLL: Duration = Duration::from_millis(10);
// Reads per stream after the child is reaped, bounding the final drain.
const DRAIN_READS: usize = 128;
const SIGKILL: i32 = 9;

pub(super) struct Output {
    pub success: bool,
    pub text: String,
}

#[derive(Default)]
struct Lines {
    pending: Vec<u8>,
    oversized: bool,
}

impl Lines {
    fn push(&mut self, bytes: &[u8], line: &mut impl FnMut(&str)) {
        for &byte in bytes {
            if byte == b'\n' {
                self.finish(line);
            } else if self.pending.len() < MAX_LINE && !self.oversized {
                self.pending.push(byte);
            } else {
                self.oversized = true;
                self.pending.clear();
            }
        }
    }

    fn finish(&mut self, line: &mut impl FnMut(&str)) {
        if !self.oversized && !self.pending.is_empty() {
            line(&String::from_utf8_lossy(&self.pending));
        }
        self.pending.clear();
        self.oversized = false;
    }
}

// One child stream with its own partial line, so stdout and stderr bytes never join one line.
struct Stream {
    reader: UnixStream,
    lines: Lines,
}

impl Stream {
    fn open() -> Result<(Self, OwnedFd), String> {
        let (reader, writer) = UnixStream::pair().map_err(|e| e.to_string())?;
        reader.set_nonblocking(true).map_err(|e| e.to_string())?;
        let lines = Lines::default();
        Ok((Self { reader, lines }, writer.into()))
    }

    // An empty or ended stream stops the reads; only a read error is returned.
    fn drain(
        &mut self,
        buffer: &mut [u8],
        reads: usize,
        line: &mut impl FnMut(&str),
    ) -> Option<String> {
        for _ in 0..reads {
            match self.reader.read(buffer) {
                Ok(0) => break,
                Ok(n) => self.lines.push(&buffer[..n], line),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Some(e.to_string()),
            }
        }
        None
    }
}

pub(super) fn run(
    args: &[String],
    cancel: &AtomicBool,
    limit: Option<Duration>,
    capture: bool,
    line: impl FnMut(&str),
) -> Result<Output, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Send cancelled.".into());
    }
    let mut command = Command::new("rclone");
    super::rcloneenv::sanitize(&mut command);
    command.args(flea::bytename::args(args));
    drive(command, cancel, limit, capture, line)
}

// Every line of either stream reaches `line`, and only stdout is captured, so a stderr notice never joins parsed output.
fn drive(
    mut command: Command,
    cancel: &AtomicBool,
    limit: Option<Duration>,
    capture: bool,
    mut line: impl FnMut(&str),
) -> Result<Output, String> {
    let (mut out, stdout) = Stream::open()?;
    let (mut err, stderr) = Stream::open()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .process_group(0);
    crate::open::prepare(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start rclone: {}.", e))?;
    drop(command);
    let started = Instant::now();
    let mut text = String::new();
    let mut overflow = false;
    let mut keep = |value: &str, stdout: bool| {
        if capture && stdout {
            if text.len() + value.len() + 1 > MAX_LINE {
                overflow = true;
            } else {
                text.push_str(value);
                text.push('\n');
            }
        }
        line(value);
    };
    let mut buffer = [0u8; READ_CHUNK];
    loop {
        let stdout_error = out.drain(&mut buffer, READS_PER_WAIT, &mut |v| keep(v, true));
        let stderr_error = err.drain(&mut buffer, READS_PER_WAIT, &mut |v| keep(v, false));
        let cancelled = cancel.load(Ordering::Relaxed);
        let expired = limit.is_some_and(|limit| started.elapsed() >= limit);
        let mut failure = stdout_error.or(stderr_error);
        let exited = match super::child::wait_for_exit(&child, Instant::now() + EXIT_POLL) {
            super::child::Woke::Exited => true,
            super::child::Woke::Deadline => false,
            super::child::Woke::Failed(error) => {
                failure = Some(error.to_string());
                false
            }
        };
        if cancelled || expired || exited || failure.is_some() {
            // Kill before reaping so the group id cannot have been reused by another child.
            let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
            let status = child.wait().map_err(|e| e.to_string())?;
            let _ = out.drain(&mut buffer, DRAIN_READS, &mut |v| keep(v, true));
            let _ = err.drain(&mut buffer, DRAIN_READS, &mut |v| keep(v, false));
            out.lines.finish(&mut |v| keep(v, true));
            err.lines.finish(&mut |v| keep(v, false));
            return if cancelled {
                Err("Send cancelled.".into())
            } else if expired {
                Err("rclone stopped answering.".into())
            } else if let Some(error) = failure {
                Err(format!("Could not read rclone: {}.", error))
            } else if overflow {
                Err("rclone returned too much output.".into())
            } else {
                Ok(Output {
                    success: status.success(),
                    text,
                })
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rclone_log_lines_are_bounded_and_recover_after_an_oversized_line() {
        let mut lines = Lines::default();
        let mut found = Vec::new();
        let mut line = |text: &str| found.push(text.to_string());
        lines.push(&vec![b'x'; MAX_LINE + 1], &mut line);
        assert!(lines.pending.is_empty());
        lines.push(b"discarded\n{\"stats\":{}}\n", &mut line);
        assert_eq!(found, ["{\"stats\":{}}"]);
    }

    // Lines per flood and bytes per line, so each flood is 1 MiB, several times a stream buffer.
    const FLOOD_LINES: usize = 1024;
    const FLOOD_WIDTH: usize = 1023;
    // Far past a 3 MiB drain, so only a deadlock reaches it.
    const FLOOD_LIMIT: Duration = Duration::from_secs(30);

    fn flood(script: &str, capture: bool) -> (Result<Output, String>, usize) {
        let mut command = Command::new("/bin/sh");
        let flood = format!(
            "yes \"$(printf '%0{}d' 0)\" | head -n {}",
            FLOOD_WIDTH, FLOOD_LINES
        );
        command.args(["-c", &script.replace("FLOOD", &flood)]);
        let mut lines = 0;
        let cancel = AtomicBool::new(false);
        let result = drive(command, &cancel, Some(FLOOD_LIMIT), capture, |_| lines += 1);
        (result, lines)
    }

    #[test]
    fn rclone_drains_stdout_and_stderr_past_their_buffers_and_captures_only_stdout() {
        let (result, lines) = flood("FLOOD; FLOOD >&2; FLOOD", false);
        assert!(result.unwrap().success);
        assert_eq!(lines, 3 * FLOOD_LINES);
        let (result, lines) = flood("FLOOD >&2; echo '{\"Hashes\":[]}'; FLOOD >&2", true);
        let output = result.unwrap();
        assert!(output.success);
        assert_eq!(output.text, "{\"Hashes\":[]}\n");
        assert_eq!(lines, 2 * FLOOD_LINES + 1);
        let (result, _) = flood("FLOOD; FLOOD >&2", true);
        assert_eq!(result.err().unwrap(), "rclone returned too much output.");
    }
}
