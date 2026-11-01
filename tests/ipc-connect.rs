use flea::ipc::{
    client, endpoint,
    wire::{self, Reply, Request},
};
use std::fs;
use std::io::BufReader;
use std::os::fd::AsRawFd;
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

extern "C" {
    fn listen(fd: i32, backlog: i32) -> i32;
}

// A short root of its own: a socket path over SUN_LEN fails to bind, so it must not depend on where the source lives.
fn root() -> PathBuf {
    let output = Command::new("mktemp")
        .args(["-d", "/tmp/flea-ipc-connect.XXXXXX"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    fs::write(root.join(".flea-test-sandbox"), "IPC connect regression\n").unwrap();
    root
}

fn saturated(root: &Path, pid: u32) -> (UnixListener, Vec<UnixStream>, PathBuf) {
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    assert_eq!(
        fs::metadata(root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let path = endpoint::directory(root)
        .unwrap()
        .join(format!("{pid}.sock"));
    let listener = UnixListener::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    // Linux admits backlog plus one pending connections, so both slots must be filled.
    assert_eq!(unsafe { listen(listener.as_raw_fd(), 1) }, 0);
    let pending = (0..2)
        .map(|_| UnixStream::connect(&path).unwrap())
        .collect();
    (listener, pending, path)
}

struct BoundedChild(Child);

impl BoundedChild {
    fn wait(&mut self, limit: Duration) {
        let deadline = Instant::now() + limit;
        loop {
            if self.0.try_wait().unwrap().is_some() {
                return;
            }
            assert!(Instant::now() < deadline, "IPC connect exceeded {limit:?}");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for BoundedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn startup_probe() {
    let Some(root) = std::env::var_os("FLEA_IPC_CONNECT_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let endpoint = endpoint::Endpoint::bind(&root).unwrap();
    assert!(endpoint.path.exists());
}

#[test]
fn stale_full_backlog_does_not_hold_startup_or_remove_the_socket() {
    let root = root();
    let mut gone = Command::new("true").spawn().unwrap();
    let dead_pid = gone.id();
    gone.wait().unwrap();
    assert!(!endpoint::alive(dead_pid));
    let (_listener, _pending, path) = saturated(&root, dead_pid);
    let mut child = BoundedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "startup_probe", "--nocapture"])
            .env("FLEA_IPC_CONNECT_ROOT", &root)
            .spawn()
            .unwrap(),
    );
    child.wait(Duration::from_secs(3));
    assert!(child.0.wait().unwrap().success());
    assert!(path.exists(), "a backlogged socket is not a proven refusal");
}

#[test]
fn live_full_backlog_exits_255_at_the_connection_deadline() {
    let root = root();
    let pid = std::process::id();
    let (_listener, _pending, _path) = saturated(&root, pid);
    let started = Instant::now();
    let mut child = BoundedChild(
        Command::new(env!("CARGO_BIN_EXE_flea"))
            .args(["ipc", "--pid", &pid.to_string(), "show"])
            .env("XDG_RUNTIME_DIR", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    child.wait(Duration::from_secs(6));
    let elapsed = started.elapsed();
    assert_eq!(child.0.wait().unwrap().code(), Some(255));
    let stderr = std::io::read_to_string(child.0.stderr.take().unwrap()).unwrap();
    assert!(stderr.contains("timed out"), "{stderr}");
    assert!(elapsed >= Duration::from_millis(4500), "{elapsed:?}");
    println!("IPC_CONNECT deadline-ms={}", elapsed.as_millis());
}

#[test]
fn a_backlogged_connect_completes_when_queue_space_returns() {
    let root = root();
    let pid = std::process::id();
    let (listener, _pending, path) = saturated(&root, pid);
    let reader = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        drop(listener.accept().unwrap());
        drop(listener.accept().unwrap());
        let (mut stream, _) = listener.accept().unwrap();
        let request = wire::read_line(
            &mut BufReader::new(stream.try_clone().unwrap()),
            wire::REQUEST_LIMIT,
        )
        .unwrap();
        assert_eq!(Request::decode(&request).unwrap(), Request::Show);
        wire::write_line(&mut stream, &Reply::value("connected").encode()).unwrap();
    });
    assert_eq!(
        client::exchange(&path, &Request::Show).unwrap(),
        Reply::value("connected")
    );
    reader.join().unwrap();
}
