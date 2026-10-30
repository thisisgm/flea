use std::{
    io::{self, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
}

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 0o4000;
const SIGKILL: i32 = 9;

struct Owned {
    child: Child,
    finished: bool,
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.finished {
            unsafe {
                kill(-(self.child.id() as i32), SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn spawn(command: &mut Command) -> io::Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

fn capture(child: Child, deadline: Duration, cap: usize) -> io::Result<String> {
    let mut owned = Owned {
        child,
        finished: false,
    };
    let mut stdout = owned.child.stdout.take().expect("piped font output");
    let fd = stdout.as_raw_fd();
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let until = Instant::now() + deadline;
    let mut bytes = Vec::new();
    let mut status = None;
    let mut eof = false;
    loop {
        let mut buffer = [0; 512];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(count) => {
                    if bytes.len() + count > cap {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if status.is_none() {
            status = owned.child.try_wait()?;
        }
        if let (true, Some(status)) = (eof, status) {
            owned.finished = true;
            if !status.success() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let family = String::from_utf8(bytes).map_err(|_| io::ErrorKind::InvalidData)?;
            return if family.trim().is_empty() {
                Err(io::ErrorKind::InvalidData.into())
            } else {
                Ok(family.trim().into())
            };
        }
        if Instant::now() >= until {
            return Err(io::ErrorKind::TimedOut.into());
        }
        thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn fc_match() -> Command {
    let mut command = Command::new("fc-match");
    command.args(["-f", "%{family[0]}", "monospace"]);
    command
}

/// The quick probe's deadline at launch and on each input change, so a slow fontconfig never delays a frame or a theme.
pub(super) const LAUNCH: Duration = Duration::from_millis(250);
/// The worker's one retry after a quick timeout: off the UI thread, a cold fontconfig cache has time to build and answer.
pub(super) const PATIENT: Duration = Duration::from_secs(10);

/// Runs the probe under `deadline` and a 4 KiB output cap; a child that outlives either is killed and reaped.
pub(super) fn probe_within(mut command: Command, deadline: Duration) -> io::Result<String> {
    capture(spawn(&mut command)?, deadline, 4096)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(script: &str) -> Child {
        spawn(Command::new("/bin/sh").args(["-c", script])).unwrap()
    }

    #[test]
    fn font_probe_caps_output_and_kills_and_reaps_a_stalled_process_group() {
        if std::env::var_os("FLEA_U01_FACE_CI").is_none() {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "presentation::face::tests::font_probe_caps_output_and_kills_and_reaps_a_stalled_process_group", "--nocapture"])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("FLEA_U01_FACE_CI", "1")
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        assert!(
            std::fs::File::open("/dev/tty").is_err(),
            "font probe has a controlling tty"
        );
        assert!(std::env::vars_os().all(|(key, _)| key == "PATH" || key == "FLEA_U01_FACE_CI"));
        assert_eq!(
            capture(shell("printf 'Resolved Mono'"), Duration::from_secs(1), 64).unwrap(),
            "Resolved Mono"
        );
        for (script, limit, cap, kind) in [
            (
                "sleep 30 & wait",
                Duration::from_millis(20),
                64,
                io::ErrorKind::TimedOut,
            ),
            (
                "printf '123456789'",
                Duration::from_secs(1),
                4,
                io::ErrorKind::InvalidData,
            ),
            (
                "exit 1",
                Duration::from_secs(1),
                64,
                io::ErrorKind::InvalidData,
            ),
        ] {
            let child = shell(script);
            let pid = child.id();
            assert_eq!(capture(child, limit, cap).unwrap_err().kind(), kind);
            assert!(
                !std::path::Path::new(&format!("/proc/{pid}")).exists(),
                "font helper was not reaped"
            );
        }
    }
}
