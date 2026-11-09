use super::{protocol, session::Config};
use crate::jsondoc::Json;
use crate::ui_logic_plugin_settings::wire::compact;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const PR_SET_PDEATHSIG: i32 = 1;
const SIGKILL: i32 = 9;
const TIMED_OUT: &str = "plugin timed out";
const RLIMIT_AS: i32 = 9;
const RLIMIT_NOFILE: i32 = 7;

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, ...) -> i32;
    fn getppid() -> i32;
    fn setrlimit(resource: i32, limits: *const Limits) -> i32;
}

#[repr(C)]
struct Limits {
    current: u64,
    maximum: u64,
}

pub(super) struct Process {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    bytes: Vec<u8>,
}

fn nonblocking(fd: RawFd) -> Result<(), String> {
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | crate::oflags::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

impl Process {
    pub(super) fn start(config: &Config, stop: &AtomicBool) -> Result<Self, String> {
        let deadline = Instant::now() + config.ready_timeout;
        let path = config.entry()?;
        let parent = std::process::id() as i32;
        let mut command = Command::new(path);
        command
            .current_dir(&config.folder)
            .env("LC_ALL", "C")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        unsafe {
            command.pre_exec(move || {
                let memory = Limits {
                    current: 512 << 20,
                    maximum: 512 << 20,
                };
                let files = Limits {
                    current: 256,
                    maximum: 256,
                };
                if prctl(PR_SET_PDEATHSIG, SIGKILL, 0, 0, 0) != 0
                    || getppid() != parent
                    || setrlimit(RLIMIT_AS, &memory) != 0
                    || setrlimit(RLIMIT_NOFILE, &files) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("missing plugin stdin")?;
        let output = child.stdout.take().ok_or("missing plugin stdout")?;
        let mut process = Self {
            child,
            input,
            output,
            bytes: Vec::new(),
        };
        nonblocking(process.input.as_raw_fd())?;
        nonblocking(process.output.as_raw_fd())?;
        let hello = |error: String| {
            if error == TIMED_OUT {
                protocol::HELLO_TIMEOUT.to_string()
            } else {
                error
            }
        };
        process
            .write(&config.hello, deadline, stop)
            .map_err(hello)?;
        protocol::ready(&process.read(deadline, stop).map_err(hello)?)?;
        Ok(process)
    }

    pub(super) fn exited(&mut self) -> bool {
        self.child
            .try_wait()
            .map_or(true, |status| status.is_some())
    }

    pub(super) fn write(
        &mut self,
        value: &Json,
        deadline: Instant,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        let mut bytes = compact(value).into_bytes();
        if bytes.len() > protocol::MAX_MESSAGE {
            return Err("plugin request exceeds 64 KiB".into());
        }
        bytes.push(b'\n');
        let mut at = 0;
        while at < bytes.len() {
            check(deadline, stop)?;
            match self.input.write(&bytes[at..]) {
                Ok(0) => return Err("plugin closed stdin".into()),
                Ok(n) => at += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => pause(),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    pub(super) fn read(&mut self, deadline: Instant, stop: &AtomicBool) -> Result<Json, String> {
        loop {
            check(deadline, stop)?;
            if let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
                let line: Vec<_> = self.bytes.drain(..=end).collect();
                return protocol::parse(&line[..line.len() - 1]);
            }
            if self.bytes.len() > protocol::MAX_MESSAGE {
                return Err("plugin message exceeds 64 KiB".into());
            }
            let mut buffer = [0; 4096];
            match self.output.read(&mut buffer) {
                Ok(0) => return Err("plugin closed stdout".into()),
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => pause(),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

fn check(deadline: Instant, stop: &AtomicBool) -> Result<(), String> {
    if stop.load(Ordering::Acquire) {
        return Err("plugin stopped".into());
    }
    if Instant::now() >= deadline {
        return Err(TIMED_OUT.into());
    }
    Ok(())
}

fn pause() {
    std::thread::sleep(Duration::from_millis(5));
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.input.write_all(b"{\"op\":\"cancel\"}\n");
        unsafe {
            kill(-(self.child.id() as i32), SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
