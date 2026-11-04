use super::*;
use std::{os::fd::AsRawFd, time::Duration};

pub(crate) struct Fixture(pub PathBuf, std::fs::File);

impl Fixture {
    pub(crate) fn new() -> Self {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join(".superpowers/m9-corner");
        std::fs::create_dir_all(&parent).unwrap();
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("rounding.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(parent));
        std::fs::write(path.join(".flea-test-sandbox"), "compositor controls\n").unwrap();
        let directory = std::fs::File::open(&path).unwrap();
        Self(path, directory)
    }

    pub(crate) fn socket(&self) -> PathBuf {
        PathBuf::from(format!(
            "/proc/self/fd/{}/.socket2.sock",
            self.1.as_raw_fd()
        ))
    }

    pub(crate) fn command(&self, status: u8) -> PathBuf {
        let path = self.0.join("hyprctl");
        // A child writes the script, since a writable fd here leaks into concurrent forks and exec then fails ETXTBSY.
        let written = Command::new("/bin/sh")
            .args([
                "-c",
                "printf '%s' \"$2\" > \"$1\" && chmod 700 \"$1\"",
                "sh",
            ])
            .arg(&path)
            .arg(format!(
                "#!/bin/sh\ncat \"${{0%/*}}/answer\"\nexit {status}\n"
            ))
            .status()
            .unwrap();
        assert!(written.success());
        path
    }

    pub(crate) fn answer(&self, answer: &str) {
        std::fs::write(self.0.join("answer"), answer).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join(".superpowers/m9-corner");
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(parent)
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn from_command(answer: &str, status: u8) -> f32 {
    let root = Fixture::new();
    root.answer(answer);
    read(&mut Command::new(root.command(status)))
}

#[test]
fn m9_corner_missing_hyprctl_is_square() {
    let root = Fixture::new();
    assert_eq!(read(&mut Command::new(root.0.join("missing-hyprctl"))), 0.);
}

#[test]
fn m9_corner_nonzero_exit_is_square() {
    assert_eq!(from_command(r#"{"int":8}"#, 1), 0.);
}

#[test]
fn m9_corner_garbage_json_is_square() {
    assert_eq!(from_command("not json", 0), 0.);
}

#[test]
fn m9_corner_negative_is_square() {
    assert_eq!(from_command(r#"{"int":-8}"#, 0), 0.);
}

#[test]
fn m9_corner_nan_and_infinite_are_square() {
    for answer in [r#"{"int":NaN}"#, r#"{"int":1e400}"#, r#"{"int":"NaN"}"#] {
        assert_eq!(from_command(answer, 0), 0.);
    }
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        assert_eq!(valid_corner(number), 0.);
    }
}

#[test]
fn m9_corner_unset_is_square() {
    for answer in ["{}", r#"{"int":8,"set":false}"#] {
        assert_eq!(from_command(answer, 0), 0.);
    }
}

#[test]
fn m9_corner_zero_is_square() {
    assert_eq!(from_command(r#"{"int":0,"set":true}"#, 0), 0.);
}

#[test]
fn m9_corner_eight_is_rounded() {
    assert_eq!(from_command(r#"{"int":8,"set":true}"#, 0), 8.);
}

#[test]
fn m9_corner_defaults_are_square() {
    assert_eq!(crate::g2::st1::Facts::default().corner, 0.);
    assert_eq!(crate::g2::st1::seams::COMPOSITOR_CORNER_DEFAULT, 0.);
    assert_eq!(crate::g2::pl4::rows::RowsModel::default().corner, 0.);
}

#[test]
fn m9_corner_socket_loss_returns_to_square_and_drop_closes_reader() {
    use std::{io::Read, os::unix::net::UnixListener, sync::mpsc};
    let root = Fixture::new();
    root.answer(r#"{"int":8}"#);
    let socket = root.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let (send, updates) = mpsc::channel();
    let service = Service::at(Some(socket.clone()), root.command(0), move |value| {
        send.send(value).is_ok()
    })
    .unwrap();
    let (stream, _) = listener.accept().unwrap();
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    drop(stream);
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 0.);
    drop(service);
    let (send, updates) = mpsc::channel();
    let service = Service::at(Some(socket), root.command(0), move |value| {
        send.send(value).is_ok()
    })
    .unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    drop(service);
    assert_eq!(stream.read(&mut [0]).unwrap(), 0);
}

fn accept(listener: &std::os::unix::net::UnixListener) -> UnixStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "subscription must reconnect"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

#[test]
fn m9_corner_non_utf8_event_does_not_lose_reload() {
    use std::{io::Write, os::unix::net::UnixListener, sync::mpsc};
    let root = Fixture::new();
    root.answer(r#"{"int":0}"#);
    let socket = root.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let (send, updates) = mpsc::channel();
    let _service = Service::at(Some(socket), root.command(0), move |value| {
        send.send(value).is_ok()
    })
    .unwrap();
    let mut stream = accept(&listener);
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 0.);
    root.answer(r#"{"int":8}"#);
    stream
        .write_all(b"activewindow>>caf\xe9\nconfigreloaded>>\n")
        .unwrap();
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    assert!(
        updates.try_recv().is_err(),
        "invalid text must not publish a socket-loss default"
    );
}

#[test]
fn m9_corner_missing_and_lost_socket_reconnects_and_rereads() {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        sync::mpsc,
    };
    let root = Fixture::new();
    root.answer(r#"{"int":0}"#);
    let socket = root.socket();
    let (send, updates) = mpsc::channel();
    let service = Service::at(Some(socket.clone()), root.command(0), move |value| {
        send.send(value).is_ok()
    })
    .unwrap();
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 0.);
    root.answer(r#"{"int":8}"#);
    let listener = UnixListener::bind(&socket).unwrap();
    let stream = accept(&listener);
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    drop(stream);
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 0.);
    let mut stream = accept(&listener);
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    // A value other than CORNER_DEFAULT pins the reload branch, not a socket-loss publish.
    root.answer(r#"{"int":4}"#);
    stream.write_all(b"configreloaded>>\n").unwrap();
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 4.);
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    drop(service);
    assert_eq!(stream.read(&mut [0]).unwrap(), 0);
}

#[test]
fn m9_corner_reload_buffered_after_drop_is_ignored() {
    use std::{io::Write, os::unix::net::UnixListener, sync::mpsc};
    let root = Fixture::new();
    root.answer(r#"{"int":8}"#);
    let socket = root.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let (send, updates) = mpsc::channel();
    let (gate, opened) = mpsc::channel::<()>();
    let mut calls = 0;
    let service = Service::at(Some(socket), root.command(0), move |value| {
        calls += 1;
        let sent = send.send(value).is_ok();
        // The first reload holds the reader until the service is dropped.
        if calls == 2 {
            let _ = opened.recv();
        }
        sent
    })
    .unwrap();
    let mut stream = accept(&listener);
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    stream
        .write_all(b"configreloaded>>\nconfigreloaded>>\n")
        .unwrap();
    assert_eq!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), 8.);
    drop(service);
    gate.send(()).unwrap();
    assert_eq!(
        updates.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected),
        "a reload read after Drop must not publish"
    );
}
