// A 7z password travels on the child's stdin and never in argv: only a compress carries a bare `-p`.
use crate::backend::archive::{Formats, Reader, STORE_LINKS, ZIP_TYPE};
use crate::backend::archivework::{cancel_failure, failure_message, spawn_stderr_reader, Work, SANDBOX_MISSING};
use crate::backend::opsreq::op_err;
use crate::backend::sandbox;
use crate::error::{from_io, FleaError};
use std::io::Write;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// The code the backend answers when an archive needs a password the request did not carry.
pub const ENCRYPTED: &str = "encrypted";
// The code the backend answers when the password did not open the archive.
pub const WRONG_PASSWORD: &str = "wrong-password";
// 7z exits 2 on a wrong password and on damaged encrypted data alike, and gives no finer signal.
const WRONG_PASSWORD_CODE: i32 = 2;
// 7z exits 255 (Break signaled) when it must ask for a password and stdin is closed.
pub(crate) const NEEDS_PASSWORD_CODE: i32 = 255;
// Bare, so a compress reads the secret off stdin rather than off argv.
const PASSWORD_PROMPT: &str = "-p";
// Hides the entry names of a password .7z.
const HEADER_ENCRYPTION: &str = "-mhe=on";
// A password zip is AES-256, never the broken default ZipCrypto.
const ZIP_AES: &str = "-mem=AES256";
// A cancel is seen within this.
const PW_POLL: Duration = Duration::from_millis(50);
// Only 7z's reader takes a password; any other reader is refused with this sentence and never sees the secret.
pub const NO_PASSWORD_READER: &str = "only an archive that 7-Zip reads can take a password";

// A compress prompts on stdin only when -p is bare, so the secret never rides argv.
pub fn compress_argv(format: &str, staged: &Path, parent: &Path, names: &[String], encrypt_names: bool) -> Vec<String> {
    let mut argv = vec!["7z".to_string(), "a".to_string(), PASSWORD_PROMPT.to_string(), "-bd".to_string(),
                        "-spd".to_string(), STORE_LINKS.to_string()];
    if format == "zip" {
        argv.extend([ZIP_TYPE.to_string(), ZIP_AES.to_string()]);
    } else if encrypt_names {
        argv.push(HEADER_ENCRYPTION.to_string());
    }
    argv.push(staged.to_string_lossy().to_string());
    argv.extend(names.iter().map(|n| parent.join(n).to_string_lossy().to_string()));
    argv
}

pub fn reads_password(formats: &Formats, archive: &Path) -> bool {
    formats.seven_reads(archive)
}

// Whether the extract's argv is 7z's: a zip bsdtar reads still goes to 7z once a password is in hand.
pub fn runs_on_seven(formats: &Formats, archive: &Path, password: Option<&str>) -> bool {
    // archiveops::extract refused any password 7z cannot read before this, so a password alone picks 7z.
    password.is_some() || formats.reader_for(archive) == Some(Reader::SevenZip)
}

// An extract carries no -p: 7z reads a bare one as an empty password and never asks on stdin.
pub fn extract_argv(formats: &Formats, archive: &Path, staged: &Path) -> Option<Vec<String>> {
    // archiveops::extract refused a disk image before this, so only the 7z reader is checked here.
    reads_password(formats, archive).then(|| formats.seven_extract(archive, staged))
}

// A failed extract's code; 7z exits 2 on a non-archive too, so wrong-password needs `guarded` to prove a password.
pub fn extract_verdict(code: Option<i32>, with_password: bool, guarded: impl FnOnce() -> Option<bool>) -> Option<&'static str> {
    match (code, with_password) {
        (Some(WRONG_PASSWORD_CODE), true) => (guarded() == Some(true)).then_some(WRONG_PASSWORD),
        (Some(NEEDS_PASSWORD_CODE), false) => Some(ENCRYPTED),
        _ => None,
    }
}

// Whether a failed extract's code asks the card for a password, so the line names the archive to retry.
pub fn asks_again(err: &str) -> bool {
    err == ENCRYPTED || err == WRONG_PASSWORD
}

// The terminal line for a password code, naming the archive and destination the card retries.
pub fn asks_again_line(id: usize, err: &str, archive: &Path, dest: &Path) -> String {
    use crate::json::escape;
    format!(r#"{{"t":"archivedone","id":{},"ok":false,"verified":true,"err":"{}","path":"{}","dest":"{}"}}"#,
            id, escape(err), escape(&archive.to_string_lossy()), escape(&dest.to_string_lossy()))
}

// A failed run: the exit code the verdict reads, and the tool's own sentence for any other failure.
#[derive(Debug)]
pub struct SevenFailure {
    pub code: Option<i32>,
    pub message: String,
}

// The secret on stdin, newline-terminated the way an answered prompt is, then stdin closed.
pub fn answer_prompt(child: &mut Child, secret: &str) -> std::io::Result<()> {
    if let Some(mut stdin) = child.stdin.take() {
        let mut line = secret.as_bytes().to_vec();
        line.push(b'\n');
        stdin.write_all(&line)?;
    }
    Ok(())
}

// 7z reads one stdin line, so a line break would lock the archive with only the text before it.
pub(crate) fn refuse_line_break(what: &str, archive: &str, secret: Option<&str>) -> Result<(), FleaError> {
    if secret.is_some_and(|s| s.contains(['\n', '\r'])) {
        return Err(op_err(what, archive, &format!("the password for {archive} cannot hold a line break")));
    }
    Ok(())
}

// The archive jail around 7z writing the archive named; a secret rides its stdin, None leaves stdin closed. Ok(None) is a clean exit.
pub fn run_boxed_cancellable_password(
    what: &str,
    archive: &str,
    inner: Vec<String>,
    read_only: &Path,
    work: &mut Work,
    secret: Option<&str>,
    cancel: &AtomicBool,
) -> Result<Option<SevenFailure>, FleaError> {
    refuse_line_break(what, archive, secret)?;
    run_with(what, inner, read_only, work, secret, cancel, answer_prompt)
}

// The stdin write is a parameter so a test can fail it without a race.
fn run_with(
    what: &str,
    inner: Vec<String>,
    read_only: &Path,
    work: &mut Work,
    secret: Option<&str>,
    cancel: &AtomicBool,
    write: fn(&mut Child, &str) -> std::io::Result<()>,
) -> Result<Option<SevenFailure>, FleaError> {
    if !sandbox::available() {
        let tool = inner.first().map_or("", |s| s.as_str());
        return Err(op_err(what, tool, SANDBOX_MISSING));
    }
    let mut full = sandbox::wrap_archive(&inner, read_only, &work.dir);
    sandbox::add_status(&mut full, inner.len());
    let mut jailed = crate::backend::jail::spawn_jailed(&full, |cmd| {
        cmd.stdin(if secret.is_some() { Stdio::piped() } else { Stdio::null() });
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());
    })
    .map_err(|e| from_io(what, &full[0], &e))?;
    if let Some(secret) = secret {
        // A child that left before reading fails this write; its own exit below is the answer, not a cancel.
        let _ = write(&mut jailed.child, secret);
    }
    let reader = spawn_stderr_reader(jailed.child.stderr.take());
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancel_failure(what, &mut jailed, reader, work));
        }
        match jailed.child.try_wait() {
            Ok(Some(status)) => {
                let text = reader.join().unwrap_or_default();
                if status.success() {
                    return Ok(None);
                }
                return Ok(Some(SevenFailure { code: status.code(), message: failure_message(what, &status, &text) }));
            }
            Ok(None) => std::thread::sleep(PW_POLL),
            Err(e) => {
                crate::backend::jail::kill_tree(&mut jailed);
                reader.join().ok();
                return Err(from_io(what, &full[0], &e));
            }
        }
    }
}

// Extraction loops own the nonblocking password pipe, so dropping it never waits for a writer.
pub(crate) struct Prompt {
    input: std::process::ChildStdin,
    line: Vec<u8>,
    written: usize,
}

impl Prompt {
    pub(crate) fn new(child: &mut Child, secret: Option<&str>) -> std::io::Result<Option<Self>> {
        use std::os::fd::AsRawFd;
        extern "C" {
            fn fcntl(fd: i32, command: i32, ...) -> i32;
        }
        const F_GETFL: i32 = 3;
        const F_SETFL: i32 = 4;
        let Some((input, secret)) = child.stdin.take().zip(secret) else {
            return Ok(None);
        };
        let fd = input.as_raw_fd();
        let flags = unsafe { fcntl(fd, F_GETFL) };
        if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | crate::oflags::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut line = secret.as_bytes().to_vec();
        line.push(b'\n');
        Ok(Some(Self {
            input,
            line,
            written: 0,
        }))
    }

    // One write per poll leaves the caller's deadline and Cancel active even when stdin stays full.
    pub(crate) fn advance(prompt: &mut Option<Self>) {
        let complete = prompt.as_mut().is_some_and(|prompt| {
            match prompt.input.write(&prompt.line[prompt.written..]) {
                Ok(count) => {
                    prompt.written += count;
                    count == 0 || prompt.written == prompt.line.len()
                }
                Err(error) => !matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ),
            }
        });
        if complete {
            *prompt = None;
        }
    }
}

#[cfg(test)]
#[path = "archivepass_tests.rs"]
mod tests;
