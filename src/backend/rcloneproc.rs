// Rclone children own a process group and bounded nonblocking output, including during discovery.
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_LINE: usize = 64 * 1024;

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

pub(super) fn run(
    args: &[String],
    cancel: &AtomicBool,
    limit: Option<Duration>,
    capture: bool,
    mut line: impl FnMut(&str),
) -> Result<Output, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Send cancelled.".into());
    }
    let (mut reader, writer) = UnixStream::pair().map_err(|e| e.to_string())?;
    reader.set_nonblocking(true).map_err(|e| e.to_string())?;
    let stdout: OwnedFd = writer.try_clone().map_err(|e| e.to_string())?.into();
    let stderr: OwnedFd = writer.into();
    let mut command = Command::new("rclone");
    super::rcloneenv::sanitize(&mut command);
    command
        .args(args)
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
    let mut lines = Lines::default();
    let mut text = String::new();
    let mut overflow = false;
    let mut consume = |value: &str| {
        if capture {
            if text.len() + value.len() + 1 > MAX_LINE {
                overflow = true;
            } else {
                text.push_str(value);
                text.push('\n');
            }
        }
        line(value);
    };
    let mut buffer = [0u8; 8192];
    loop {
        let mut read_error = None;
        for _ in 0..8 {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => lines.push(&buffer[..n], &mut consume),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    read_error = Some(e.to_string());
                    break;
                }
            }
        }
        let cancelled = cancel.load(Ordering::Relaxed);
        let expired = limit.is_some_and(|limit| started.elapsed() >= limit);
        let mut failure = read_error;
        let exited =
            match super::child::wait_for_exit(&child, Instant::now() + Duration::from_millis(10)) {
                super::child::Woke::Exited => true,
                super::child::Woke::Deadline => false,
                super::child::Woke::Failed(error) => {
                    failure = Some(error.to_string());
                    false
                }
            };
        if cancelled || expired || exited || failure.is_some() {
            // Kill before reaping so the group id cannot have been reused by another child.
            let _ = crate::procsignal::signal_group(child.id() as i32, 9);
            let status = child.wait().map_err(|e| e.to_string())?;
            for _ in 0..128 {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => lines.push(&buffer[..n], &mut consume),
                }
            }
            lines.finish(&mut consume);
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
}
