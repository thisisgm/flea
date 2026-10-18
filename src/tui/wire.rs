use crate::jsondoc::{self, Json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};

pub struct Wire {
    child: Child,
    input: ChildStdin,
    pub events: Receiver<Result<Json, String>>,
    // One byte per backend event, so the main loop wakes for it instead of waiting out its input timeout.
    wake: Option<UnixStream>,
}
// Sample prefetch: dest ".../gvfs-1-2-3.list", path "/run/user/1000/gvfs/...", start_ms 1728000000000.
pub(crate) fn backend_command(exe: &std::path::Path, prefetch: Option<&crate::gvfsprefetch::Prefetch>) -> Command {
    let mut cmd = Command::new(exe);
    cmd.arg("--backend");
    if let Some(found) = prefetch {
        cmd.env(crate::gvfsprefetch::PREFETCH_ENV, &found.dest);
        cmd.env(crate::gvfsprefetch::PATH_ENV, &found.path);
        cmd.env(crate::gvfsprefetch::START_ENV, found.start_ms.to_string());
    }
    cmd
}
impl Wire {
    pub fn start(prefetch: Option<&crate::gvfsprefetch::Prefetch>) -> io::Result<Self> {
        let mut child = backend_command(&std::env::current_exe()?, prefetch)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("backend stdin unavailable"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("backend stdout unavailable"))?;
        let (tx, events) = mpsc::channel();
        let (wake, ring) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        // A full socket already holds a wake the loop has not read, so a refused byte loses nothing.
        ring.set_nonblocking(true)?;
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let result = line
                    .map_err(|e| e.to_string())
                    .and_then(|line| jsondoc::parse(&line));
                if tx.send(result).is_err() {
                    return;
                }
                let _ = (&ring).write(&[1]);
            }
            let _ = tx.send(Err("The file list stopped; restart Flea".into()));
            let _ = (&ring).write(&[1]);
        });
        Ok(Self {
            child,
            input,
            events,
            wake: Some(wake),
        })
    }
    // -1 once the wake is gone, which poll(2) skips.
    pub fn wake_fd(&self) -> RawFd {
        self.wake.as_ref().map_or(-1, |wake| wake.as_raw_fd())
    }
    // Empties the wake socket and drops it once the reader has closed its end, because a closed socket polls ready forever.
    pub fn drain_wake(&mut self) {
        let mut bytes = [0; 64];
        let closed = match &self.wake {
            Some(wake) => {
                let mut wake: &UnixStream = wake;
                loop {
                    match wake.read(&mut bytes) {
                        Ok(0) => break true,
                        Ok(_) => continue,
                        Err(_) => break false,
                    }
                }
            }
            None => false,
        };
        if closed {
            self.wake = None;
        }
    }
    pub fn send(&mut self, fields: Vec<(&str, Json)>) -> io::Result<()> {
        let document = Json::Obj(fields.into_iter().map(|(k, v)| (k.into(), v)).collect());
        let text = jsondoc::render(&document).replace('\n', "");
        writeln!(self.input, "{}", text)?;
        self.input.flush()
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        let _ = self.send(vec![("c", word("quit"))]);
        // A requested shutdown must not orphan a backend; its own quit drains operation workers.
        let _ = self.child.wait();
    }
}
pub fn word(value: &str) -> Json {
    Json::Str(value.into())
}
pub fn number(value: usize) -> Json {
    Json::Num(value.to_string())
}
pub fn text<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap_or("")
}
pub fn count(value: &Json, key: &str) -> usize {
    value
        .get(key)
        .and_then(Json::as_f64)
        .unwrap_or(0.0)
        .max(0.0) as usize
}
pub fn flag(value: &Json, key: &str) -> bool {
    value.get(key).and_then(Json::as_bool).unwrap_or(false)
}

// An echo child for model tests: every send comes back as an event, quit ends it.
#[cfg(test)]
pub(crate) fn echo_wire() -> (Wire, std::thread::JoinHandle<()>) {
    let quit = jsondoc::render(&Json::Obj(vec![("c".into(), word("quit"))])).replace('\n', "");
    let mut child = Command::new("sh").args(["-c",
        r#"while IFS= read -r line; do [ "$line" = "$1" ] && exit 0; printf '%s\n' "$line"; done"#,
        "flea-tui-wire-test", &quit]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let (tx, events) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let value = line.map_err(|error| error.to_string()).and_then(|line| jsondoc::parse(&line));
            if tx.send(value).is_err() { break; }
        }
    });
    // The tests read events straight from the channel and never poll, so they need no wake.
    (Wire { child, input, events, wake: None }, reader)
}

// Ends an echo wire, proving the child exits cleanly rather than lingering.
#[cfg(test)]
pub(crate) fn finish(mut wire: Wire, reader: std::thread::JoinHandle<()>) {
    wire.send(vec![("c", word("quit"))]).unwrap();
    assert!(wire.child.wait().unwrap().success(), "TUI test wire child did not exit cleanly");
    reader.join().unwrap();
}

// The sort-anchor battery, kept beside this suite so this file stays a mechanism file.
#[cfg(test)]
#[path = "sort_tests.rs"]
mod sort_tests;

// Issue 133's key battery, which drives the same echo wire.
#[cfg(test)]
#[path = "trash_tests.rs"]
mod trash_tests;

// Tabs040's reorder battery, which drives the same echo wire.
#[cfg(test)]
#[path = "tabmove_tests.rs"]
mod tabmove_tests;

#[cfg(test)]
#[path = "selection_tests.rs"]
mod selection_tests;

// The model's status lines and faults, which also need the echo wire.
#[cfg(test)]
#[path = "words_tests.rs"]
mod words_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::model::{Model, Row};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn a_wake_the_reader_closed_leaves_the_poll() {
        let (mut wire, reader) = echo_wire();
        let (wake, ring) = UnixStream::pair().unwrap();
        wake.set_nonblocking(true).unwrap();
        (&ring).write_all(&[1, 1]).unwrap();
        wire.wake = Some(wake);
        wire.drain_wake();
        assert!(wire.wake_fd() >= 0, "an open wake stays polled after its bytes are read");
        // The reader thread's end goes away with it; a closed socket would poll ready on every call from then on.
        drop(ring);
        wire.drain_wake();
        assert_eq!(wire.wake_fd(), -1);
        drop(wire);
        reader.join().unwrap();
    }

    // In-harness the backend is the test binary, which refuses --backend and exits; its stop is queued before the wake rings.
    #[test]
    fn backend_stop_is_queued_before_the_wake_rings() {
        let wire = Wire::start(None).unwrap();
        let (stdin, _held) = UnixStream::pair().unwrap();
        let before = std::time::Instant::now();
        assert!(!crate::tui::terminal::wait_input(stdin.as_raw_fd(), wire.wake_fd(), 5000).unwrap());
        assert!(before.elapsed() < Duration::from_millis(5000), "the wake never rang after the backend exited");
        let event = wire.events.try_recv();
        assert!(matches!(&event, Ok(Err(error)) if error == "The file list stopped; restart Flea"), "a ringing wake had no stop queued: {:?}", event.map(|value| value.map(|_| ())));
    }

    #[test]
    fn hidden_reload_restores_cursor_after_the_older_window_reply() {
        let quit = jsondoc::render(&Json::Obj(vec![("c".into(), word("quit"))])).replace('\n', "");
        // Echo actual Wire::send output; honoring Quit also lets Wire::drop reap the child during a panic.
        let mut child = Command::new("sh").args(["-c",
            r#"while IFS= read -r line; do [ "$line" = "$1" ] && exit 0; printf '%s\n' "$line"; done"#,
            "flea-tui-wire-test", &quit]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = line
                    .map_err(|error| error.to_string())
                    .and_then(|line| jsondoc::parse(&line));
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut wire = Wire {
            child,
            input,
            events,
            wake: None,
        };
        let next = |wire: &Wire| {
            wire.events
                .recv_timeout(Duration::from_secs(2))
                .expect("TUI did not request the expected listing window")
                .unwrap()
        };
        let row = |name: &str| Json::Obj(vec![("n".into(), word(name))]);
        let rows = |start, names: &[&str]| {
            Json::Obj(vec![
                ("t".into(), word("rows")),
                ("start".into(), number(start)),
                (
                    "rows".into(),
                    Json::Arr(names.iter().map(|name| row(name)).collect()),
                ),
            ])
        };
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.total = 6;
        model.cursor = 3;
        model.top = 2;
        model.rows.insert(3, Row::parse(&row("charlie.txt"), &[]));
        model.open(model.path.clone(), &mut wire).unwrap();
        assert_eq!(text(&next(&wire), "c"), "list");

        // List emits listed plus rows(0); handling listed already queued window(3) and locate(charlie).
        model
            .receive(
                jsondoc::parse(r#"{"t":"listed","n":5}"#).unwrap(),
                &mut wire,
            )
            .unwrap();
        assert_eq!(text(&next(&wire), "c"), "peek");
        let older_window = next(&wire);
        assert_eq!(text(&older_window, "c"), "window");
        assert_eq!(count(&older_window, "start"), 3);
        let locate = next(&wire);
        assert_eq!(text(&locate, "c"), "locate");
        assert_eq!(text(&locate, "path"), "/listing/charlie.txt");
        let names = ["amber", "bronze", "charlie.txt", "delta.txt", "echo.txt"];
        model.receive(rows(0, &names), &mut wire).unwrap();
        let restored_window = next(&wire);
        assert_eq!(text(&restored_window, "c"), "window");
        assert_eq!(count(&restored_window, "start"), 2);
        assert_eq!(count(&restored_window, "count"), model.height);

        model.receive(rows(3, &names[3..]), &mut wire).unwrap();
        assert!(
            model.current_path().is_none(),
            "the older reply does not contain the restored cursor"
        );
        model.receive(jsondoc::parse(r#"{"t":"located","directory":"/listing","path":"/listing/charlie.txt","index":2}"#).unwrap(), &mut wire).unwrap();
        assert!(model.restore_path.is_none());
        model
            .receive(
                rows(count(&restored_window, "start"), &names[2..]),
                &mut wire,
            )
            .unwrap();
        assert_eq!((model.cursor, model.top), (2, 2));
        assert_eq!(
            model.current_path(),
            Some(PathBuf::from("/listing/charlie.txt"))
        );
        assert!(model.cursor < model.top + model.height);
        wire.send(vec![("c", word("quit"))]).unwrap();
        assert!(
            wire.child.wait().unwrap().success(),
            "TUI test wire child did not exit cleanly"
        );
        reader.join().unwrap();
        assert!(
            wire.events.try_recv().is_err(),
            "restoration queued an unexpected extra request"
        );
    }

    #[test]
    fn the_backend_inherits_the_prefetch_only_when_one_waits() {
        let bare = backend_command(std::path::Path::new("flea"), None);
        assert!(bare.get_envs().next().is_none(), "a local start hands the backend no prefetch");
        let found = crate::gvfsprefetch::Prefetch {
            dest: PathBuf::from("/run/flea/gvfs-1.list"),
            path: "/run/user/1000/gvfs/smb-share:server=x,share=y/dir".into(),
            start_ms: 1728000000000,
        };
        let cmd = backend_command(std::path::Path::new("flea"), Some(&found));
        let mut envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(key, value)| (key.to_string_lossy().into_owned(), value.map(|v| v.to_string_lossy().into_owned())))
            .collect();
        envs.sort();
        // Sample command: FLEA_GVFS_PATH, FLEA_GVFS_PREFETCH and FLEA_GVFS_START beside --backend.
        assert_eq!(envs, vec![
            (String::from(crate::gvfsprefetch::PATH_ENV), Some(String::from("/run/user/1000/gvfs/smb-share:server=x,share=y/dir"))),
            (String::from(crate::gvfsprefetch::PREFETCH_ENV), Some(String::from("/run/flea/gvfs-1.list"))),
            (String::from(crate::gvfsprefetch::START_ENV), Some(String::from("1728000000000"))),
        ]);
    }
}
