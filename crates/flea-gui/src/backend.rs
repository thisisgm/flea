use async_channel::Receiver;
use flea::protocol::{parse_reply, Reply, Request};
use std::io::{self, BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;

pub struct Backend {
    requests: Sender<Control>,
    pub replies: Receiver<Result<Reply, String>>,
}

enum Control {
    Window { start: usize, count: usize },
    Stop,
}

fn command() -> Command {
    let binary = std::env::var_os("FLEA_BIN")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "flea".into());
    let mut command = Command::new(binary);
    command
        .arg("--backend")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    command
}

impl Backend {
    #[cfg(test)]
    pub(crate) fn test_channel() -> (Self, impl FnMut() -> Option<(usize, usize)>) {
        let (requests, receiver) = mpsc::channel();
        let (_, replies) = async_channel::unbounded();
        let receive = move || match receiver.try_recv().ok()? {
            Control::Window { start, count } => Some((start, count)),
            Control::Stop => None,
        };
        (Self { requests, replies }, receive)
    }

    pub fn start(path: String, select: Option<String>, first: usize) -> io::Result<Self> {
        let (requests, receiver) = mpsc::channel();
        let finished = requests.clone();
        let (output, replies) = async_channel::unbounded();
        thread::Builder::new()
            .name("flea-backend".into())
            .spawn(move || {
                let run = || -> Result<(), String> {
                    let mut child = command()
                        .spawn()
                        .map_err(|error| format!("the backend could not start: {error}"))?;
                    let mut input = child.stdin.take().expect("piped stdin");
                    let stdout = child.stdout.take().expect("piped stdout");
                    let messages = output.clone();
                    let reader =
                        thread::Builder::new()
                            .name("flea-replies".into())
                            .spawn(move || {
                                for line in BufReader::new(stdout).lines() {
                                    let line = match line {
                                        Ok(line) => line,
                                        Err(error) => {
                                            let _ = messages.send_blocking(Err(error.to_string()));
                                            let _ = finished.send(Control::Stop);
                                            return;
                                        }
                                    };
                                    let reply = parse_reply(&line);
                                    if messages.send_blocking(reply).is_err() {
                                        let _ = finished.send(Control::Stop);
                                        return;
                                    }
                                }
                                let _ = messages.send_blocking(Err(
                                    "the backend closed its reply stream".into(),
                                ));
                                let _ = finished.send(Control::Stop);
                            });
                    match reader {
                        Ok(reader) => {
                            let result = Request::List {
                                path: &path,
                                first,
                                select: select.as_deref(),
                            }
                            .write_to(&mut input)
                            .and_then(|()| {
                                for request in receiver {
                                    match request {
                                        Control::Window { start, count } => {
                                            Request::Window { start, count }.write_to(&mut input)?
                                        }
                                        Control::Stop => break,
                                    }
                                }
                                Ok(())
                            })
                            .map_err(|error| {
                                format!("the backend request could not be written: {error}")
                            });
                            drop(input);
                            let _ = child.kill();
                            let _ = child.wait();
                            let _ = reader.join();
                            result
                        }
                        Err(error) => {
                            drop(input);
                            let _ = child.kill();
                            let _ = child.wait();
                            Err(format!("the reply reader could not start: {error}"))
                        }
                    }
                };
                if let Err(error) = run() {
                    let _ = output.send_blocking(Err(error));
                }
            })?;
        Ok(Self { requests, replies })
    }

    pub fn window(&self, start: usize, count: usize) -> Result<(), String> {
        self.requests
            .send(Control::Window { start, count })
            .map_err(|_| "the backend stopped accepting requests".into())
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // The worker closes stdin, terminates and reaps the child without blocking the UI.
        let _ = self.requests.send(Control::Stop);
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
