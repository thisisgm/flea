use super::*;
use crate::tui::capabilities::KEYBOARD_DISABLE;

// A failed handshake must not leave a stopped child or its terminal helpers behind.
struct PtyChild(Child);

impl Drop for PtyChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = crate::procsignal::signal_group(self.0.id() as i32, SIGCONT);
            let _ = crate::procsignal::signal_group(self.0.id() as i32, SIGKILL);
            let _ = self.0.wait();
        }
    }
}

fn read_output(master: &mut std::fs::File, transcript: &mut Vec<u8>, timeout: i32) -> bool {
    use std::os::fd::AsRawFd;
    if !wait_input(master.as_raw_fd(), -1, timeout).unwrap() { return false; }
    let mut bytes = [0; 4096];
    let n = master.read(&mut bytes).unwrap();
    transcript.extend_from_slice(&bytes[..n]);
    n != 0
}

pub(super) fn check() {
    use std::os::fd::FromRawFd;
    #[link(name = "util")]
    extern "C" {
        fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
    }
    fn mode(slave: &std::fs::File) -> Vec<u8> {
        let output = Command::new("stty").arg("-g").stdin(slave.try_clone().unwrap()).output().unwrap();
        assert!(output.status.success());
        output.stdout
    }
    let dir = TestDir::new("review-suspend");
    let mut master = -1;
    let mut slave = -1;
    let size = WindowSize { rows: 31, columns: 130, x: 1040, y: 682 };
    assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    let saved = mode(&slave);
    let child = pty_test_command()
        .args(["--exact", "tui::terminal::tests::review_suspend_child", "--nocapture"])
        .env("FLEA_T1_REVIEW_SUSPEND_ROOT", dir.path()).env_remove("TMUX")
        .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave.try_clone().unwrap())
        .process_group(0).spawn().unwrap();
    let mut child = PtyChild(child);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut transcript = Vec::new();
    let mut suspended = false;
    let mut suspension_sent = false;
    while std::time::Instant::now() < deadline {
        read_output(&mut master, &mut transcript, 20);
        if !suspension_sent && transcript.windows(12).any(|p| p == b"REVIEW_READY") {
            crate::procsignal::signal_group(child.0.id() as i32, SIGTSTP).unwrap();
            suspension_sent = true;
        }
        let state = std::fs::read_to_string(format!("/proc/{}/status", child.0.id())).unwrap_or_default();
        suspended = state.lines().any(|line| line.starts_with("State:") && line.contains("T (stopped)"));
        // The PTY may deliver the pop after /proc already reports the stop.
        if suspended && transcript.windows(KEYBOARD_DISABLE.len()).any(|p| p == KEYBOARD_DISABLE) { break; }
        assert!(child.0.try_wait().unwrap().is_none(), "child exited before suspension: {}", String::from_utf8_lossy(&transcript));
    }
    while read_output(&mut master, &mut transcript, 0) {}
    let mode_while_stopped = mode(&slave);
    let pops_at_stop = transcript.windows(KEYBOARD_DISABLE.len())
        .filter(|p| *p == KEYBOARD_DISABLE).count();
    crate::procsignal::signal_group(child.0.id() as i32, SIGCONT).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline && !transcript.windows(14).any(|p| p == b"REVIEW_RESUMED") {
        read_output(&mut master, &mut transcript, 20);
        assert!(child.0.try_wait().unwrap().is_none(), "child exited before resume: {}", String::from_utf8_lossy(&transcript));
    }
    // SIGTERM to the group must follow the resume's stty child, or it can kill that child mid-command.
    assert!(transcript.windows(14).any(|p| p == b"REVIEW_RESUMED"), "child did not acknowledge resume: {}", String::from_utf8_lossy(&transcript));
    crate::procsignal::signal_group(child.0.id() as i32, SIGTERM).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status = loop {
        read_output(&mut master, &mut transcript, 20);
        if let Some(status) = child.0.try_wait().unwrap() { break status; }
        assert!(std::time::Instant::now() < deadline, "child did not exit: {}", String::from_utf8_lossy(&transcript));
    };
    while read_output(&mut master, &mut transcript, 0) {}
    assert!(status.success(), "child failed ({status}): {}", String::from_utf8_lossy(&transcript));
    println!("SIGTSTP stopped={suspended}, keyboard pops before stop={pops_at_stop}, raw mode remained={}", mode_while_stopped != saved);
    assert!(suspended, "child did not suspend");
    assert_eq!(pops_at_stop, 1, "keyboard protocol remained enabled while suspended");
    assert_eq!(mode_while_stopped, saved);
    assert_eq!(mode(&slave), saved, "terminal mode remained changed after exit");
}
