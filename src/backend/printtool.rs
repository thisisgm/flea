// Bounded tool output and deadlines, shared by CUPS queries and sandboxed image jobs.
use super::{archivework::Work, child, jail, sandbox};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub const TOOL_WAIT: Duration = Duration::from_secs(60);
pub const QUERY_WAIT: Duration = Duration::from_secs(10);
const OUTPUT_CAP: usize = 64 * 1024;
const STEP: Duration = Duration::from_millis(20);
const READ_CHUNK_BYTES: usize = 4096;
const DRAIN_WAIT: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Failure { pub message: String, pub lingering: bool }

fn drain(pipe: impl Read + Send + 'static) -> Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut pipe = pipe;
        let mut held = Vec::new();
        let mut buf = [0; READ_CHUNK_BYTES];
        let result = loop {
            match pipe.read(&mut buf) {
                Ok(0) => break Ok(held),
                Ok(n) => {
                    let left = (OUTPUT_CAP + 1).saturating_sub(held.len());
                    held.extend_from_slice(&buf[..n.min(left)]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => break Err(crate::error::io_message(&e)),
            }
        };
        let _ = tx.send(result);
    });
    rx
}

// A tool the system lacks is named as missing; only a missing bwrap or prlimit is the sandbox's own fault.
// Sample input: ["lpstat", "-e"] with NotFound says "lpstat is not installed".
fn spawn_message(argv: &[String], error: &std::io::Error) -> String {
    let tool = argv.first().map_or("tool", String::as_str);
    let launcher = Path::new(tool).file_name().is_some_and(|name| name == sandbox::PRLIMIT || name == sandbox::BWRAP);
    if error.kind() == std::io::ErrorKind::NotFound && !launcher {
        format!("{tool} is not installed")
    } else {
        format!("{tool}: {}", crate::error::io_message(error))
    }
}

pub fn run(argv: &[String], cancel: &AtomicBool, wait: Duration) -> Result<String, Failure> {
    let failure = |message| Failure { message, lingering: false };
    if cancel.load(Ordering::Relaxed) { return Err(failure("cancelled".into())); }
    let mut jailed = jail::spawn_jailed(argv, |cmd| {
        cmd.env("LC_ALL", "C.UTF-8").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    }).map_err(|e| failure(spawn_message(argv, &e)))?;
    let stdout = drain(jailed.child.stdout.take().expect("piped stdout"));
    let stderr = drain(jailed.child.stderr.take().expect("piped stderr"));
    let end = Instant::now() + wait;
    let (mut out, mut err, mut exited) = (None, None, false);
    let reason = loop {
        if out.is_none() { out = stdout.try_recv().ok(); }
        if err.is_none() { err = stderr.try_recv().ok(); }
        if cancel.load(Ordering::Relaxed) { break "cancelled".to_string(); }
        if Instant::now() >= end { break format!("the tool took longer than {} seconds", wait.as_secs()); }
        if exited && out.is_some() && err.is_some() { break String::new(); }
        if exited { std::thread::sleep(STEP); continue; }
        match child::wait_for_exit(&jailed.child, end.min(Instant::now() + STEP)) {
            child::Woke::Exited => exited = true,
            child::Woke::Deadline => {},
            child::Woke::Failed(e) => break crate::error::io_message(&e),
        }
    };
    if !reason.is_empty() {
        jail::kill_tree(&mut jailed);
        let out_done = out.is_some() || stdout.recv_timeout(DRAIN_WAIT).is_ok();
        let err_done = err.is_some() || stderr.recv_timeout(DRAIN_WAIT).is_ok();
        return Err(Failure { message: reason, lingering: !out_done || !err_done });
    }
    let status = jailed.child.wait().map_err(|e| failure(crate::error::io_message(&e)))?;
    let out = out.unwrap().map_err(failure)?;
    let err = err.unwrap().map_err(failure)?;
    let said = String::from_utf8_lossy(&err);
    if !status.success() {
        // ImageMagick's failed fallback can end with a PNG write error that hides the missing encoder.
        let diagnostic = said.lines().find(|line| line.contains("no encode delegate for this image format")).unwrap_or(&said);
        return Err(failure(super::archivework::failure_message("conversion or print", &status, diagnostic)));
    }
    if out.len() > OUTPUT_CAP || err.len() > OUTPUT_CAP { return Err(failure("the tool returned too much output".into())); }
    String::from_utf8(out).map_err(|_| failure("the tool returned invalid text".into()))
}

pub fn boxed(argv: Vec<String>, work: &mut Work, cancel: &AtomicBool) -> Result<String, String> {
    if !sandbox::available() { return Err(super::archivework::SANDBOX_MISSING.into()); }
    let mut full = sandbox::wrap_compile(&argv, &[], &work.dir);
    sandbox::add_status(&mut full, argv.len());
    run(&full, cancel, TOOL_WAIT).map_err(|failure| {
        if failure.lingering { work.keep(); }
        failure.message
    })
}

// Copies have fixed, ordinary names: ImageMagick cannot interpret a source's brackets or coder prefix.
pub fn stage(source: &Path, dest: &Path) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut input = super::regfile::open_if_regular(source, 0).map_err(|e| crate::error::io_message(&e))?;
    let mut output = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(dest)
        .map_err(|e| crate::error::io_message(&e))?;
    std::io::copy(&mut input, &mut output).map_err(|e| crate::error::io_message(&e))?;
    Ok(())
}

pub fn publish(staged: &Path, dest: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let meta = staged.symlink_metadata().map_err(|_| "the converter wrote nothing".to_string())?;
    if !meta.is_file() || meta.len() == 0 { return Err("the converter wrote no regular output file".into()); }
    // Issue 370: a stage workhome kept off a copy-fallback mount lands by the exclusive copy, which the job's cancel stops.
    super::renamecompat::rename_cancellable(staged, dest, cancel).map_err(|e| e.msg)
}

pub fn inputs(paths: &[PathBuf]) -> Result<(), String> {
    use super::printconvert::{MIN_ITEMS, MAX_ITEMS};
    if !(MIN_ITEMS..=MAX_ITEMS).contains(&paths.len()) { return Err(format!("Select between {MIN_ITEMS} and {MAX_ITEMS} items.")); }
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if !path.is_absolute() || path.file_name().is_none() || path.components().any(|c| c == std::path::Component::ParentDir) {
            return Err("Absolute file paths are required.".into());
        }
        if !seen.insert(path) { return Err("An item was selected more than once.".into()); }
        super::regfile::open_if_regular(path, 0).map_err(|e| format!("{}: {}", path.display(), crate::error::io_message(&e)))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "printtool_tests.rs"]
mod tests;
