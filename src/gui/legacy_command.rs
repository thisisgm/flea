// Bounded legacy IPC calls never leave a pipe reader waiting on a hung process.
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const SIGKILL: i32 = 9;
const MAX_REPLY_BYTES: usize = 64 * 1024;
const REPLY_WAIT_MS: u64 = 750;

pub(super) fn output(command: &mut Command) -> Result<String, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| "the old window could not be contacted")?;
    let mut pipe = child
        .stdout
        .take()
        .ok_or("the old window has no reply pipe")?;
    let flags = unsafe { fcntl(pipe.as_raw_fd(), F_GETFL) };
    if flags < 0
        || unsafe { fcntl(pipe.as_raw_fd(), F_SETFL, flags | crate::oflags::O_NONBLOCK) } < 0
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the old window's reply pipe could not be bounded".into());
    }
    let deadline = Instant::now() + Duration::from_millis(REPLY_WAIT_MS);
    let mut bytes = Vec::new();
    let failure = loop {
        let mut buffer = [0u8; 4096];
        match pipe.read(&mut buffer) {
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break "the old window's reply could not be read".to_string(),
        }
        if bytes.len() > MAX_REPLY_BYTES {
            break "the old window's reply was larger than 64 KiB".to_string();
        }
        if Instant::now() >= deadline {
            break format!("the old window did not answer within {REPLY_WAIT_MS} ms");
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                // The direct child has exited; a descendant holding stdout must not extend the bound.
                loop {
                    match pipe.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                    }
                    if bytes.len() > MAX_REPLY_BYTES {
                        break;
                    }
                }
                if bytes.len() <= MAX_REPLY_BYTES {
                    return String::from_utf8(bytes)
                        .map_err(|_| "the old window's reply is not text".into());
                }
                return Err("the old window's reply was larger than 64 KiB".into());
            }
            Ok(Some(_)) => return Err("the old window's IPC command failed".into()),
            Err(_) => return Err("the old window's IPC command could not be polled".into()),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
    let _ = child.wait();
    Err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oversized_reply_is_named_as_oversized_not_as_a_timeout() {
        let started = Instant::now();
        let error =
            output(Command::new("/bin/sh").args(["-c", "head -c 70000 /dev/zero; sleep 5"]))
                .unwrap_err();
        assert_eq!(error, "the old window's reply was larger than 64 KiB");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the group was killed at the size bound"
        );
    }
}
