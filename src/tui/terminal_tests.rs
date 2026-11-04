use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn the_shared_memory_query_names_a_fresh_private_pixel() {
    let dir = TestDir::new("shm");
    let root = dir.path();
    let query = shm_query(root, "flea-tui-shm-1", 32).expect("query");
    assert_eq!(query, format!("\x1b_Gi=32,s=1,v=1,a=q,t=s,f=24;{}\x1b\\", crate::tui::graphics::base64(b"/flea-tui-shm-1")));
    let made = root.join("flea-tui-shm-1");
    assert_eq!(std::fs::read(&made).unwrap(), [0, 0, 0]);
    assert_eq!(std::fs::metadata(&made).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_planted_link_at_the_name_is_replaced_never_followed() {
    let dir = TestDir::new("shm");
    let root = dir.path();
    let target = root.join("bystander");
    std::fs::write(&target, b"keep").unwrap();
    symlink(&target, root.join("flea-tui-shm-2")).unwrap();
    assert!(shm_query(root, "flea-tui-shm-2", 32).is_some());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");
    assert!(!std::fs::symlink_metadata(root.join("flea-tui-shm-2")).unwrap().file_type().is_symlink());
}

#[test]
fn opening_and_closing_balance_only_the_negotiated_keyboard_stack() {
    use crate::tui::{capabilities::{Negotiation, KEYBOARD_ENABLE, KEYBOARD_DISABLE}, tmux::Transport};
    let dir = TestDir::new("shm");
    let shared = shm_query(dir.path(), "flea-tui-shm-3", 32).unwrap();
    let negotiation = Negotiation::begin(Transport::Direct, (31, 32), true, std::time::Instant::now());
    let bytes = String::from_utf8(opening(&shared, &negotiation)).unwrap();
    assert!(bytes.find("\x1b_Gi=32,").unwrap() < bytes.find("\x1b_Gi=31,").unwrap());
    assert!(bytes.contains("\x1b[?u") && bytes.contains("\x1b[c"));
    assert!(!bytes.as_bytes().windows(KEYBOARD_ENABLE.len()).any(|s| s == KEYBOARD_ENABLE));
    assert!(closing(true).starts_with(KEYBOARD_DISABLE));
    assert_eq!(closing(false), LEAVE);
}

#[test]
fn an_editor_that_exits_nonzero_ends_the_rename_with_its_status() {
    assert!(editor_result(std::process::ExitStatus::from_raw(0)).is_ok());
    // Raw wait status 1 << 8 is exit code 1.
    let failed = editor_result(std::process::ExitStatus::from_raw(1 << 8)).unwrap_err();
    assert_eq!(failed.to_string(), "The editor could not finish (exit status: 1)");
}

#[test]
fn a_terminal_cannot_open_in_a_file() {
    let file = std::env::current_exe().unwrap();
    assert_eq!(launch(&file, false).unwrap_err().to_string(), "That is not a folder, so no terminal was opened");
}

#[test]
fn a_failed_stty_call_says_what_it_was_for() {
    assert!(stty_result(std::process::ExitStatus::from_raw(0), PREPARE_FAILED).is_ok());
    let failed = std::process::ExitStatus::from_raw(1 << 8);
    assert_eq!(stty_result(failed, PREPARE_FAILED).unwrap_err().to_string(), "The terminal could not be prepared");
    assert_eq!(stty_result(failed, RESTORE_FAILED).unwrap_err().to_string(), "The terminal could not be restored");
    assert_eq!(stty_result(failed, PREPARE_FOR_EDITOR_FAILED).unwrap_err().to_string(), "The terminal could not be prepared for the editor");
    assert_eq!(stty_result(failed, RESTORE_AFTER_EDITOR_FAILED).unwrap_err().to_string(), "The terminal could not be restored after the editor");
}

#[test]
fn terminal_session_child() {
    let Some(root) = std::env::var_os("FLEA_T1_TEST_ROOT") else { return; };
    let root = std::path::PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let mut terminal = Terminal::enter_at(&root).unwrap();
    if std::env::var("FLEA_T1_RESIZE_TEST").as_deref() == Ok("1") {
        extern "C" {
            fn pthread_self() -> u64;
            fn pthread_kill(thread: u64, signal: i32) -> i32;
        }
        let main_thread = unsafe { pthread_self() };
        let signaler = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert_eq!(unsafe { pthread_kill(main_thread, SIGWINCH) }, 0);
        });
        let started = std::time::Instant::now();
        assert!(terminal.read(-1, 2000).unwrap().is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        signaler.join().unwrap();
        assert!(!terminal.stopped());
        std::fs::write(root.join("observed"), format!("resize\n{:?}\n", terminal.decoder.negotiation.detected)).unwrap();
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut text = String::new();
    while std::time::Instant::now() < deadline && !terminal.stopped() {
        let bytes = terminal.read(-1, 50).unwrap();
        for key in terminal.decode(&bytes, false).unwrap() { text.push_str(&key.text); }
        if text.ends_with('q') { break; }
    }
    assert_eq!(size(), (11, 6));
    assert_eq!(cell_pixels(), Some((8, 22)));
    let capabilities = terminal.decoder.negotiation.detected;
    std::fs::write(root.join("observed"), format!("{text}\n{capabilities:?}\n")).unwrap();
    assert!(text.ends_with('q') || terminal.stopped());
}

#[test]
fn real_pty_startup_resize_legacy_quit_and_signaled_quit_restore_the_terminal() {
    use std::os::fd::{AsRawFd, FromRawFd};
    #[link(name = "util")]
    extern "C" {
        fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
    }
    fn mode(slave: &std::fs::File) -> Vec<u8> {
        let output = Command::new("stty").arg("-g").stdin(slave.try_clone().unwrap()).output().unwrap();
        assert!(output.status.success());
        output.stdout
    }
    for (name, enhanced, signaled) in [("legacy", false, false), ("kitty", true, false), ("signal", true, true), ("resize", false, false)] {
        let dir = TestDir::new(name);
        let mut master = -1;
        let mut slave = -1;
        let size = WindowSize { rows: 31, columns: 130, x: 1040, y: 682 };
        assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
        let mut master = unsafe { std::fs::File::from_raw_fd(master) };
        let slave = unsafe { std::fs::File::from_raw_fd(slave) };
        let saved = mode(&slave);
        let mut child = pty_test_command()
            .args(["--exact", "tui::terminal::tests::terminal_session_child", "--nocapture"])
            .env("FLEA_T1_TEST_ROOT", dir.path()).env("TERM", "kitty").env_remove("TMUX")
            .env("FLEA_TUI_CAPABILITIES", "1")
            .env("FLEA_T1_RESIZE_TEST", if name == "resize" { "1" } else { "" })
            .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave.try_clone().unwrap())
            .process_group(0).spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut transcript = Vec::new();
        let mut sent = false;
        let mut signaled_once = false;
        let status = loop {
            assert!(std::time::Instant::now() < deadline, "PTY child timed out: {}", String::from_utf8_lossy(&transcript));
            if wait_input(master.as_raw_fd(), -1, 25).unwrap() {
                let mut bytes = [0; 4096];
                if let Ok(n) = master.read(&mut bytes) { transcript.extend_from_slice(&bytes[..n]); }
            }
            if !sent && transcript.windows(4).any(|p| p == b"\x1b[?u") {
                let next = WindowSize { rows: 6, columns: 11, x: 88, y: 132 };
                assert_eq!(unsafe { ioctl(master.as_raw_fd(), 0x5414usize, &next) }, 0);
                if enhanced { master.write_all(b"\x1b[?0uj").unwrap(); }
                if !signaled && name != "resize" { master.write_all(b"q").unwrap(); }
                sent = true;
            }
            if signaled && !signaled_once && transcript.windows(6).any(|p| p == b"\x1b[>29u") {
                crate::procsignal::signal_group(child.id() as i32, SIGTERM).unwrap();
                signaled_once = true;
            }
            if let Some(status) = child.try_wait().unwrap() { break status; }
        };
        drain_exited_pty(&mut master, &mut transcript);
        assert!(status.success(), "PTY child failed: {}", String::from_utf8_lossy(&transcript));
        assert!(String::from_utf8_lossy(&transcript).contains("flea: terminal capabilities Capabilities { graphics: None"));
        assert_eq!(mode(&slave), saved);
        assert!(transcript.windows(LEAVE.len()).any(|p| p == LEAVE));
        assert_eq!(transcript.windows(super::super::capabilities::KEYBOARD_ENABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_ENABLE).count(), usize::from(enhanced));
        assert_eq!(transcript.windows(super::super::capabilities::KEYBOARD_DISABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_DISABLE).count(), usize::from(enhanced));
        let observed = std::fs::read_to_string(dir.join("observed")).unwrap();
        assert!(observed.contains("graphics: None"));
        if enhanced { assert!(observed.contains("keyboard: Kitty")); } else { assert!(!observed.contains("keyboard: Kitty")); }
        println!("PTY {name}: saved={:?} restored={:?} observed={observed}", saved, mode(&slave));
    }
}

fn drain_exited_pty(master: &mut std::fs::File, transcript: &mut Vec<u8>) {
    use std::os::fd::AsRawFd;
    // Child exit can become visible between the last poll and the child's final terminal write.
    while wait_input(master.as_raw_fd(), -1, 0).unwrap() {
        let mut bytes = [0; 4096];
        match master.read(&mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(n) => transcript.extend_from_slice(&bytes[..n]),
        }
    }
}

#[test]
fn an_exited_pty_child_still_has_its_final_terminal_write_to_drain() {
    use std::os::fd::{AsRawFd, FromRawFd};
    #[link(name = "util")]
    extern "C" {
        fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
    }
    let (mut master, mut slave) = (-1, -1);
    assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), std::ptr::null()) }, 0);
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    let mut child = Command::new("/usr/bin/printf").args(["%s", std::str::from_utf8(LEAVE).unwrap()])
        .stdout(slave.try_clone().unwrap()).spawn().unwrap();
    assert!(child.wait().unwrap().success());
    assert!(wait_input(master.as_raw_fd(), -1, 0).unwrap(), "exited child must have unread output");
    let mut transcript = Vec::new();
    drain_exited_pty(&mut master, &mut transcript);
    assert_eq!(transcript, LEAVE);
}


#[path = "terminal_suspend_fixture.rs"]
mod suspension;
include!("terminal_suspend_tests.rs");

#[test]
fn review_positive_lifecycle_child() {
    let Some(root) = std::env::var_os("FLEA_T1_REVIEW_LIFECYCLE_ROOT") else { return; };
    let root = std::path::PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let mode = std::env::var("FLEA_T1_REVIEW_LIFECYCLE_MODE").unwrap();
    let mut terminal = Terminal::enter_at(&root).unwrap();
    if mode == "silent" {
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_millis(650) {
            let wait = terminal.decoder.negotiation.wait_ms(std::time::Instant::now(), 50);
            let bytes = terminal.read(-1, wait).unwrap();
            assert!(terminal.decode(&bytes, true).unwrap().is_empty());
        }
        assert_eq!(terminal.decoder.negotiation.detected.keyboard, super::super::capabilities::Keyboard::Legacy);
        return;
    }
    terminal.decode(b"\x1b[?0u", false).unwrap();
    if mode == "panic" { panic!("controlled review panic"); }
    let result = terminal.run_editor(root.join("editor").to_str().unwrap(), &root.join("editor-mode"));
    if mode == "editor" { assert!(result.is_ok(), "{result:?}"); }
    if mode == "editor-failed" { assert!(result.as_ref().unwrap_err().to_string().contains("exit status: 7")); }
    if mode == "editor-suspended" { assert!(result.as_ref().unwrap_err().to_string().contains("suspended")); }
    assert!(terminal.ready());
    terminal.decode(b"\x1b[?0u", false).unwrap();
}

#[test]
fn review_positive_panic_editor_handoff_and_silent_terminal_cleanup() {
    use std::os::fd::{AsRawFd, FromRawFd};
    #[link(name = "util")]
    extern "C" {
        fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
        fn setsid() -> i32;
    }
    fn mode(slave: &std::fs::File) -> Vec<u8> {
        let output = Command::new("stty").arg("-g").stdin(slave.try_clone().unwrap()).output().unwrap();
        assert!(output.status.success());
        output.stdout
    }
    for name in ["panic", "editor", "editor-failed", "editor-suspended", "silent"] {
        let dir = TestDir::new("review-positive");
        let editor_tail = match name { "editor-failed" => "exit 7", "editor-suspended" => "kill -TSTP $$", _ => "exit 0" };
        dir.script("editor", &format!("#!/bin/sh\nstty -g > \"$1\"\nprintf 'EDITOR_READY\\n'\n{editor_tail}\n"));
        let mut master = -1;
        let mut slave = -1;
        let size = WindowSize { rows: 31, columns: 130, x: 1040, y: 682 };
        assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
        let mut master = unsafe { std::fs::File::from_raw_fd(master) };
        let slave = unsafe { std::fs::File::from_raw_fd(slave) };
        let saved = mode(&slave);
        let mut command = pty_test_command();
        command.args(["--exact", "tui::terminal::tests::review_positive_lifecycle_child", "--nocapture"])
            .env("FLEA_T1_REVIEW_LIFECYCLE_ROOT", dir.path()).env("FLEA_T1_REVIEW_LIFECYCLE_MODE", name).env_remove("TMUX")
            .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave.try_clone().unwrap());
        unsafe { command.pre_exec(|| {
            if setsid() < 0 { return Err(io::Error::last_os_error()); }
            if ioctl(0, 0x540eusize, 0) < 0 { return Err(io::Error::last_os_error()); }
            Ok(())
        }); }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut transcript = Vec::new();
        let status = loop {
            if std::time::Instant::now() >= deadline {
                let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
                let _ = child.wait();
                panic!("PTY lifecycle {name} timed out: {}", String::from_utf8_lossy(&transcript));
            }
            if wait_input(master.as_raw_fd(), -1, 10).unwrap() {
                let mut bytes = [0; 4096];
                if let Ok(n) = master.read(&mut bytes) { transcript.extend_from_slice(&bytes[..n]); }
            }
            if let Some(status) = child.try_wait().unwrap() { break status; }
        };
        while wait_input(master.as_raw_fd(), -1, 20).unwrap() {
            let mut bytes = [0; 4096];
            match master.read(&mut bytes) { Ok(0) | Err(_) => break, Ok(n) => transcript.extend_from_slice(&bytes[..n]) }
        }
        assert_eq!(status.success(), name != "panic", "PTY {name}: {}", String::from_utf8_lossy(&transcript));
        assert_eq!(mode(&slave), saved, "PTY {name} left termios changed");
        let pushes = transcript.windows(super::super::capabilities::KEYBOARD_ENABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_ENABLE).count();
        let pops = transcript.windows(super::super::capabilities::KEYBOARD_DISABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_DISABLE).count();
        let expected = if name == "silent" { 0 } else if name == "panic" { 1 } else { 2 };
        assert_eq!((pushes, pops), (expected, expected), "PTY {name} keyboard stack imbalance: {}", String::from_utf8_lossy(&transcript));
        if name.starts_with("editor") {
            assert_eq!(std::fs::read(dir.join("editor-mode")).unwrap(), saved, "editor inherited raw mode");
            let first_pop = transcript.windows(4).position(|p| p == b"\x1b[<u").unwrap();
            let editor_ready = transcript.windows(12).position(|p| p == b"EDITOR_READY").unwrap();
            assert!(first_pop < editor_ready, "editor inherited enhanced keyboard flags");
        }
        println!("PTY lifecycle {name}: pushes={pushes}, pops={pops}, termios restored=true");
    }
}

#[test]
fn suspension_reenters_with_fresh_queries_and_balances_the_second_keyboard_push() {
    use std::os::fd::{AsRawFd, FromRawFd};
    #[link(name = "util")]
    extern "C" {
        fn openpty(master: *mut i32, slave: *mut i32, name: *mut u8, termios: *const u8, size: *const WindowSize) -> i32;
    }
    let dir = TestDir::new("resume");
    let mut master = -1;
    let mut slave = -1;
    let size = WindowSize { rows: 31, columns: 130, x: 1040, y: 682 };
    assert_eq!(unsafe { openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) }, 0);
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    let saved = Command::new("stty").arg("-g").stdin(slave.try_clone().unwrap()).output().unwrap();
    assert!(saved.status.success());
    let mut child = pty_test_command()
        .args(["--exact", "tui::terminal::tests::review_suspend_child", "--nocapture"])
        .env("FLEA_T1_REVIEW_SUSPEND_ROOT", dir.path()).env_remove("TMUX")
        .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave.try_clone().unwrap())
        .process_group(0).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut transcript = Vec::new();
    let mut suspension_sent = false;
    let mut continued = false;
    let mut answered = false;
    let mut negotiated_again = false;
    while std::time::Instant::now() < deadline {
        if wait_input(master.as_raw_fd(), -1, 20).unwrap() {
            let mut bytes = [0; 4096];
            if let Ok(n) = master.read(&mut bytes) { transcript.extend_from_slice(&bytes[..n]); }
        }
        if !suspension_sent && transcript.windows(12).any(|p| p == b"REVIEW_READY") {
            crate::procsignal::signal_group(child.id() as i32, SIGTSTP).unwrap();
            suspension_sent = true;
        }
        let state = std::fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap_or_default();
        if !continued && state.lines().any(|line| line.starts_with("State:") && line.contains("T (stopped)")) {
            crate::procsignal::signal_group(child.id() as i32, SIGCONT).unwrap();
            continued = true;
        }
        if continued && !answered && transcript.windows(4).filter(|p| *p == b"\x1b[?u").count() == 2 {
            master.write_all(b"\x1b[?0u").unwrap();
            answered = true;
        }
        if transcript.windows(super::super::capabilities::KEYBOARD_ENABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_ENABLE).count() == 2 {
            negotiated_again = true;
            break;
        }
    }
    let _ = crate::procsignal::signal_group(child.id() as i32, SIGCONT);
    crate::procsignal::signal_group(child.id() as i32, SIGTERM).unwrap();
    assert!(child.wait().unwrap().success());
    while wait_input(master.as_raw_fd(), -1, 20).unwrap() {
        let mut bytes = [0; 4096];
        match master.read(&mut bytes) { Ok(0) | Err(_) => break, Ok(n) => transcript.extend_from_slice(&bytes[..n]) }
    }
    assert!(negotiated_again, "no keyboard renegotiation: {}", String::from_utf8_lossy(&transcript));
    assert_eq!(transcript.windows(LEAVE.len()).filter(|p| *p == LEAVE).count(), 2);
    assert_eq!(transcript.windows(super::super::capabilities::KEYBOARD_DISABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_DISABLE).count(), 2);
    let ids: Vec<_> = String::from_utf8_lossy(&transcript).split("\x1b_Gi=").skip(1).map(|s| s.split(',').next().unwrap().parse::<u32>().unwrap()).collect();
    assert_eq!(ids.len(), 4);
    assert_eq!(ids.iter().collect::<std::collections::BTreeSet<_>>().len(), 4);
    let restored = Command::new("stty").arg("-g").stdin(slave).output().unwrap();
    assert!(restored.status.success());
    assert_eq!(restored.stdout, saved.stdout);
    println!("SIGCONT: fresh graphics IDs={ids:?}, keyboard pushes=2, pops=2, termios restored=true");
}
