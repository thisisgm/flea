//! Test-only host for the production window and IPC server; stdin carries input, never IPC mutations.
#[path = "uish_driver/host.rs"]
mod host;

use flea::jsondoc::{self, Json};
use host::{number, object, text, Host};
use std::{
    io::{self, BufRead, Read, Write},
    path::Path,
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LIMIT: u64 = 65_536;

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
        assert!(
            idle.elapsed() < Duration::from_secs(30),
            "control idle timeout"
        );
        assert!(
            started.elapsed() < Duration::from_secs(300),
            "bounded replay timeout"
        );
        host.pump();
        thread::sleep(Duration::from_millis(2));
    }
    let identities: Vec<_> = host.windows.keys().cloned().collect();
    for identity in identities {
        host.command(&object([("op", text("close")), ("window", text(identity))]))
            .unwrap();
    }
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
        reply.code, 3,
        "a constant Surface must fail the mounted row geometry precondition"
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
