// The extraction-only runner samples progress while preserving the archive jail and cancellation drain.
use super::archivepass::{Prompt, SevenFailure};
use super::archivework::{
    before_trailer, cancel_failure, failure_message, tool_note, Work, SANDBOX_MISSING,
};
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
// The first member errors are kept apart from progress lines and bsdtar's own lines, so neither a later member nor a warning replaces the cause.
const MEMBER_ERRORS: usize = 8;
// bsdtar names itself before its own lines: warnings, archive-level errors and the delayed-error trailer.
const TOOL_PREFIX: &str = "bsdtar: ";

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
    // A piped -bso stream is buffered, so 7z's member log moves to unbuffered stderr and its errors to stdout.
    let seven_stderr = seven && inner.first().is_some_and(|tool| tool == "7z");
    if inner
        .first()
        .is_some_and(|tool| tool == "7z" || tool == "bsdtar")
    {
        inner.insert(1, if seven { "-bb1" } else { "-v" }.into());
        if seven_stderr {
            inner.insert(2, "-bso2".into());
            inner.insert(3, "-bse1".into());
        }
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
        stdout
            .map(|pipe| {
                read(
                    pipe,
                    (!seven_stderr).then_some((&output_progress, output_stage.as_path(), true)),
                )
            })
            .unwrap_or_default()
    });
    let error_progress = Arc::clone(&progress);
    let error_stage = stage.clone();
    let errors = std::thread::spawn(move || {
        stderr
            .map(|pipe| {
                read(
                    pipe,
                    (!seven || seven_stderr).then_some((
                        &error_progress,
                        error_stage.as_path(),
                        seven,
                    )),
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
                let stdout = output.join().unwrap_or_default();
                let stderr = errors.join().unwrap_or_default();
                // Under -bse1 7z's own errors are on stdout; stderr keeps only the jail's refusal, read first.
                let text = if seven_stderr {
                    format!("{stderr}\n{stdout}")
                } else {
                    stderr
                };
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

// The first MEMBER_ERRORS member errors, bsdtar's own first line and first diagnosis, and the last line; never a bsdtar "x name" progress line.
#[derive(Default)]
struct Diagnostics {
    first: Vec<String>,
    // bsdtar's own first line, then its first line that is no warning or retry note when the first one was.
    tool: Vec<String>,
    diagnosed: bool,
    last: String,
}

impl Diagnostics {
    fn text(self) -> String {
        let mut lines = self.tool;
        lines.extend(self.first);
        if !self.last.is_empty() && lines.last() != Some(&self.last) {
            lines.push(self.last);
        }
        lines.join("\n")
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
    if matches!(progress, Some((_, _, true))) && !jail_line(text) {
        return;
    }
    let bsdtar = matches!(progress, Some((_, _, false)));
    let line = match text.strip_prefix("x ") {
        Some(member) if bsdtar => {
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
    match line.strip_prefix(TOOL_PREFIX).filter(|_| bsdtar) {
        // The trailer is kept only as the last line.
        Some(own) if before_trailer(own) == Some("") => {}
        Some(own) => {
            let note = tool_note(own);
            if diagnostics.tool.is_empty() || !note && !diagnostics.diagnosed {
                diagnostics.tool.push(line.clone());
            }
            diagnostics.diagnosed |= !note;
        }
        None if diagnostics.first.len() < MEMBER_ERRORS => diagnostics.first.push(line.clone()),
        None => {}
    }
    diagnostics.last = line;
}

// Sample: "bwrap: Can't find source path /a.7z: No such file or directory"; the rest of 7z's log never diagnoses.
fn jail_line(text: &str) -> bool {
    [sandbox::BWRAP, sandbox::PRLIMIT].iter().any(|tool| {
        text.strip_prefix(tool)
            .is_some_and(|rest| rest.starts_with(": "))
    })
}

#[cfg(test)]
#[path = "extractrun_tests.rs"]
mod tests;
