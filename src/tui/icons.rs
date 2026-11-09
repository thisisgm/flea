use super::model::Row;
use std::io::Read;
use std::os::{fd::AsRawFd, unix::process::CommandExt};
use std::process::{Command, Stdio};
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc};
use std::time::{Duration, Instant};

pub const FOLDER: &str = "\u{ea83}";
pub const FILE: &str = "\u{ea7b}";
pub const TEXT: &str = "\u{ec5e}";
pub const LOCK: &str = "\u{ea75}";
pub const FILES: &str = "\u{eaf0}";
pub fn row(row: &Row, available: bool) -> &'static str {
    let folder = row.directory || row.icon == "folder";
    if !available {
        return if folder { "›" } else { " " };
    }
    if !row.link.is_empty() {
        return if folder { "\u{eaed}" } else { "\u{eaee}" };
    }
    if row.directory {
        return FOLDER;
    }
    let extension = std::path::Path::new(&*row.shown())
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => "\u{e68b}",
        "py" | "pyw" => "\u{e606}",
        "md" | "markdown" => "\u{eb1d}",
        "js" | "mjs" | "jsx" => "\u{e60c}",
        "ts" | "tsx" => "\u{e628}",
        "json" => "\u{e60b}",
        "sh" | "bash" | "zsh" | "fish" => "\u{e691}",
        "c" | "h" => "\u{e649}",
        "cpp" | "cc" | "cxx" | "hpp" => "\u{e646}",
        "go" => "\u{e627}",
        "html" | "htm" => "\u{e60e}",
        "css" | "scss" => "\u{e614}",
        "lua" => "\u{e620}",
        "toml" => "\u{e615}",
        "yaml" | "yml" => "\u{e6a8}",
        "pdf" => "\u{eaeb}",
        "xls" | "xlsx" | "ods" | "csv" => "\u{ebb7}",
        "ppt" | "pptx" | "odp" => "\u{f0428}",
        "stl" | "obj" | "3mf" | "step" | "stp" | "gcode" | "bgcode" => "\u{eb29}",
        "dat" | "bin" => "\u{eae8}",
        _ if row.icon.starts_with("audio") => "\u{ec1b}",
        _ if row.icon.starts_with("video") => "\u{ead9}",
        _ if row.icon.starts_with("image") => "\u{eaea}",
        _ if row.icon.contains("package") || matches!(extension.as_str(), "zip" | "tar" | "gz" | "xz" | "7z" | "rar") => "\u{eaef}",
        _ if row.icon.starts_with("text") || matches!(extension.as_str(), "txt" | "doc" | "docx" | "odt" | "rtf") => TEXT,
        _ if row.icon.contains("executable") || row.mode & 0o111 != 0 => "\u{eae8}",
        _ => FILE,
    }
}

pub fn builtin() -> bool {
    ["KITTY_WINDOW_ID", "GHOSTTY_RESOURCES_DIR", "GHOSTTY_BIN_DIR"].iter().any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

// Font coverage and terminfo are launch probes, never per-row processes.
pub struct Probe {
    pub result: mpsc::Receiver<(bool, super::theme::Depth)>,
    cancel: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Probe {
    pub fn start() -> Self {
        let (tx, result) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let worker = std::thread::spawn(move || {
            let mut command = Command::new("fc-list");
            command.args(["--quiet", ":charset=ea83"]);
            let marks = output(&mut command, &stopped).is_some() || builtin();
            let depth = super::theme::Depth::detect(&stopped);
            let _ = tx.send((marks, depth));
        });
        Self { result, cancel, worker: Some(worker) }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}
const F_SETFL: i32 = 4;
const SIGKILL: i32 = 9;
const PROBE_BYTES: usize = 4096;
const READ_BYTES: usize = 1024;
const PROBE_DEADLINE: Duration = Duration::from_millis(150);
const PROBE_POLL: Duration = Duration::from_millis(5);

#[derive(Debug, PartialEq, Eq)]
enum Stop { Failed, Refused, OutputLimit, Cancelled, Deadline }

pub(super) fn output(command: &mut Command, cancel: &AtomicBool) -> Option<String> {
    collect_output(command, cancel).ok()
}
fn collect_output(command: &mut Command, cancel: &AtomicBool) -> Result<String, Stop> {
    extern "C" { fn fcntl(fd: i32, command: i32, ...) -> i32; }
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).process_group(0).spawn().map_err(|_| Stop::Failed)?;
    let mut pipe = child.stdout.take().unwrap();
    if unsafe { fcntl(pipe.as_raw_fd(), F_SETFL, crate::oflags::O_NONBLOCK) } < 0 {
        let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
        let _ = child.wait();
        return Err(Stop::Failed);
    }
    let start = Instant::now();
    let mut bytes = Vec::new();
    let outcome = loop {
        let mut chunk = [0; READ_BYTES];
        match pipe.read(&mut chunk) {
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
            Err(_) => break Err(Stop::Failed),
        }
        if bytes.len() > PROBE_BYTES { break Err(Stop::OutputLimit); }
        if cancel.load(Ordering::Relaxed) { break Err(Stop::Cancelled); }
        if start.elapsed() >= PROBE_DEADLINE { break Err(Stop::Deadline); }
        // owner_live peeks with WNOWAIT, so an exited leader stays unreaped and its group id cannot be reused before the signal below.
        if !crate::clip::own::owner_live(child.id()) { break Ok(()); }
        std::thread::sleep(PROBE_POLL);
    };
    let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL);
    let status = child.wait().map_err(|_| Stop::Failed)?;
    outcome?;
    if !status.success() { return Err(Stop::Refused); }
    let _ = (&mut pipe).take((PROBE_BYTES + 1 - bytes.len()) as u64).read_to_end(&mut bytes);
    if bytes.len() > PROBE_BYTES { return Err(Stop::OutputLimit); }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
