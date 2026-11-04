use super::*;
use std::os::fd::{AsRawFd, FromRawFd};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// Only a hung child reaches this bound; every wait below ends on a transcript marker or a waitpid report first.
const LIMIT: Duration = Duration::from_secs(10);
// waitpid(2) WUNTRACED: report a stop as well as an exit.
const UNTRACED: i32 = 2;
const TIOCSCTTY: usize = 0x540e;

// One review_suspend_child leading its own session on its own PTY, in a mode; a gate puts the sandbox's stty first on PATH.
pub(super) struct Session {
    master: std::fs::File,
    pub slave: std::fs::File,
    pub saved: Vec<u8>,
    child: Child,
    pub transcript: Vec<u8>,
    reaped: bool,
}

impl Session {
    pub fn start(root: &Path, child_mode: &str, gate: Option<&str>) -> Self {
        #[link(name = "util")]
        extern "C" {
            fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
            fn setsid() -> i32;
        }
        let (mut master, mut slave) = (-1, -1);
        let size = WindowSize { rows: 31, columns: 130, x: 1040, y: 682 };
        assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
        let master = unsafe { std::fs::File::from_raw_fd(master) };
        let slave = unsafe { std::fs::File::from_raw_fd(slave) };
        let saved = mode(&slave);
        let mut command = pty_test_command();
        command.args(["--exact", "tui::terminal::tests::review_suspend_child", "--nocapture"])
            .env("XDG_DATA_HOME", root.join("data")).env("XDG_CONFIG_HOME", root.join("config")).env("XDG_CACHE_HOME", root.join("cache"))
            .env("FLEA_T1_REVIEW_SUSPEND_ROOT", root).env("FLEA_T1_REVIEW_SUSPEND_MODE", child_mode).env_remove("TMUX")
            .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave.try_clone().unwrap());
        // The PTY is the child's controlling terminal, as a shell's is, so the editor handoff can move its foreground.
        unsafe { command.pre_exec(|| {
            if setsid() < 0 || ioctl(0, TIOCSCTTY, 0) < 0 { return Err(io::Error::last_os_error()); }
            Ok(())
        }); }
        if let Some(call) = gate {
            command.env("PATH", format!("{}:/usr/bin:/bin", root.display())).env("FLEA_T1_INTERRUPT_CALL", call);
        }
        let child = command.spawn().unwrap();
        Self { master, slave, saved, child, transcript: Vec::new(), reaped: false }
    }

    pub fn signal(&self, sig: i32) {
        crate::procsignal::signal_group(self.child.id() as i32, sig).unwrap();
    }

    pub fn read_until(&mut self, marker: &[u8]) {
        let deadline = Instant::now() + LIMIT;
        while !self.transcript.windows(marker.len()).any(|bytes| bytes == marker) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "PTY event {:?} timed out: {}", String::from_utf8_lossy(marker), String::from_utf8_lossy(&self.transcript));
            if wait_input(self.master.as_raw_fd(), -1, left.as_millis().min(i32::MAX as u128) as i32).unwrap() {
                let mut bytes = [0; 4096];
                let n = self.master.read(&mut bytes).unwrap();
                assert_ne!(n, 0, "PTY closed before event {:?}", String::from_utf8_lossy(marker));
                self.transcript.extend_from_slice(&bytes[..n]);
            }
        }
    }

    pub fn wait_stopped(&mut self) -> bool {
        self.wait_event(UNTRACED).stopped_signal() == Some(SIGSTOP)
    }

    fn wait_event(&mut self, options: i32) -> std::process::ExitStatus {
        let pid = self.child.id() as i32;
        let (send, receive) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let result = loop {
                let mut status = 0;
                if unsafe { waitpid(pid, &mut status, options) } >= 0 {
                    break Ok(std::process::ExitStatus::from_raw(status));
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted { break Err(error); }
            };
            let _ = send.send(result);
        });
        let event = receive.recv_timeout(LIMIT).ok();
        let timed_out = event.is_none();
        if timed_out { self.signal(SIGKILL); }
        // A killed child is still reaped here, so Drop never signals a group number the kernel may have reused.
        let status = event.unwrap_or_else(|| receive.recv().unwrap()).unwrap();
        waiter.join().unwrap();
        self.reaped = status.stopped_signal().is_none();
        assert!(!timed_out, "child wait event timed out: {}", String::from_utf8_lossy(&self.transcript));
        status
    }

    pub fn finish(&mut self) -> std::process::ExitStatus {
        let status = self.wait_event(0);
        while wait_input(self.master.as_raw_fd(), -1, 0).unwrap() {
            let mut bytes = [0; 4096];
            match self.master.read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(n) => self.transcript.extend_from_slice(&bytes[..n]),
            }
        }
        status
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.reaped && !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = crate::procsignal::signal_group(self.child.id() as i32, SIGKILL);
            let _ = self.child.wait();
        }
    }
}

pub(super) fn mode(slave: &std::fs::File) -> Vec<u8> {
    let output = Command::new("/usr/bin/stty").arg("-g").stdin(slave.try_clone().unwrap()).output().unwrap();
    assert!(output.status.success());
    output.stdout
}
