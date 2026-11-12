use super::*;
use crate::ipc::Surface;
use flea::ipc::{
    client::{self, Options},
    endpoint,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command};

fn root() -> PathBuf {
    let output = Command::new("mktemp")
        .arg("-d")
        .arg("/tmp/flea-ipc-server.XXXXXX")
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = fs::canonicalize(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(root.join(".flea-test-sandbox"), "IPC test root\n").unwrap();
    root
}

#[test]
fn short_fixture_roots_fit_real_ipc_endpoints() {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt, net::SocketAddr};
    let root = root();
    let temporary = fs::canonicalize("/tmp").unwrap();
    assert_eq!(root.parent(), Some(temporary.as_path()));
    let metadata = fs::symlink_metadata(&root).unwrap();
    assert_eq!(metadata.uid(), endpoint::uid());
    assert_eq!(metadata.permissions().mode() & 0o7777, 0o700);
    assert!(root.join(".flea-test-sandbox").is_file());
    let nested = root.join("r");
    fs::create_dir(&nested).unwrap();
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        nested.join(".flea-test-sandbox"),
        "private IPC fixture runtime\n",
    )
    .unwrap();
    for runtime in [&root, &nested] {
        let server = Server::bind(runtime).unwrap();
        let path = server.path();
        let expected = endpoint::directory(runtime)
            .unwrap()
            .join(format!("{}.sock", std::process::id()));
        assert_eq!(path, expected.as_path());
        let address = SocketAddr::from_pathname(path).unwrap();
        assert_eq!(address.as_pathname(), Some(path));
        println!(
            "IPC_SHORT_FIXTURE root={} endpoint={} bytes={}",
            root.display(),
            path.display(),
            path.as_os_str().as_bytes().len()
        );
    }
}

fn request(function: &str) -> Request {
    Request::Call {
        target: "flea".into(),
        function: function.into(),
        arguments: vec![],
    }
}

#[test]
fn ipc_errors_use_the_selected_catalog_and_keep_the_wire_status() {
    use flea::i18n::Paths;
    use std::io::Write;
    let root = root();
    fs::create_dir_all(root.join("catalogs/de")).unwrap();
    fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    fs::write(root.join("language"), "de\n").unwrap();
    fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{}}"#,
    )
    .unwrap();
    fs::write(
        root.join("catalogs/de/flea.json"),
        r#"{"ipc.invalid_request":"Ungueltige IPC-Anfrage: %1","ipc.window_busy":"Fenster belegt"}"#,
    ).unwrap();
    let i18n = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalogs")],
    });
    assert_eq!(i18n.language(), "de");
    for (line, code, message) in [
        ("{}\n".to_owned(), 2, "Ungueltige IPC-Anfrage: "),
        (request("path").encode(), 255, "Fenster belegt\n"),
    ] {
        let (mut client, server) = UnixStream::pair().unwrap();
        let (requests, events) = async_channel::bounded(1);
        drop(events);
        client.write_all(line.as_bytes()).unwrap();
        serve(server, &requests, &i18n).unwrap();
        let line = wire::read_line(&mut io::BufReader::new(client), wire::REPLY_LIMIT).unwrap();
        let reply = Reply::decode(&line).unwrap();
        assert_eq!(reply.code, code);
        assert!(reply.stdout.is_empty());
        assert!(reply.stderr.starts_with(message), "{}", reply.stderr);
    }
}

fn fake(event: Event, path: &str) -> bool {
    match event {
        Event::Call { request, answer } => {
            let surface = Surface {
                path,
                total: 3,
                cursor: 2,
                selected: &[0, 2],
            };
            let _ = answer.try_send(surface.dispatch(request));
            false
        }
        Event::Quit => true,
    }
}

#[test]
fn idle_partial_disconnected_and_oversized_clients_do_not_hold_other_reads() {
    use std::io::Write;
    let root = root();
    let server = Server::bind(&root).unwrap();
    let path = server.path().to_owned();
    let events = server.events.clone();
    let model = thread::spawn(move || {
        while let Ok(event) = events.recv_blocking() {
            if fake(event, "/files") {
                break;
            }
        }
    });
    let _idle = UnixStream::connect(&path).unwrap();
    let mut partial = UnixStream::connect(&path).unwrap();
    partial.write_all(b"{\"call\":[").unwrap();
    let mut disconnected = UnixStream::connect(&path).unwrap();
    disconnected
        .write_all(request("path").encode().as_bytes())
        .unwrap();
    drop(disconnected);
    let mut oversized = UnixStream::connect(&path).unwrap();
    oversized
        .write_all(&vec![b'x'; wire::REQUEST_LIMIT + 1])
        .unwrap();
    let mut malformed = UnixStream::connect(&path).unwrap();
    malformed.write_all(b"{}\n").unwrap();
    let line = wire::read_line(&mut io::BufReader::new(malformed), wire::REPLY_LIMIT).unwrap();
    assert_eq!(Reply::decode(&line).unwrap().code, 2);
    let started = Instant::now();
    assert_eq!(
        client::exchange(&path, &request("path")).unwrap(),
        Reply::value("/files")
    );
    assert!(
        started.elapsed() < CLIENT_TIMEOUT,
        "a stalled client serialized the IPC server"
    );
    assert_eq!(
        client::exchange(&path, &request("palette")).unwrap(),
        Reply::error(3, "FLEA_IPC_NOT_PORTED flea.palette")
    );
    assert_eq!(client::exchange(&path, &request("quit")).unwrap().code, 0);
    model.join().unwrap();
    drop(server);
    assert!(!path.exists());
}

#[test]
fn disconnected_mid_reply_cannot_stop_the_listener() {
    use std::io::Write;
    let root = root();
    let server = Server::bind(&root).unwrap();
    let events = server.events.clone();
    let model = thread::spawn(move || {
        let Event::Call { answer, .. } = events.recv_blocking().unwrap() else {
            panic!("expected call")
        };
        answer
            .send(Answer {
                reply: Reply::value("x".repeat(4 * 1024 * 1024)),
                quit: false,
            })
            .unwrap();
        while let Ok(event) = events.recv_blocking() {
            if fake(event, "/survived") {
                break;
            }
        }
    });
    let mut socket = UnixStream::connect(server.path()).unwrap();
    socket
        .write_all(request("path").encode().as_bytes())
        .unwrap();
    let mut prefix = [0; 128];
    socket.read_exact(&mut prefix).unwrap();
    drop(socket);
    assert_eq!(
        client::exchange(server.path(), &request("path")).unwrap(),
        Reply::value("/survived")
    );
    assert_eq!(
        client::exchange(server.path(), &request("quit"))
            .unwrap()
            .code,
        0
    );
    model.join().unwrap();
}

struct Window(Child);

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn window_child() {
    let Some(runtime) = std::env::var_os("FLEA_IPC_TEST_RUNTIME") else {
        return;
    };
    let runtime = PathBuf::from(runtime);
    assert!(runtime.is_absolute() && runtime.join(".flea-test-sandbox").is_file());
    let server = Server::bind(&runtime).unwrap();
    let path = std::env::var("FLEA_IPC_TEST_PATH").unwrap();
    while let Ok(event) = server.events.recv_blocking() {
        if fake(event, &path) {
            break;
        }
    }
}

#[test]
fn pid_routes_two_window_processes_and_missing_pid_never_falls_back() {
    let root = root();
    let start = |path: &str| {
        Window(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "ipc::server::tests::window_child", "--nocapture"])
                .env("FLEA_IPC_TEST_RUNTIME", &root)
                .env("FLEA_IPC_TEST_PATH", path)
                .spawn()
                .unwrap(),
        )
    };
    let first = start("/first");
    let second = start("/second");
    let deadline = Instant::now() + Duration::from_secs(5);
    while endpoint::instances(&root).unwrap().len() < 2 {
        assert!(
            Instant::now() < deadline,
            "both windows must publish an endpoint"
        );
        thread::sleep(Duration::from_millis(10));
    }
    for (pid, path) in [(first.0.id(), "/first"), (second.0.id(), "/second")] {
        let options = Options {
            pid: Some(pid),
            request: request("path"),
        };
        assert_eq!(client::execute(&root, &options), Reply::value(path));
        let options = Options {
            pid: Some(pid),
            request: request("quit"),
        };
        assert_eq!(client::execute(&root, &options).code, 0);
    }
    assert_eq!(
        client::execute(
            &root,
            &Options {
                pid: Some(i32::MAX as u32),
                request: request("path")
            }
        )
        .code,
        255
    );
}

#[test]
fn headless_cli_and_harness_contract() {
    let exe = std::env::current_exe().unwrap();
    let backend = crate::cargo_artifacts::binary("flea");
    let exe = exe.to_str().unwrap();
    assert!(!exe.contains('\''), "the fixture script cannot quote {exe}");
    let fixture = root().join("ipc-fixture");
    let script = format!(
        "#!/bin/sh\nFLEA_GUI_IPC_FIXTURE_PATH=$1\nexport FLEA_GUI_IPC_FIXTURE_PATH\nexec '{exe}' --exact ipc::server::fixture_child::ipc_fixture_child --nocapture --test-threads=1\n"
    );
    // Written by a child, so no sibling test's fork holds it open for write when the harness runs it.
    let written = Command::new("/bin/sh")
        .args([
            "-c",
            "printf '%s' \"$2\" > \"$1\" && chmod 700 \"$1\"",
            "sh",
        ])
        .arg(&fixture)
        .arg(script)
        .status()
        .unwrap();
    assert!(written.success(), "the fixture script was not written");
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new("bash")
        .arg(repo.join("tests/gui-ipc.sh"))
        .env("FLEA_BIN", backend)
        .env("FLEA_GUI_IPC_FIXTURE", &fixture)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("failed=0"));
    assert!(result.stdout.ends_with(b"GUI_IPC DONE\n"));
    print!("{}", String::from_utf8_lossy(&result.stdout));
}

#[test]
fn migrated_harness_doubles_keep_their_process_and_timeout_proofs() {
    let root = root();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for arguments in [
        vec!["tests/ui-process-ownership.py"],
        vec!["tools/flea-field-instant", "selftest"],
    ] {
        let result = Command::new("python3")
            .arg("-B")
            .args(&arguments)
            .current_dir(&repo)
            .env("TMPDIR", &root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{arguments:?}\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

// Review g2-pk1-r2-1: a signal that lands while the server waits under its read timeout is retried, never a dropped client.
#[test]
fn a_signal_during_a_request_read_is_retried() {
    extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
        fn pthread_kill(thread: std::os::unix::thread::RawPthread, signum: i32) -> i32;
    }
    extern "C" fn woken(_: i32) {}
    const SIGUSR1: i32 = 10;
    assert_ne!(
        unsafe { signal(SIGUSR1, woken as *const () as usize) },
        usize::MAX
    );
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let reader = thread::spawn(move || read_request(&mut server, &crate::test_catalogs::english()));
    let thread = std::os::unix::thread::JoinHandleExt::as_pthread_t(&reader);
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(20));
        assert_eq!(unsafe { pthread_kill(thread, SIGUSR1) }, 0);
    }
    let _ = wire::write_line(&mut client, &request("ready").encode());
    let read = reader.join().unwrap();
    assert!(read.is_ok(), "the signal failed the read: {read:?}");
}
