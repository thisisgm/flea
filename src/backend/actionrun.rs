// An action has one process group per invocation and a bounded merged stdout and stderr tail.
use super::actions::Action;
use super::opsreq::{OpMsg, PROGRESS_EVERY};
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

const LINE_BYTES: usize = 4096;
const TAIL_LINES: usize = 2;
const READ_CHUNKS: usize = 8;
const DRAIN_CHUNKS: usize = 128;
const READ_BUFFER: usize = 8192;
const EXIT_POLL: Duration = Duration::from_millis(10);
const SIGKILL: i32 = 9;

#[derive(Default)]
struct Tail {
    lines: Vec<String>,
    pending: Vec<u8>,
}
impl Tail {
    fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte == b'\n' {
                self.lines.push(
                    bounded_line(&self.pending)
                        .trim_end_matches('\r')
                        .to_string(),
                );
                if self.lines.len() > TAIL_LINES {
                    self.lines.remove(0);
                }
                self.pending.clear();
            } else {
                if self.pending.len() == LINE_BYTES {
                    self.pending.drain(..LINE_BYTES / 2);
                }
                self.pending.push(byte);
            }
        }
    }
    fn lines(&self) -> Vec<String> {
        let mut lines = self.lines.clone();
        if !self.pending.is_empty() {
            lines.push(bounded_line(&self.pending));
        }
        if lines.len() > TAIL_LINES {
            lines.drain(..lines.len() - TAIL_LINES);
        }
        lines
    }
}

fn bounded_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut end = text.len().min(LINE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub(crate) fn background(
    id: usize,
    action: &Action,
    paths: Vec<String>,
    selection: Option<Vec<super::menu_actions::Selected>>,
    cancel: Arc<AtomicBool>,
    tx: Sender<OpMsg>,
) {
    let items = selection.unwrap_or_else(|| super::jobwork::capture(&paths));
    let mut failed = Vec::new();
    let mut errors = Vec::new();
    let mut tail = Tail::default();
    let mut done = 0;
    for (index, path) in paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let file = Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let _ = tx.send(OpMsg::JobSample {
            id,
            done,
            total: paths.len(),
            file: file.clone(),
            output: tail.lines(),
        });
        let argv = if action.script {
            std::iter::once(action.manifest.run[0].clone())
                .chain(paths.clone())
                .collect()
        } else {
            action.argv(Path::new(path))
        };
        let selected: Vec<_> = items
            .iter()
            .filter(|item| action.script || item.path == Path::new(path))
            .cloned()
            .collect();
        let checked_paths = if action.script {
            paths.iter().map(std::path::PathBuf::from).collect()
        } else {
            vec![std::path::PathBuf::from(path)]
        };
        let result = super::menu_actions::validate_sources(Some(&selected), &checked_paths)
            .and_then(|()| {
                invoke(
                    &argv,
                    Path::new(path).parent().unwrap_or(Path::new("/")),
                    &cancel,
                    &mut tail,
                    |tail| {
                        let _ = tx.send(OpMsg::JobSample {
                            id,
                            done,
                            total: paths.len(),
                            file: file.clone(),
                            output: tail.lines(),
                        });
                    },
                )
            });
        match result {
            Ok(()) => done += if action.script { paths.len() } else { 1 },
            Err(error) if !cancel.load(Ordering::Relaxed) => {
                failed.extend(super::jobwork::failures(&selected));
                errors.push(error);
            }
            Err(_) => {}
        }
        let _ = tx.send(OpMsg::JobSample {
            id,
            done,
            total: paths.len(),
            file,
            output: tail.lines(),
        });
        if action.script || index + 1 == paths.len() {
            break;
        }
    }
    let error = errors.first().cloned().unwrap_or_default();
    let _ = tx.send(OpMsg::JobDone {
        id,
        failed,
        error,
        line: String::new(),
    });
}

fn invoke(
    argv: &[String],
    directory: &Path,
    cancel: &AtomicBool,
    tail: &mut Tail,
    mut sample: impl FnMut(&Tail),
) -> Result<(), String> {
    let (mut reader, writer) = UnixStream::pair().map_err(|e| e.to_string())?;
    let stdout: OwnedFd = writer.try_clone().map_err(|e| e.to_string())?.into();
    let stderr: OwnedFd = writer.into();
    reader.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .process_group(0);
    crate::open::prepare(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    // Command retains its pipe ends until dropped, so the reader must not depend on its destructor.
    drop(command);
    let mut last = Instant::now();
    let mut buffer = [0u8; READ_BUFFER];
    loop {
        for _ in 0..READ_CHUNKS {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => tail.push(&buffer[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        if last.elapsed() >= PROGRESS_EVERY {
            sample(tail);
            last = Instant::now();
        }
        let cancelled = cancel.load(Ordering::Relaxed);
        let exited = if cancelled {
            false
        } else {
            match super::child::wait_for_exit(&child, Instant::now() + EXIT_POLL) {
                super::child::Woke::Exited => true,
                super::child::Woke::Deadline => false,
                super::child::Woke::Failed(error) => {
                    let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
                    let _ = child.wait();
                    return Err(error.to_string());
                }
            }
        };
        if cancelled || exited {
            // The leader has not been reaped, so its group id cannot name a later, unrelated process.
            let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
            let status = child.wait().map_err(|e| e.to_string())?;
            for _ in 0..DRAIN_CHUNKS {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => tail.push(&buffer[..n]),
                }
            }
            return if cancelled {
                Err("Action cancelled.".into())
            } else if status.success() {
                Ok(())
            } else {
                Err(format!("Action exited with {}.", status))
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_output_tail_bounds_lossy_decoding_and_keeps_only_two_lines() {
        let mut tail = Tail::default();
        tail.push(&vec![0xff; 20_000]);
        assert!(tail.lines()[0].len() <= LINE_BYTES);
        tail.push(b"\nfirst\nsecond\n");
        assert_eq!(tail.lines(), ["first", "second"]);
    }

    #[test]
    fn successful_action_reaps_its_remaining_group() {
        let sandbox = crate::backend::testdir::TestDir::new("action-success-group");
        let root = sandbox.path();
        let pidfile = root.join("child.pid");
        let argv = vec![
            "/bin/sh".into(),
            "-c".into(),
            "sleep 30 & echo $! > \"$1\"; exit 0".into(),
            "sh".into(),
            pidfile.to_str().unwrap().into(),
        ];
        invoke(
            &argv,
            root,
            &AtomicBool::new(false),
            &mut Tail::default(),
            |_| {},
        )
        .unwrap();
        let pid = std::fs::read_to_string(pidfile).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
            if stat.map_or(true, |text| {
                text.rsplit_once(')').unwrap().1.split_whitespace().next() == Some("Z")
            }) {
                break;
            }
            assert!(Instant::now() < deadline, "child still alive after success");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
