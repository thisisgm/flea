use super::*;
use crate::ipc::wire;
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};

fn assert_bare_probe(listener: &UnixListener) {
    listener.set_nonblocking(true).unwrap();
    let (mut probe, _) = listener.accept().expect("one liveness probe must connect");
    probe
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    probe.read_to_end(&mut bytes).unwrap();
    assert!(bytes.is_empty(), "a probe sends no request bytes");
    assert!(matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
}

fn dropped_reply(listing: bool, read_request: bool, live: bool) -> Reply {
    let root = root();
    let endpoint = endpoint::Endpoint::bind(&root.0).unwrap();
    let listener = endpoint.listener.try_clone().unwrap();
    let (kept, closing) = if live {
        (Some(endpoint), None)
    } else {
        (None, Some(endpoint))
    };
    let window = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        if read_request {
            wire::read_line(&mut std::io::BufReader::new(&mut stream), 4096).unwrap();
        } else {
            // Leave request bytes unread so closing the peer deterministically resets it.
            stream.read_exact(&mut [0]).unwrap();
        }
        drop(listener);
        drop(closing);
        drop(stream);
    });
    let arguments: &[&str] = if listing {
        &["--actions"]
    } else {
        &["--action", "viewGrid"]
    };
    let mut options = parse(&args(arguments)).unwrap();
    options.window = Some(std::process::id());
    let reply = execute(&root.0, &options, Vec::new);
    window.join().unwrap();
    if let Some(endpoint) = kept {
        if reply.code == UNAVAILABLE {
            assert_bare_probe(&endpoint.listener);
        }
    }
    reply
}

#[test]
fn a_window_closing_during_the_request_is_no_window() {
    for listing in [false, true] {
        for read_request in [false, true] {
            assert_eq!(
                dropped_reply(listing, read_request, false),
                Reply::error(
                    NO_WINDOW,
                    "flea: no Flea window is running, so there is nothing to run the action in"
                ),
                "listing={listing}, read_request={read_request}"
            );
        }
    }
}

#[test]
fn a_live_window_dropping_a_client_is_unavailable() {
    let mut failures = Vec::new();
    for listing in [false, true] {
        for read_request in [false, true] {
            let reply = dropped_reply(listing, read_request, true);
            if reply.code != UNAVAILABLE
                || !reply.stdout.is_empty()
                || !reply
                    .stderr
                    .starts_with("flea: the window did not answer: ")
            {
                failures.push((listing, read_request, reply));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "listing, read_request, reply: {failures:?}"
    );
}

fn broken_pipe() -> io::Error {
    let (mut client, peer) = UnixStream::pair().unwrap();
    peer.shutdown(Shutdown::Both).unwrap();
    drop(peer);
    let error = wire::write_line(&mut client, "request\n").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    error
}

#[test]
fn broken_pipe_probes_live_refused_and_missing_listeners() {
    for state in ["missing", "refused", "live"] {
        let root = root();
        let endpoint = endpoint::Endpoint::bind(&root.0).unwrap();
        let path = endpoint.path.clone();
        let kept = if state == "live" {
            Some(endpoint)
        } else {
            drop(endpoint);
            if state == "refused" {
                drop(UnixListener::bind(&path).unwrap());
                std::fs::set_permissions(
                    &path,
                    std::os::unix::fs::PermissionsExt::from_mode(0o600),
                )
                .unwrap();
            }
            None
        };
        let error = broken_pipe();
        let original = error.to_string();
        let reply = exchange_failed(&path, error);
        if let Some(endpoint) = kept {
            assert_eq!(
                reply,
                Reply::error(
                    UNAVAILABLE,
                    format!("flea: the window did not answer: {original}")
                )
            );
            assert_bare_probe(&endpoint.listener);
        } else {
            assert_eq!(
                reply,
                Reply::error(
                    NO_WINDOW,
                    "flea: no Flea window is running, so there is nothing to run the action in"
                ),
                "{state}"
            );
        }
    }
}

// Linux sun_path holds 108 bytes, so a name this long cannot be connected to.
const SUN_PATH_BYTES: usize = 108;

#[test]
fn an_inconclusive_probe_preserves_the_original_error() {
    let root = root();
    let path = root.0.join("x".repeat(SUN_PATH_BYTES));
    let error = broken_pipe();
    let original = error.to_string();
    assert_eq!(
        exchange_failed(&path, error),
        Reply::error(
            UNAVAILABLE,
            format!("flea: the window did not answer: {original}")
        )
    );
}

#[test]
fn malformed_window_replies_are_still_unavailable() {
    let root = root();
    let endpoint = endpoint::Endpoint::bind(&root.0).unwrap();
    let window = std::thread::spawn(move || {
        let (mut stream, _) = endpoint.listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        wire::read_line(
            &mut std::io::BufReader::new(stream.try_clone().unwrap()),
            4096,
        )
        .unwrap();
        wire::write_line(&mut stream, "not a reply\n").unwrap();
    });
    let reply = execute(
        &root.0,
        &parse(&args(&["--action", "viewGrid"])).unwrap(),
        Vec::new,
    );
    window.join().unwrap();
    assert_eq!(reply.code, UNAVAILABLE);
    assert!(reply.stdout.is_empty());
    assert!(reply
        .stderr
        .starts_with("flea: the window did not answer: "));
}
