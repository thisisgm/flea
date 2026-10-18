// The extraction-only runner samples progress while preserving the archive jail and cancellation drain.
use super::archivepass::{Prompt, SevenFailure};
use super::archivework::{cancel_failure, failure_message, Work, SANDBOX_MISSING};
use super::extractprogress::Progress;
use super::opsreq::{op_err, OpMsg};
use super::{jail, sandbox};
use crate::error::{from_io, FleaError};
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc::Sender, Arc, Mutex};
use std::time::Duration;

pub(crate) struct Command<'a> {
    pub inner: Vec<String>,
    pub archive: &'a Path,
    pub secret: Option<&'a str>,
    pub seven: bool,
}

pub(crate) fn run(
    command: Command<'_>,
    work: &mut Work,
    cancel: &AtomicBool,
    progress: Progress,
    report: Option<(usize, &Sender<OpMsg>)>,
) -> Result<(Option<SevenFailure>, Progress), FleaError> {
    let Command {
        inner,
        archive,
        secret,
        seven,
    } = command;
    if !sandbox::available() {
        return Err(op_err("archive", "", SANDBOX_MISSING));
    }
    let progress = Arc::new(Mutex::new(progress));
    let stage = work.dir.join("out");
    let mut inner = inner;
    if inner
        .first()
        .is_some_and(|tool| tool == "7z" || tool == "bsdtar")
    {
        inner.insert(1, if seven { "-bb1" } else { "-v" }.into());
    }
    let mut full = sandbox::wrap_archive(&inner, archive, &work.dir);
    sandbox::add_status(&mut full, inner.len());
    let mut jailed = jail::spawn_jailed(&full, |command| {
        command.stdin(if secret.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        command.stdout(if seven { Stdio::piped() } else { Stdio::null() });
        command.stderr(Stdio::piped());
    })
    .map_err(|e| from_io("archive", &full[0], &e))?;
    let stdout = jailed.child.stdout.take();
    let stderr = jailed.child.stderr.take();
    let output_progress = Arc::clone(&progress);
    let output_stage = stage.clone();
    let output = std::thread::spawn(move || {
        if let Some(pipe) = stdout {
            read(pipe, Some((&output_progress, &output_stage, true)));
        }
    });
    let error_progress = Arc::clone(&progress);
    let error_stage = stage.clone();
    let errors = std::thread::spawn(move || {
        stderr
            .map(|pipe| {
                read(
                    pipe,
                    (!seven).then_some((&error_progress, error_stage.as_path(), false)),
                )
            })
            .unwrap_or_default()
    });
    let mut prompt = match Prompt::new(&mut jailed.child, secret) {
        Ok(prompt) => prompt,
        Err(error) => {
            jail::kill_tree(&mut jailed);
            return Err(from_io("archive", &full[0], &error));
        }
    };
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancel_failure("archive", &mut jailed, errors, work));
        }
        Prompt::advance(&mut prompt);
        if let Some((id, tx)) = report {
            progress.lock().unwrap().sample(&stage, id, tx, false);
        }
        match jailed.child.try_wait() {
            Ok(Some(status)) if output.is_finished() && errors.is_finished() => {
                output.join().ok();
                let text = errors.join().unwrap_or_default();
                let progress = Arc::try_unwrap(progress)
                    .ok()
                    .unwrap()
                    .into_inner()
                    .unwrap();
                let failed = (!status.success()).then(|| SevenFailure {
                    code: status.code(),
                    message: failure_message("archive", &status, &text),
                });
                return Ok((failed, progress));
            }
            Ok(_) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                jail::kill_tree(&mut jailed);
                return Err(from_io("archive", &full[0], &e));
            }
        }
    }
}

// A hostile member name cannot grow a line without bound; diagnostics retain only their bounded tail.
type Observation<'a> = (&'a Mutex<Progress>, &'a Path, bool);

fn read(mut pipe: impl Read, progress: Option<Observation<'_>>) -> String {
    const LINE_LIMIT: usize = 65_536;
    let mut chunk = [0u8; 8192];
    let mut pending = Vec::new();
    let mut tail = String::new();
    let mut skipping = false;
    loop {
        let count = match pipe.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return tail,
        };
        if count == 0 {
            if !skipping && !pending.is_empty() {
                complete_line(&pending, progress, &mut tail);
            }
            return tail;
        }
        for part in chunk[..count].split_inclusive(|byte| *byte == b'\n') {
            let whole = part.last() == Some(&b'\n');
            if pending.len() + part.len() > LINE_LIMIT {
                pending.clear();
                skipping = true;
            }
            if !skipping {
                pending.extend_from_slice(part);
                if whole {
                    complete_line(&pending, progress, &mut tail);
                } else if let Some((progress, _, false)) = progress {
                    if let Ok(text) = std::str::from_utf8(&pending) {
                        progress.lock().unwrap().preview(text);
                    }
                }
            }
            if whole {
                pending.clear();
                skipping = false;
            }
        }
    }
}

fn complete_line(bytes: &[u8], progress: Option<Observation<'_>>, tail: &mut String) {
    let text = String::from_utf8_lossy(bytes);
    if let Some((progress, stage, seven)) = progress {
        progress
            .lock()
            .unwrap()
            .line(text.trim_end_matches(['\r', '\n']), seven, stage);
    }
    if !text.trim().is_empty() {
        *tail = text.chars().take(4096).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::extractindex::Index;
    use crate::backend::testdir::TestDir;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::sync::mpsc::{channel, Receiver};

    struct Paused {
        first: Cursor<Vec<u8>>,
        ready: Sender<()>,
        release: Receiver<()>,
    }

    impl Read for Paused {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let count = self.first.read(bytes)?;
            if count == 0 {
                self.ready.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            Ok(count)
        }
    }

    #[test]
    fn bsdtar_unterminated_member_name_exposes_live_unpacked_bytes() {
        let root = TestDir::new("extractpartialline");
        root.file("done", "abc");
        root.file("large", &"x".repeat(4096));
        let index = Index {
            known: true,
            total: 2,
            bytes: 8195,
            members: HashMap::from([("done".into(), 3), ("large".into(), 8192)]),
            ..Index::default()
        };
        let progress = Arc::new(Mutex::new(Progress::new(index, false)));
        let observed = Arc::clone(&progress);
        let stage = root.path().to_owned();
        let (ready_tx, ready) = channel();
        let (release, release_rx) = channel();
        let reader = std::thread::spawn(move || {
            let pipe = Paused {
                first: Cursor::new(b"x done\nx large".to_vec()),
                ready: ready_tx,
                release: release_rx,
            };
            read(pipe, Some((&observed, &stage, false)))
        });
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let (tx, rx) = channel();
        progress.lock().unwrap().sample(root.path(), 44, &tx, false);
        release.send(()).unwrap();
        reader.join().unwrap();
        match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            OpMsg::JobEntries {
                done, bytes, file, ..
            } => {
                assert_eq!(done, 1);
                assert_eq!(bytes, 4099);
                assert_eq!(file, "large");
            }
            _ => panic!("expected extraction sample"),
        }
    }
}
