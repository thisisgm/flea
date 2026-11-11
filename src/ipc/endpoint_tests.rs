use super::*;
use std::os::fd::AsRawFd;
use std::os::unix::fs::symlink;

// A short root of its own: a socket path over SUN_LEN fails to bind, so it must not depend on where the source lives.
fn root() -> PathBuf {
    let output = std::process::Command::new("mktemp")
        .args(["-d", "/tmp/flea-ep.XXXXXX"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    fs::write(root.join(".flea-test-sandbox"), "IPC tests\n").unwrap();
    root
}

#[test]
fn socket_and_directory_are_private_and_symlinks_are_refused() {
    let root = root();
    let endpoint = Endpoint::bind(&root).unwrap();
    assert_eq!(
        fs::metadata(root.join("flea-ipc")).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(&endpoint.path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        instances(&root).unwrap(),
        [(std::process::id(), endpoint.path.clone())]
    );
    assert!(
        Endpoint::bind(&root).is_err(),
        "a live endpoint must never be replaced"
    );
    let path = endpoint.path.clone();
    drop(endpoint);
    assert!(!path.exists());
    let other = root.join("other");
    fs::create_dir(&other).unwrap();
    let linked = root.join("linked");
    fs::create_dir(&linked).unwrap();
    fs::set_permissions(&linked, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&other, linked.join("flea-ipc")).unwrap();
    assert!(directory(&linked).is_err());
    fs::set_permissions(root.join("flea-ipc"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(directory(&root).is_err());
}

#[test]
fn stale_requires_both_a_dead_pid_and_a_refused_connection() {
    let root = root();
    let directory = directory(&root).unwrap();
    // The kernel hands out pids below pid_max only, so pid_max names no process even while other tests spawn.
    let dead: u32 = fs::read_to_string("/proc/sys/kernel/pid_max")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(!alive(dead));
    let stale = directory.join(format!("{dead}.sock"));
    let listener = UnixListener::bind(&stale).unwrap();
    // Keep a descriptor alive like a concurrently forked child before exec.
    let inherited = listener.try_clone().unwrap();
    fs::set_permissions(&stale, fs::Permissions::from_mode(0o600)).unwrap();
    prune(&directory).unwrap();
    assert!(
        stale.exists(),
        "a connectable endpoint is retained even under a dead pid"
    );
    // Shutdown stops listening through every descriptor, including a fork-inherited one.
    assert_eq!(
        unsafe { libc::shutdown(listener.as_raw_fd(), libc::SHUT_RDWR) },
        0
    );
    drop(listener);
    prune(&directory).unwrap();
    assert!(!stale.exists());
    drop(inherited);
    let live = directory.join(format!("{}.sock", std::process::id()));
    let listener = UnixListener::bind(&live).unwrap();
    fs::set_permissions(&live, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    prune(&directory).unwrap();
    assert!(
        live.exists(),
        "a live pid with a refusing socket is retained"
    );
    let file = directory.join(format!("{dead}.sock"));
    fs::write(&file, "not a socket").unwrap();
    prune(&directory).unwrap();
    assert_eq!(fs::read_to_string(file).unwrap(), "not a socket");
}

// Review g2-pk1-r1-4: a chooser's socket has its own role, so default routing never lands on it.
#[test]
fn a_picker_socket_is_listed_apart_and_skipped_by_default_routing() {
    use super::super::client::{execute, Options};
    use super::super::wire::{Reply, Request};
    let root = root();
    let picker = Endpoint::bind_picker(&root).unwrap();
    assert!(picker
        .path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("picker-"));
    assert_eq!(
        instances(&root).unwrap(),
        [],
        "default routing sees no main window"
    );
    assert_eq!(
        pickers(&root).unwrap(),
        [(std::process::id(), picker.path.clone())]
    );
    let window = Endpoint::bind(&root).unwrap();
    assert_eq!(
        instances(&root).unwrap(),
        [(std::process::id(), window.path.clone())]
    );
    drop(window);
    let listener = picker.listener.try_clone().unwrap();
    let served = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(stream.try_clone().unwrap()),
            &mut line,
        )
        .unwrap();
        super::super::wire::write_line(&mut stream, &Reply::value("picked").encode()).unwrap();
    });
    let call = |pid| Options {
        pid,
        request: Request::Call {
            target: "fleapicker".into(),
            function: "ready".into(),
            arguments: Vec::new(),
        },
    };
    assert_eq!(
        execute(&root, &call(None)).code,
        255,
        "no main window, so no default target"
    );
    let reached = execute(&root, &call(Some(std::process::id())));
    assert_eq!(
        reached.stdout.trim_end(),
        "picked",
        "an explicit pid reaches the chooser: {reached:?}"
    );
    served.join().unwrap();
}
