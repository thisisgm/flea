use super::*;

#[test]
fn the_status_parser_names_only_the_sandbox_pid() {
    assert_eq!(parse_child_pid(b"{\"child-pid\": 1234}\n"), Some(1234));
    assert_eq!(parse_child_pid(b"noise {\"child-pid\":42} tail"), Some(42));
    assert_eq!(parse_child_pid(b"no key here"), None);
    assert_eq!(parse_child_pid(b"{\"child-pid\": 0}"), None);
    assert_eq!(parse_child_pid(b"{\"child-pid\":}"), None);
}

// Sample /proc/<pid>/stat rows: the comm may hold spaces and parens, so the ppid follows the last ")".
#[test]
fn the_stat_parser_reads_the_ppid_past_any_comm() {
    assert_eq!(ppid_of_stat("123 (bwrap) S 456 789"), Some(456));
    assert_eq!(ppid_of_stat("123 (my tool) S 456 789"), Some(456));
    assert_eq!(ppid_of_stat("123 (we)ird) S 456 789"), Some(456));
    assert_eq!(ppid_of_stat("no parens at all"), None);
    assert_eq!(ppid_of_stat("123 (bwrap)"), None);
}

// Cancels immediately after spawn in a loop; on the base this waits out the sleep and leaves it running.
#[test]
fn an_immediate_cancel_ends_every_process_in_the_jail_promptly() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let token = format!("30.{}", std::process::id());
    for _ in 0..200 {
        let inner = vec!["/usr/bin/sleep".to_string(), token.clone()];
        let mut full = crate::backend::sandbox::wrap_readonly(&inner, std::path::Path::new("/usr/bin/sleep"));
        crate::backend::sandbox::add_status(&mut full, inner.len());
        let began = std::time::Instant::now();
        let mut jailed = spawn_jailed(&full, |cmd| {
            cmd.stdin(std::process::Stdio::null());
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
        })
        .expect("jailed sleep did not start");
        kill_tree(&mut jailed);
        assert!(began.elapsed() < std::time::Duration::from_secs(WAIT_GONE_SECS), "a cancelled jail was waited out");
    }
    assert!(!proc_with_token(&token), "a jailed sleep outlived its cancel");
}

// The status-fd branch is the point: with the sandbox pid learned before the cancel, only killing that pid ends the jail promptly.
#[test]
fn a_learned_sandbox_pid_is_killed_not_waited_out() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let token = format!("31.{}", std::process::id());
    let inner = vec!["/usr/bin/sleep".to_string(), token.clone()];
    let mut full = crate::backend::sandbox::wrap_readonly(&inner, std::path::Path::new("/usr/bin/sleep"));
    crate::backend::sandbox::add_status(&mut full, inner.len());
    let mut jailed = spawn_jailed(&full, |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    })
    .expect("jailed sleep did not start");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while jailed.sandbox_pid.load(Ordering::SeqCst) <= 0 && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    let learned = jailed.sandbox_pid.load(Ordering::SeqCst);
    assert!(learned > 0, "the sandbox pid was never learned, so this pins nothing");
    let began = std::time::Instant::now();
    kill_tree(&mut jailed);
    assert!(began.elapsed() < std::time::Duration::from_secs(WAIT_GONE_SECS), "a cancel with a learned pid was waited out");
    assert!(!proc_with_token(&token), "the learned sandbox process outlived its cancel");
}

// A child spawned while a jail is alive must not see the jail's status pipe: the read end stays open here until the jail ends, so without CLOEXEC every later spawn inherits it.
#[test]
fn a_concurrent_spawn_does_not_inherit_the_status_pipe() {
    let before = child_fd_count();
    let mut jailed = spawn_jailed(&["/usr/bin/sleep".to_string(), "30".to_string()], |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    })
    .expect("plain sleep jail did not start");
    let during = child_fd_count();
    kill_tree(&mut jailed);
    assert_eq!(during, before, "a child spawned during a jail inherited the status pipe");
}

// A forged child-pid naming a process this jail did not start must never be signalled.
#[test]
fn a_reported_pid_owned_by_someone_else_is_never_killed() {
    let mut victim = std::process::Command::new("/usr/bin/sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("victim sleep did not start");
    let victim_pid = victim.id() as i32;
    let mut jailed = spawn_jailed(&["/usr/bin/sleep".to_string(), "30".to_string()], |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    })
    .expect("holder jail did not start");
    jailed.sandbox_pid.store(victim_pid, Ordering::SeqCst);
    kill_tree(&mut jailed);
    let alive = victim.try_wait().expect("wait on victim").is_none();
    let _ = victim.kill();
    let _ = victim.wait();
    assert!(alive, "kill_tree signalled a pid its jail never owned");
}

// setsid puts the sleep outside the group like bwrap's --new-session, so only the learned-pid kill ends it; the shell reports its own pid from there, since a /proc scan missed a mid-exec argv under load.
#[test]
fn a_learned_pid_outside_the_group_is_killed_by_its_own_pid() {
    let fd = crate::backend::sandbox::STATUS_FD;
    let script = format!("/usr/bin/setsid /bin/sh -c 'printf \"{{\\\"child-pid\\\": %d}}\" $$ >&{fd}; exec /usr/bin/sleep 32' & wait");
    let mut jailed = spawn_jailed(&["/bin/sh".to_string(), "-c".to_string(), script], |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    })
    .expect("shell jail did not start");
    // The bound only ends a run whose fixture never reports; the pid itself is the signal.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while jailed.sandbox_pid.load(Ordering::SeqCst) <= 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let learned = jailed.sandbox_pid.load(Ordering::SeqCst);
    assert!(learned > 0, "the fixture never reported its pid, so this pins nothing");
    let group = jailed.child.id() as i32;
    let (pgrp, session) = pgrp_session_of_stat(&std::fs::read_to_string(format!("/proc/{learned}/stat")).expect("the reported pid is not alive"))
        .expect("the reported pid has no readable stat");
    assert_ne!(pgrp, group, "the fixture sleep is inside the jail group, so the group kill would pass this vacuously");
    assert_eq!(session, learned, "the fixture sleep did not leave the session");
    kill_tree(&mut jailed);
    assert!(gone(learned), "the setsid sleep outlived a kill_tree that learned its pid");
}

// A jail whose argv never writes the status fd still returns within the bounded wait.
#[test]
fn a_jail_that_never_writes_the_status_fd_still_returns_promptly() {
    let mut jailed = spawn_jailed(&["/usr/bin/sleep".to_string(), "30".to_string()], |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
    })
    .expect("plain sleep jail did not start");
    let began = std::time::Instant::now();
    kill_tree(&mut jailed);
    assert!(
        began.elapsed() < std::time::Duration::from_millis(STATUS_WAIT_MS) + std::time::Duration::from_secs(1),
        "a jail with no status writer hung past the bounded wait"
    );
}

// A reap the kernel holds, as an uninterruptible sleep does, must not hold kill_tree: it returns within its bound, and the reap thread takes the child once the hold lets go.
#[test]
fn a_child_the_kernel_holds_does_not_hold_kill_tree() {
    let child = std::process::Command::new("/usr/bin/sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("sleep did not start");
    let pid = child.id() as i32;
    let proc_entry = format!("/proc/{pid}");
    crate::backend::reaphold::hold(pid);
    let mut jailed = Jailed { child, sandbox_pid: Arc::new(AtomicI32::new(GONE)) };
    let (done, returned) = std::sync::mpsc::channel();
    let helper = std::thread::spawn(move || {
        kill_tree(&mut jailed);
        let _ = done.send(());
    });
    // The bound is the 200 ms status wait plus the 500 ms reap wait; 2 s leaves the margin a loaded box needs.
    let outcome = returned.recv_timeout(std::time::Duration::from_secs(2));
    let unreaped_while_held = std::path::Path::new(&proc_entry).exists();
    crate::backend::reaphold::release(pid);
    let _ = helper.join();
    assert!(outcome.is_ok(), "kill_tree was still waiting on a child the kernel held after 2 s");
    assert!(unreaped_while_held, "the held child was reaped before its hold let go");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::path::Path::new(&proc_entry).exists() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!std::path::Path::new(&proc_entry).exists(), "the reap thread did not reap the child after its hold let go");
}

// Sample /proc/self/fd listing: one numeric entry per open descriptor, so the count is the inheritance signal.
fn child_fd_count() -> usize {
    let out = std::process::Command::new("/usr/bin/ls")
        .arg("/proc/self/fd")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .expect("ls /proc/self/fd did not run");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).lines().count()
}

// Sample /proc/<pid>/stat, comm holds a space and a ")" so the split is at the last ")": 4242 (my sl)eep) S 4200 4242 4242 0 -1 4194560 120 0 0 0 0 0 0 0 20 0 1 0 98765 5242880 90 18446744073709551615 (after it: state, ppid, pgrp, session).
fn pgrp_session_of_stat(text: &str) -> Option<(i32, i32)> {
    let mut fields = text.rsplit_once(')')?.1.split_whitespace();
    fields.next()?;
    fields.next()?;
    let pgrp = fields.next()?.parse::<i32>().ok()?;
    let session = fields.next()?.parse::<i32>().ok()?;
    Some((pgrp, session))
}

// Sample /proc/<pid>/stat, split at the last ")" as above: 4242 (my sl)eep) Z 4200 4242 4242 0 -1 4194560 120 0 0 0 0 0 0 0 20 0 1 0 98765 5242880 90 18446744073709551615 (first field after it is the state; a killed process is gone once /proc forgets it or it is a zombie).
fn gone(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next().map(|state| state == "Z")).unwrap_or(false),
        Err(_) => true,
    }
}

#[test]
fn a_raw_named_argument_reaches_the_child_as_its_own_bytes() {
    use flea::bytename::ExactText as _;
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    let root = crate::backend::testdir::TestDir::new("jail-raw-arg");
    let raw = root
        .path()
        .join(std::ffi::OsStr::from_bytes(b"part-\xff.txt"));
    std::fs::write(&raw, b"raw").unwrap();
    std::fs::write(root.path().join("part-\u{fffd}.txt"), b"twin").unwrap();
    // Production argv starts with prlimit, which takes the nested branch; a bare program takes the other.
    for wrapper in [
        vec![],
        vec![
            crate::backend::sandbox::PRLIMIT.to_string(),
            "--cpu=5".to_string(),
        ],
    ] {
        let full: Vec<String> = wrapper
            .into_iter()
            .chain([
                "/usr/bin/cat".to_string(),
                "--".to_string(),
                raw.exact_text().into_owned(),
            ])
            .collect();
        let mut jailed = spawn_jailed(&full, |cmd| {
            cmd.stdin(std::process::Stdio::null());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::null());
        })
        .expect("plain cat jail did not start");
        let mut seen = String::new();
        jailed
            .child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut seen)
            .unwrap();
        assert!(jailed.child.wait().unwrap().success(), "{full:?}");
        assert_eq!(seen, "raw", "{full:?}");
    }
}

// Sample /proc/<pid>/cmdline: NUL-separated argv, so the token matches exactly one argument.
fn proc_with_token(token: &str) -> bool {
    let procs = match std::fs::read_dir("/proc") {
        Ok(p) => p,
        Err(_) => return false,
    };
    procs.flatten().any(|p| match std::fs::read(p.path().join("cmdline")) {
        Ok(c) => c.split(|b| *b == 0).any(|arg| arg == token.as_bytes()),
        Err(_) => false,
    })
}
