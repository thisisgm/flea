//! A pane's backend child; all process and pipe work stays off the UI thread.
use async_channel::Receiver;
use flea::protocol::{parse_reply, Reply, Request};
use std::io::{self, BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender},
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Failure {
    pub key: &'static str,
    pub detail: String,
}

#[derive(Debug)]
pub enum Event {
    Reply(Reply),
    Failed(Failure),
    Drained,
}

pub struct Backend {
    requests: Sender<Control>,
    quitting: AtomicBool,
    pub replies: Receiver<Event>,
    drained: Receiver<()>,
}

enum Control {
    List { path: String, first: usize },
    Window { start: usize, count: usize },
    Quit,
    Eof,
}

fn command() -> Command {
    let mut command = Command::new(crate::window::startup::binary());
    command.arg("--backend");
    command
}

fn failure(key: &'static str, detail: impl ToString) -> Failure {
    Failure {
        key,
        detail: detail.to_string(),
    }
}

fn reap(child: &mut Child, graceful: bool) -> Result<(), Failure> {
    let deadline = Instant::now() + Duration::from_secs(30);
    if graceful {
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) => break,
                Err(error) => return Err(failure("window.backend_wait", error)),
            }
        }
    }
    let _ = child.kill();
    child
        .wait()
        .map_err(|error| failure("window.backend_wait", error))?;
    if graceful {
        Err(failure("window.backend_timeout", ""))
    } else {
        Ok(())
    }
}

impl Backend {
    pub fn start(path: String, select: Option<String>, first: usize) -> io::Result<Self> {
        Self::start_command(command(), path, select, first)
    }

    // Injecting the command lets acceptance drive the real pipe, EOF and reaping path.
    pub fn start_command(
        mut command: Command,
        path: String,
        select: Option<String>,
        first: usize,
    ) -> io::Result<Self> {
        let (requests, receiver) = mpsc::channel();
        let finished = requests.clone();
        let (output, replies) = async_channel::unbounded();
        let (done, drained) = async_channel::bounded(1);
        thread::Builder::new()
            .name("flea-backend".into())
            .spawn(move || {
                let run = || -> Result<(), Failure> {
                    let mut child = command
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::inherit())
                        .spawn()
                        .map_err(|error| failure("window.backend_start", error))?;
                    let mut input = child.stdin.take().expect("piped stdin");
                    let stdout = child.stdout.take().expect("piped stdout");
                    let messages = output.clone();
                    let reader =
                        thread::Builder::new()
                            .name("flea-replies".into())
                            .spawn(move || {
                                for line in BufReader::new(stdout).lines() {
                                    let event = match line {
                                        Ok(line) => match parse_reply(&line) {
                                            Ok(reply) => Event::Reply(reply),
                                            Err(error) => Event::Failed(failure(
                                                "window.backend_protocol",
                                                error,
                                            )),
                                        },
                                        Err(error) => {
                                            let _ = messages.send_blocking(Event::Failed(failure(
                                                "window.backend_read",
                                                error,
                                            )));
                                            break;
                                        }
                                    };
                                    if messages.send_blocking(event).is_err() {
                                        break;
                                    }
                                }
                                let _ = finished.send(Control::Eof);
                            });
                    let reader = match reader {
                        Ok(reader) => reader,
                        Err(error) => {
                            drop(input);
                            let _ = reap(&mut child, false);
                            return Err(failure("window.backend_reader", error));
                        }
                    };
                    let mut graceful = false;
                    let result = Request::List {
                        path: &path,
                        first,
                        select: select.as_deref(),
                    }
                    .write_to(&mut input)
                    .and_then(|()| {
                        for control in receiver {
                            match control {
                                Control::List { path, first } => Request::List {
                                    path: &path,
                                    first,
                                    select: None,
                                }
                                .write_to(&mut input)?,
                                Control::Window { start, count } => {
                                    Request::Window { start, count }.write_to(&mut input)?
                                }
                                Control::Quit => {
                                    Request::Quit.write_to(&mut input)?;
                                    graceful = true;
                                    break;
                                }
                                Control::Eof => break,
                            }
                        }
                        Ok(())
                    })
                    .map_err(|error| failure("window.backend_write", error));
                    drop(input);
                    let waited = reap(&mut child, graceful);
                    let _ = reader.join();
                    result?;
                    waited?;
                    if graceful {
                        Ok(())
                    } else {
                        Err(failure("window.backend_eof", ""))
                    }
                };
                if let Err(error) = run() {
                    let _ = output.send_blocking(Event::Failed(error));
                }
                let _ = output.send_blocking(Event::Drained);
                let _ = done.try_send(());
            })?;
        Ok(Self {
            requests,
            quitting: AtomicBool::new(false),
            replies,
            drained,
        })
    }

    pub fn list(&self, path: String, first: usize) -> Result<(), Failure> {
        self.requests
            .send(Control::List { path, first })
            .map_err(|_| failure("window.backend_stopped", ""))
    }

    pub fn window(&self, start: usize, count: usize) -> Result<(), Failure> {
        if self.quitting.load(Ordering::Acquire) {
            return Err(failure("window.backend_stopped", ""));
        }
        self.requests
            .send(Control::Window { start, count })
            .map_err(|_| failure("window.backend_stopped", ""))
    }

    pub fn drained(&self) -> Receiver<()> {
        self.drained.clone()
    }

    pub fn quit(&self) {
        if !self.quitting.swap(true, Ordering::AcqRel) {
            let _ = self.requests.send(Control::Quit);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_channel() -> (Self, impl FnMut() -> Option<(usize, usize)>) {
        let (requests, receiver) = mpsc::channel();
        let (_, replies) = async_channel::unbounded();
        let (_, drained) = async_channel::bounded(1);
        let receive = move || match receiver.try_recv().ok()? {
            Control::Window { start, count } => Some((start, count)),
            _ => None,
        };
        (
            Self {
                requests,
                quitting: AtomicBool::new(false),
                replies,
                drained,
            },
            receive,
        )
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.quit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_argv_and_environment_match_the_qml_child() {
        let command = command();
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["--backend"]);
        assert_eq!(command.get_envs().count(), 0);
        assert!(command.get_current_dir().is_none());
    }
}
