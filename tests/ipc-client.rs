use flea::ipc::{
    endpoint::Endpoint,
    wire::{self, Reply, Request},
};
use std::fs;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn executable_dispatches_ipc_before_display_and_path_parsing() {
    // A short root of its own: a socket path over SUN_LEN fails to bind, so it must not depend on where the source lives.
    let output = Command::new("mktemp")
        .args(["-d", "/tmp/flea-ipc-client.XXXXXX"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    fs::write(root.join(".flea-test-sandbox"), "CLI IPC test root\n").unwrap();
    let endpoint = Endpoint::bind(&root).unwrap();
    let listener = endpoint.listener.try_clone().unwrap();
    let arguments = [" a\nb ", "--version", "--backend"];
    let worker = std::thread::spawn(move || {
        for argument in arguments {
            let (mut stream, _) = listener.accept().unwrap();
            let line = wire::read_line(
                &mut BufReader::new(stream.try_clone().unwrap()),
                wire::REQUEST_LIMIT,
            )
            .unwrap();
            assert_eq!(
                Request::decode(&line).unwrap(),
                Request::Call {
                    target: "flea".into(),
                    function: "path".into(),
                    arguments: vec![argument.into()],
                }
            );
            wire::write_line(&mut stream, &Reply::value("/files/a\nb").encode()).unwrap();
        }
    });
    for argument in arguments {
        let result = Command::new(env!("CARGO_BIN_EXE_flea"))
            .args([
                "ipc",
                "--pid",
                &std::process::id().to_string(),
                "call",
                "flea",
                "path",
                argument,
            ])
            .env("XDG_RUNTIME_DIR", &root)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"/files/a\nb\n");
        assert!(result.stderr.is_empty());
    }
    worker.join().unwrap();
}
