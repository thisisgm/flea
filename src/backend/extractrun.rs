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

// A hostile member name cannot grow a line past this, in a listing or in the extracting tool's output.
pub(crate) const LINE_LIMIT: usize = 65_536;
// The extracting child is polled this often for its exit, a cancel and a progress sample.
const POLL: Duration = Duration::from_millis(50);
// Each kept diagnostic line is cut to this many characters.
const DIAGNOSTIC_CHARS: usize = 4096;
// The first member errors are kept apart from progress lines, so a later member never replaces the cause.
const MEMBER_ERRORS: usize = 8;

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
            Ok(_) => std::thread::sleep(POLL),
            Err(e) => {
                jail::kill_tree(&mut jailed);
                return Err(from_io("archive", &full[0], &e));
            }
        }
    }
}

// The progress a pipe feeds, the staging directory it samples, and whether the tool is 7z.
type Observation<'a> = (&'a Mutex<Progress>, &'a Path, bool);

// The first MEMBER_ERRORS diagnostic lines and the last one, never a bsdtar "x name" progress line.
#[derive(Default)]
struct Diagnostics {
    first: Vec<String>,
    last: String,
}

impl Diagnostics {
    fn text(mut self) -> String {
        if !self.last.is_empty() && self.first.last() != Some(&self.last) {
            self.first.push(self.last);
        }
        self.first.join("\n")
    }
}

fn read(mut pipe: impl Read, progress: Option<Observation<'_>>) -> String {
    let mut chunk = [0u8; 8192];
    let mut pending = Vec::new();
    let mut diagnostics = Diagnostics::default();
    let mut skipping = false;
    loop {
        let count = match pipe.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return diagnostics.text(),
        };
        if count == 0 {
            if !skipping && !pending.is_empty() {
                complete_line(&pending, progress, &mut diagnostics);
            }
            return diagnostics.text();
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
                    complete_line(&pending, progress, &mut diagnostics);
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

// Sample: bsdtar -v stderr "x docs/a.txt" for progress, "x docs/b: Could not stat docs/b: Not a directory" for a member error.
// corner: a member whose own name holds ": " reads as an error line, which only a failed extract ever shows.
fn complete_line(bytes: &[u8], progress: Option<Observation<'_>>, diagnostics: &mut Diagnostics) {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\r', '\n']);
    if let Some((progress, stage, seven)) = progress {
        progress.lock().unwrap().line(text, seven, stage);
    }
    let line = match text.strip_prefix("x ") {
        Some(member) if matches!(progress, Some((_, _, false))) => {
            if !member.contains(": ") {
                return;
            }
            member
        }
        _ => text,
    };
    let line: String = line.trim().chars().take(DIAGNOSTIC_CHARS).collect();
    if line.is_empty() {
        return;
    }
    if diagnostics.first.len() < MEMBER_ERRORS {
        diagnostics.first.push(line.clone());
    }
    diagnostics.last = line;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::extractindex::Index;
    use crate::backend::testdir::TestDir;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::os::unix::process::ExitStatusExt;
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

    #[test]
    fn a_progress_line_never_replaces_a_member_error() {
        let progress = Mutex::new(Progress::new(Index::default(), false));
        let text = read(b"bsdtar: blocked/child: Not a directory\nx lastfile\n".as_slice(), Some((&progress, Path::new("/nonexistent"), false)));
        let message = failure_message("archive", &std::process::ExitStatus::from_raw(256), &text);
        assert_eq!(message, "bsdtar: blocked/child: Not a directory");
    }

    // The acceptance case: real bsdtar reports the member error mid-stream and ends on its generic trailer.
    #[test]
    fn a_real_bsdtar_member_error_survives_the_delayed_error_trailer() {
        let root = TestDir::new("extractmembererror");
        root.dir("out");
        let archive = root.join("bad.tar");
        let script = "import io,sys,tarfile\nwith tarfile.open(sys.argv[1], 'w') as t:\n for name in ('blocked', 'blocked/child', 'lastfile'):\n  m=tarfile.TarInfo(name);m.size=1;t.addfile(m,io.BytesIO(b'x'))";
        assert!(std::process::Command::new("/usr/bin/python3").args(["-c", script]).arg(&archive).status().unwrap().success());
        let output = std::process::Command::new("bsdtar").args(["-v", "-x", "-f"]).arg(&archive).arg("-C").arg(root.join("out")).output().unwrap();
        assert!(!output.status.success());
        let progress = Mutex::new(Progress::new(Index::default(), false));
        let text = read(output.stderr.as_slice(), Some((&progress, &root.join("out"), false)));
        let message = failure_message("archive", &output.status, &text);
        assert!(message.starts_with("blocked/child: ") && message.contains("Not a directory"), "{message}");
    }
}
