use super::*;
use crate::backend::testdir::TestDir;
use std::os::fd::AsRawFd;

const TIOCSCTTY: usize = 0x540e;

#[test]
fn adv_d_r1_terminal_child() {
    let Some(root) = std::env::var_os("FLEA_T1_ADV_ROOT") else { return; };
    let root = std::path::PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let mode = std::env::var("FLEA_T1_ADV_MODE").unwrap();
    if mode == "startup" {
        extern "C" { fn raise(sig: i32) -> i32; }
        let terminal = Terminal::enter_observed(&root, |sig| {
            if sig == SIGTERM { assert_eq!(unsafe { raise(SIGTERM) }, 0); }
        }).unwrap();
        assert!(terminal.stopped(), "a stop delivered after handler installation was cleared");
        return;
    }
    let prefixes = [b"\x1b_Gi=31;O".as_slice(), b"\x1bP1+r524", b"\x1b[?62;", b"\x1b[200~old", b"\xc3"];
    let oversized = format!("\x1b[?62;4;{}", "0;".repeat(80));
    for prefix in prefixes.into_iter().chain([oversized.as_bytes()]) {
        let mut terminal = Terminal::enter_at(&root).unwrap();
        assert!(terminal.decode(prefix, false).unwrap().is_empty());
        if mode == "editor" {
            terminal.run_editor(root.join("editor").to_str().unwrap(), &root.join("unused")).unwrap();
        } else {
            crate::procsignal::signal_process(std::process::id() as i32, SIGTSTP).unwrap();
            while !terminal.take_resumed() { terminal.read(-1, 20).unwrap(); }
        }
        assert_eq!(terminal.decode(b"j", false).unwrap(), [crate::tui::input::Key::character('j', "")], "stale decoder state survived {mode}: {prefix:?}");
        terminal.decode(b"\x1b[?0u", false).unwrap();
        assert_eq!(terminal.decoder.negotiation.detected.keyboard, super::super::capabilities::Keyboard::Kitty);
    }
}

fn lifecycle(mode: &str) {
    #[link(name = "util")]
    extern "C" {
        fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
        fn setsid() -> i32;
    }
    let dir = TestDir::new("adv-terminal");
    dir.script("editor", "#!/bin/sh\nexit 0\n");
    let mut master = -1;
    let mut slave = -1;
    let size = WindowSize { rows: 31, columns: 130, x: 1040, y: 682 };
    assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    let saved = Command::new("stty").arg("-g").stdin(slave.try_clone().unwrap()).output().unwrap();
    assert!(saved.status.success());
    let mut command = pty_test_command();
    command.args(["--exact", "tui::terminal::adv_tests::adv_d_r1_terminal_child", "--nocapture"])
        .env("FLEA_T1_ADV_ROOT", dir.path()).env("FLEA_T1_ADV_MODE", mode).env_remove("TMUX")
        .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave.try_clone().unwrap());
    unsafe { command.pre_exec(|| {
        if setsid() < 0 || ioctl(0, TIOCSCTTY, 0) < 0 { return Err(io::Error::last_os_error()); }
        Ok(())
    }); }
    let mut child = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut transcript = Vec::new();
    let mut stops = 0;
    let status = loop {
        if std::time::Instant::now() >= deadline {
            let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
            let _ = child.wait();
            panic!("PTY {mode} timed out: {}", String::from_utf8_lossy(&transcript));
        }
        if wait_input(master.as_raw_fd(), -1, 10).unwrap() {
            let mut bytes = [0; 4096];
            if let Ok(n) = master.read(&mut bytes) { transcript.extend_from_slice(&bytes[..n]); }
        }
        let state = std::fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap_or_default();
        if state.lines().any(|line| line.starts_with("State:") && line.contains("T (stopped)")) {
            assert!(mode == "resume");
            crate::procsignal::signal_group(child.id() as i32, SIGCONT).unwrap();
            stops += 1;
        }
        if let Some(status) = child.try_wait().unwrap() { break status; }
    };
    assert!(status.success(), "PTY {mode}: {}", String::from_utf8_lossy(&transcript));
    let restored = Command::new("stty").arg("-g").stdin(slave).output().unwrap();
    assert!(restored.status.success());
    assert_eq!(restored.stdout, saved.stdout);
    if mode == "resume" { assert!(stops >= 6); }
    println!("ADV {mode}: stop preservation, fresh decoder and restored termios verified");
}

#[test]
fn adv_d_r1_startup_preserves_a_delivered_stop() { lifecycle("startup"); }

#[test]
fn adv_d_r1_editor_return_clears_every_partial_decoder_state() { lifecycle("editor"); }

#[test]
fn adv_d_r1_sigcont_clears_every_partial_decoder_state() { lifecycle("resume"); }
