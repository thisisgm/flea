// Bounded legacy IPC calls never leave a pipe reader waiting on a hung process.
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

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
    let flags = unsafe { fcntl(pipe.as_raw_fd(), 3) };
    if flags < 0 || unsafe { fcntl(pipe.as_raw_fd(), 4, flags | crate::oflags::O_NONBLOCK) } < 0 {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the old window's reply pipe could not be bounded".into());
    }
    let deadline = Instant::now() + Duration::from_millis(750);
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0u8; 4096];
        match pipe.read(&mut buffer) {
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }
        if bytes.len() > 65536 || Instant::now() >= deadline {
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                // The direct child has exited; a descendant holding stdout must not extend the bound.
                loop {
                    match pipe.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                    }
                    if bytes.len() > 65536 {
                        break;
                    }
                }
                if bytes.len() <= 65536 {
                    return String::from_utf8(bytes)
                        .map_err(|_| "the old window's reply is not text".into());
                }
                return Err("the old window's reply was larger than 64 KiB".into());
            }
            Ok(Some(_)) => return Err("the old window's IPC command failed".into()),
            Err(_) => return Err("the old window's IPC command could not be polled".into()),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    let _ = crate::procsignal::signal_group(child.id() as i32, 9);
    let _ = child.wait();
    Err("the old window did not answer within 750 ms".into())
}
