use super::*;
use crate::backend::testdir::TestDir;
use crate::clip::testutil::{advertise, expect, watchdog, WATCHDOG_MS};
use std::sync::mpsc::{channel, RecvTimeoutError};

extern "C" {
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
}

// A hang bound for a handshake that must arrive, never a duration the code under test is held to.
const ECHILD: i32 = 10;

#[test]
fn an_owner_without_a_clipboard_manager_returns_its_cause() {
    const CHILD: &str = "FLEA_TEST_NO_MANAGER_OWNER";
    if std::env::var_os(CHILD).is_some() {
        std::process::exit(run());
    }
    let dir = TestDir::new("clip-owner-no-manager");
    let socket = dir.join("wayland");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let executable = std::env::current_exe().unwrap();
    let quoted = executable.to_str().unwrap().replace('\'', "'\\''");
    // Keep the readiness pipe open on fd 3 while hiding libtest's stdout; the child's errors stay on stderr.
    let script = dir.script("owner", &format!(
        "#!/bin/sh\n{}=1 WAYLAND_DISPLAY='{}' exec '{}' --exact clip::own::tests::an_owner_without_a_clipboard_manager_returns_its_cause --nocapture 3>&1 >/dev/null\n",
        CHILD, socket.to_str().unwrap().replace('\'', "'\\''"), quoted));
    let compositor = std::thread::spawn(move || -> std::io::Result<()> {
        use crate::clip::protocol::{CALLBACK_DONE, DISPLAY, DISPLAY_GET_REGISTRY, DISPLAY_SYNC, REGISTRY_BIND, SEAT_VERSION};
        let stream = crate::clip::testutil::accept(&listener, watchdog())?;
        let mut conn = Conn::over(std::os::fd::OwnedFd::from(stream));
        let registry = expect(&mut conn, DISPLAY, DISPLAY_GET_REGISTRY);
        let callback = expect(&mut conn, DISPLAY, DISPLAY_SYNC);
        // The fake registry assigns the one advertised seat this global name.
        advertise(&mut conn, registry, &[("wl_seat", SEAT_VERSION)]);
        conn.send(callback, CALLBACK_DONE, &[], &[]).unwrap();
        let bind = conn.next_raw(WATCHDOG_MS).unwrap().expect("the seat bind");
        assert_eq!((bind.sender, bind.opcode), (registry, REGISTRY_BIND));
        Ok(())
    });
    let result = spawn_owner_with(&script, "copy", &["/tmp/a".into()], |rx| rx.recv_timeout(watchdog()));
    let served = compositor.join().unwrap_or_else(|_| panic!("the fake compositor panicked; owner result: {:?}", result));
    assert!(served.is_ok(), "the fake compositor failed: {:?}; owner result: {:?}", served, result);
    assert_eq!(result.unwrap_err(), "this compositor does not offer clipboard access");
}

#[test]
fn an_owner_startup_failure_returns_only_its_first_stderr_line() {
    let dir = TestDir::new("clip-owner-stderr");
    let script = dir.script("owner", "#!/bin/sh\ncat >/dev/null\nprintf 'flea: startup refused\\nsecond diagnostic\\n' >&2\nexit 2\n");
    let result = spawn_owner_with(&script, "copy", &["/tmp/a".into()], |rx| rx.recv_timeout(watchdog()));
    assert_eq!(result.unwrap_err(), "startup refused");
}

// A real child stands in for the owner: it records its pid, drains the payload, then ends or goes silent.
fn failed_child(quiet: bool) {
    let dir = TestDir::new("clip-owner-branches");
    let pidfile = dir.join("pid");
    let ending = if quiet { "printf 'payload-read\\n'\nexec tail -f /dev/null\n" } else { "exit 0\n" };
    let script = dir.script("owner", &format!("#!/bin/sh\necho $$ > '{}'\ncat >/dev/null\n{}", pidfile.display(), ending));
    let failed = spawn_owner_with(&script, "copy", &["/tmp/a".into()], |rx| {
        let read = rx.recv_timeout(watchdog()).unwrap();
        if quiet {
            assert_eq!(read.1.trim_end(), "payload-read", "the child consumed stdin before the ready deadline");
            Err(RecvTimeoutError::Timeout)
        } else {
            assert!(read.0 && read.1.is_empty(), "the real child ended stdout without ready");
            Ok(read)
        }
    });
    assert_eq!(failed.unwrap_err(), "the clipboard owner did not answer");
    let pid: c_int = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    let mut status = 0;
    let result = unsafe { waitpid(pid, &mut status, WNOHANG) };
    let errno = std::io::Error::last_os_error().raw_os_error();
    if result == 0 {
        let _ = crate::procsignal::signal_process(pid, SIGTERM);
        unsafe { waitpid(pid, &mut status, 0); }
    }
    assert_eq!((result, errno), (-1, Some(ECHILD)), "this named child must already be reaped");
}

#[test]
fn an_owner_eof_failure_reaps_its_named_child() { failed_child(false); }

#[test]
fn an_owner_ready_deadline_reaps_its_named_child() { failed_child(true); }

#[test]
fn an_exited_owner_remains_unreaped_until_removed_from_the_map() {
    let token = "exit-race-test";
    let child = std::process::Command::new("/bin/true").spawn().unwrap();
    let pid = child.id();
    remember(token, pid);
    let (arrived, waiting) = channel();
    let (release, proceed) = channel();
    let worker = std::thread::spawn(move || reap_owner(child, token, || {
        arrived.send(()).unwrap();
        proceed.recv_timeout(watchdog()).unwrap();
    }));
    waiting.recv_timeout(watchdog()).unwrap();
    let recorded = owners().lock().unwrap().get(token).copied();
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid));
    // Sample input: 123 (true) Z 456 456, with state immediately after the final closing parenthesis.
    let zombie = stat.as_ref().ok().and_then(|s| s.rsplit_once(") "))
        .map(|(_, rest)| rest.starts_with("Z ")).unwrap_or(false);
    let signalled = withdraw(token);
    let retained = owners().lock().unwrap().get(token).copied();
    let still_unreaped = std::fs::read_to_string(format!("/proc/{}/stat", pid)).is_ok();
    release.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(recorded, Some(pid));
    assert_eq!(retained, Some(pid), "withdraw leaves removal to reap_owner");
    assert!(still_unreaped, "withdraw must not reap the exited owner");
    assert!(zombie, "a mapped exited owner must stay unreaped until the map lock removes it");
    assert!(!signalled, "withdraw must not signal an exited owner while it is mapped");
    assert!(!withdraw(token), "after removal withdraw cannot signal the pid");
}

#[test]
fn withdraw_kills_only_what_this_process_owns() {
    assert!(!withdraw("no-such-token"));
    let child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("a sleeper");
    remember("test-token", child.id());
    let waiter = std::thread::spawn(move || reap_owner(child, "test-token", || {}));
    assert!(withdraw("test-token"));
    waiter.join().unwrap();
    assert!(!withdraw("test-token"), "a reaped owner leaves the map");
}

#[test]
fn a_live_owner_clear_withdraws_without_verifying_the_selection() {
    use crate::clip::testutil::Reaped;
    use std::os::unix::process::ExitStatusExt;
    const TOKEN: &str = "cb1bcb1bcb1bcb1bcb1bcb1bcb1bcb1b";
    const SIGKILL: c_int = 9;
    let mut child = Reaped(Some(std::process::Command::new("/bin/sleep")
        .arg("infinity")
        .spawn()
        .unwrap()));
    let pid = child.0.as_ref().unwrap().id();
    remember(TOKEN, pid);
    let called = std::cell::Cell::new(false);
    let line = crate::backend::clipreq::clear_token_line(TOKEN, |_| {
        called.set(true);
        Ok(false)
    });
    let (done, ended) = channel();
    let waiter = std::thread::spawn(move || {
        wait_for_exit(pid);
        forget(TOKEN, pid);
        let _ = done.send(child.0.as_mut().unwrap().wait());
    });
    let status = ended.recv_timeout(watchdog());
    if status.is_err() {
        // The map lock keeps this pid unreaped while the watchdog signals it.
        let owned = owners().lock().unwrap_or_else(|e| e.into_inner());
        if owned.get(TOKEN) == Some(&pid) {
            let _ = crate::procsignal::signal_process(pid as c_int, SIGKILL);
        }
    }
    waiter.join().unwrap();
    let status = match status {
        Ok(status) => status.unwrap(),
        Err(RecvTimeoutError::Timeout) => panic!("the withdraw signal never ended the owner"),
        Err(RecvTimeoutError::Disconnected) => panic!("the owner waiter ended without an exit status"),
    };
    assert!(!called.get(), "a live owner must be withdrawn without the verified clear");
    assert_eq!(status.signal(), Some(SIGTERM));
    assert_eq!(line, r#"{"t":"clip","op":"clear","ok":true,"cleared":true}"#);
}

#[test]
fn an_exited_owner_clear_uses_the_verified_selection_result() {
    const TOKEN: &str = "cb1ccb1ccb1ccb1ccb1ccb1ccb1ccb1c";
    let mut child = std::process::Command::new("/bin/true").spawn().unwrap();
    let pid = child.id();
    remember(TOKEN, pid);
    wait_for_exit(pid);
    let called = std::cell::Cell::new(false);
    let line = crate::backend::clipreq::clear_token_line(TOKEN, |token| {
        assert_eq!(token, TOKEN);
        called.set(true);
        Ok(true)
    });
    forget(TOKEN, pid);
    assert!(child.wait().unwrap().success());
    assert!(called.get(), "an exited owner must fall through to the verified clear");
    assert_eq!(line, r#"{"t":"clip","op":"clear","ok":true,"cleared":true}"#);
}

#[test]
fn the_content_flag_rides_ahead_of_the_paths_and_is_never_a_path() {
    let token = "ab12cd34ab12cd34ab12cd34ab12cd34";
    let plain = format!("copy\0{}\0/tmp/a\0", token);
    assert_eq!(split_payload(plain.as_bytes()).unwrap(), ("copy".into(), token.into(), vec!["/tmp/a".to_string()], false));
    let flagged = format!("copy\0{}\0{}\0/tmp/a.png\0", token, crate::clip::content::FLAG);
    assert_eq!(split_payload(flagged.as_bytes()).unwrap(), ("copy".into(), token.into(), vec!["/tmp/a.png".to_string()], true));
    let alone = format!("copy\0{}\0{}\0", token, crate::clip::content::FLAG);
    assert!(split_payload(alone.as_bytes()).is_err(), "a flag with no path is still a copy of nothing");
}
