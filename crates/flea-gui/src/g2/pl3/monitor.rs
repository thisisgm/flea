//! TrashMonitor.qml off the UI thread: one watch process, a settled count read after each burst, results on a channel.
use async_channel::{Receiver, Sender};
use flea::ui_logic_pl3::monitor::{self, Count, SETTLE_MS, TIMEOUT_MS};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// What the rail's Trash row hears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heard {
    Count(u64),
    /// A burst of changes settled. An open delete card re-reviews; a plain listing waits for a Count change.
    Changed,
    /// A catalog key under trash.monitor; the count keeps its last value.
    Failed(&'static str),
}

/// A started watch and what it hears.
pub type Started = std::io::Result<(Monitor, Receiver<Heard>)>;

/// The running watch; dropping it stops the watch process and its thread.
pub struct Monitor {
    child: Child,
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn(argv: &[String], stdout: Stdio) -> std::io::Result<Child> {
    Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()
}

// One count read with the oracle's 10 s ceiling; the reader thread keeps the wait off this one.
fn read_count(query: &[String]) -> (i32, bool, String) {
    let Ok(mut child) = spawn(query, Stdio::piped()) else {
        return (-1, false, String::new());
    };
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + Duration::from_millis(TIMEOUT_MS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return (
                    status.code().unwrap_or(-1),
                    false,
                    reader.join().unwrap_or_default(),
                )
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return (-1, true, String::new());
            }
        }
    }
}

impl Monitor {
    /// GIO's watch and count, each tied to this process so neither outlives the window.
    pub fn start() -> Started {
        let pid = std::process::id();
        Self::start_with(
            monitor::owned_command(pid, &monitor::WATCH),
            monitor::owned_command(pid, &monitor::QUERY),
        )
    }

    /// Any watch whose stdout lines are changes and any query that prints `trash::item-count: N`.
    pub fn start_with(watch: Vec<String>, query: Vec<String>) -> Started {
        let mut child = spawn(&watch, Stdio::piped())?;
        let lines = child.stdout.take().expect("piped stdout");
        let (heard, receiver) = async_channel::unbounded();
        let (changes, changed) = mpsc::channel();
        let _ = changes.send(());
        std::thread::spawn(move || {
            for _ in BufReader::new(lines).lines().map_while(Result::ok) {
                if changes.send(()).is_err() {
                    return;
                }
            }
        });
        std::thread::spawn(move || run(changed, query, heard));
        Ok((Self { child }, receiver))
    }
}

// A burst settles SETTLE_MS, says it changed, then reads once; changes during a read run it once more and say so after.
fn run(changed: mpsc::Receiver<()>, query: Vec<String>, heard: Sender<Heard>) {
    let mut count = Count::default();
    let mut started = false;
    while changed.recv().is_ok() {
        loop {
            match changed.recv_timeout(Duration::from_millis(SETTLE_MS)) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = heard.send_blocking(Heard::Failed("trash.monitor.watch_failed"));
                    return;
                }
            }
        }
        if std::mem::replace(&mut started, true) && heard.send_blocking(Heard::Changed).is_err() {
            return;
        }
        let (mut again, mut more) = (count.refresh(), false);
        while again {
            let (exit, timed_out, output) = read_count(&query);
            while changed.try_recv().is_ok() {
                count.refresh();
                more = true;
            }
            let read = count.finished(exit, timed_out, &output);
            let message = if read.failed {
                Heard::Failed("trash.monitor.count_failed")
            } else {
                Heard::Count(count.count)
            };
            if heard.send_blocking(message).is_err() {
                return;
            }
            again = read.again && count.refresh();
        }
        if more && heard.send_blocking(Heard::Changed).is_err() {
            return;
        }
    }
    let _ = heard.send_blocking(Heard::Failed("trash.monitor.watch_failed"));
}
