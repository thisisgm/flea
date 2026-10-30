//! Test-only host for the production window and IPC server; stdin carries input, never IPC mutations.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
#[path = "uish_driver/host.rs"]
mod host;
#[path = "uish_driver/input.rs"]
mod input;
#[path = "uish_driver/wire.rs"]
mod wire;

use flea::jsondoc::{self, Json};
use host::Host;
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use wire::{number, object, text, IDLE_LIMIT, LIMIT, REPLAY_LIMIT, STEP};

fn emit(sequence: &Json, result: Result<Json, String>, started: Instant) {
    let result = match result {
        Ok(value) => object([("ok", Json::Bool(true)), ("value", value)]),
        Err(error) => object([("ok", Json::Bool(false)), ("error", text(error))]),
    };
    let envelope = object([
        ("sequence", sequence.clone()),
        ("result", result),
        ("monotonic_ns", number(started.elapsed().as_nanos())),
        (
            "unix_ns",
            number(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
            ),
        ),
    ]);
    println!("UISH\t{}", jsondoc::render(&envelope).replace('\n', ""));
    io::stdout().flush().unwrap();
}

#[test]
#[ignore = "test-only stdin replay host, launched by the focused controls"]
fn uish_driver_replay() {
    let mut host = Host::new();
    let (send, receive) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut input = io::stdin().lock();
        loop {
            let mut bytes = Vec::new();
            let result = input.by_ref().take(LIMIT + 1).read_until(b'\n', &mut bytes);
            if matches!(result, Ok(0) | Err(_)) {
                break;
            }
            if bytes.len() as u64 > LIMIT {
                let _ = send.send(Err("oversized control command".to_owned()));
                break;
            }
            let command = String::from_utf8(bytes)
                .map_err(|e| e.to_string())
                .and_then(|line| jsondoc::parse(&line));
            if send.send(command).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut idle = Instant::now();
    emit(
        &Json::Null,
        Ok(object([("protocol", text("flea-uish-headless-v1"))])),
        started,
    );
    loop {
        match receive.try_recv() {
            Ok(command) => {
                idle = Instant::now();
                let sequence = command
                    .as_ref()
                    .ok()
                    .and_then(|c| c.get("sequence"))
                    .cloned()
                    .unwrap_or(Json::Null);
                let result = command.and_then(|c| host.command(&c));
                emit(&sequence, result, started);
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        assert!(idle.elapsed() < IDLE_LIMIT, "control idle timeout");
        assert!(started.elapsed() < REPLAY_LIMIT, "bounded replay timeout");
        host.pump();
        thread::sleep(STEP);
    }
    host.teardown().unwrap();
}

fn controls(case: &str) {
    let worktree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new("python3")
        .arg("-B")
        .arg(worktree.join("tests/uish-headless-controls.py"))
        .args(["--case", case, "--driver"])
        .arg(std::env::current_exe().unwrap())
        .current_dir(worktree)
        .status()
        .expect("headless controls start");
    assert!(status.success(), "mounted controls failed: {status}");
}

#[test]
fn uish_driver_real_window() {
    let constant = flea_gui::ipc::Surface {
        path: "/fixture",
        total: 3,
        cursor: 1,
        selected: &[0, 2],
    };
    let reply = constant
        .dispatch(flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: "rowCentre".into(),
            arguments: vec!["2".into()],
        })
        .reply;
    assert_eq!(
        reply,
        flea::ipc::wire::Reply::error(3, "FLEA_IPC_NOT_PORTED flea.rowCentre"),
        "a constant Surface has no row geometry, so it refuses rowCentre as not ported"
    );
    println!(
        "UISH_CONSTANT_SURFACE_REJECTED rowCentre status={}",
        reply.code
    );
    controls("real-window");
}

#[test]
fn uish_driver_rejects_stale_or_foreign() {
    controls("rejects-stale-or-foreign");
}

// A sandbox root under the host's owned prefix, removed with everything in it when the test ends.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new(name: &str) -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/m9-uish-02");
        fs::create_dir_all(&parent).unwrap();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!("{name}.{}.{stamp}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join(".flea-test-sandbox"), "").unwrap();
        Self(root)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.0.is_absolute() && self.0.join(".flea-test-sandbox").is_file() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn uish_driver_key_close_drops_the_window_and_tears_down() {
    let sandbox = Sandbox::new("key-close");
    let fixture = sandbox.0.join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("a.txt"), "a").unwrap();
    let mut host = Host::at(&sandbox.0);
    let opened = host
        .command(&object([
            ("op", text("open")),
            ("path", text(fixture.display())),
        ]))
        .unwrap();
    let identity = opened
        .get("window")
        .and_then(Json::as_str)
        .unwrap()
        .to_owned();
    let socket = PathBuf::from(opened.get("socket").and_then(Json::as_str).unwrap());
    let closed = host
        .command(&object([
            ("op", text("key")),
            ("key", text("ctrl-q")),
            ("window", text(&identity)),
        ]))
        .unwrap();
    assert_eq!(closed, object([("closed", Json::Bool(true))]));
    assert!(
        host.windows.is_empty(),
        "a window closed by a key leaves no entry"
    );
    assert!(!socket.exists(), "its socket goes with it");
    host.teardown().unwrap();
}

#[test]
fn uish_driver_window_removed_outside_a_command_is_forgotten() {
    let sandbox = Sandbox::new("outside-close");
    let fixture = sandbox.0.join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("a.txt"), "a").unwrap();
    let mut host = Host::at(&sandbox.0);
    let mut open = || {
        let opened = host
            .command(&object([
                ("op", text("open")),
                ("path", text(fixture.display())),
            ]))
            .unwrap();
        let socket = PathBuf::from(opened.get("socket").and_then(Json::as_str).unwrap());
        let identity = opened.get("window").and_then(Json::as_str).unwrap();
        (identity.to_owned(), socket)
    };
    let ((first, first_socket), (second, second_socket)) = (open(), open());
    assert!(first_socket.exists() && second_socket.exists());
    host.remove_outside_command(&first);
    host.remove_outside_command(&second);
    let closed = host
        .command(&object([("op", text("close")), ("window", text(&first))]))
        .unwrap();
    assert_eq!(closed, object([("closed", Json::Bool(true))]));
    let refused = host.command(&object([
        ("op", text("key")),
        ("key", text("down")),
        ("window", text(&second)),
    ]));
    assert_eq!(refused, Err("closed window".to_owned()));
    assert!(
        host.windows.is_empty(),
        "both removed windows are forgotten"
    );
    for socket in [first_socket, second_socket] {
        assert!(!socket.exists(), "{} survived its window", socket.display());
    }
    host.teardown().unwrap();
}
