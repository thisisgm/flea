// One AirDrop send: `owly send` (https://github.com/t4t5/owly, MIT), or `omdrop send` when owly is absent, as a job with cancel and the device's answer.
use crate::backend::menu_actions::Selected;
use crate::backend::opsreq::OpMsg;
use flea::ui_logic_send_share::airdrop::{self, Verdict};
use flea::ui_logic_send_share::omdrop::{self, Stack, Tool};
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(100);
// A cancelled `omdrop send` takes its sender's process group down on SIGTERM; this long, then it is killed.
const TERM_GRACE: Duration = Duration::from_secs(3);
// A shell reports a run killed by signal N as exit 128 + N.
const SIGNAL_EXIT_BASE: i32 = 128;
const READ_CHUNK: usize = 4096;
// One drain reads at most this much, so a child that never stops writing cannot hold off cancel or the deadlines.
const MAX_READ_TURN: usize = 64 * 1024;

pub(crate) struct Send {
    // The service ID `owly browse` printed, or the address `omdrop peers` did; either's `send --to` takes it.
    pub peer: String,
    pub paths: Vec<String>,
    pub request: usize,
    pub items: Vec<Selected>,
}

// Sample output: {"t":"airdropsent","id":4,"ok":false,"code":"declined","reason":""}
pub(crate) fn sent_line(request: usize, code: &str, reason: &str) -> String {
    format!(
        r#"{{"t":"airdropsent","id":{},"ok":{},"code":"{}","reason":"{}"}}"#,
        request,
        code == "delivered",
        code,
        crate::json::escape(reason)
    )
}

fn tool() -> Option<Tool> {
    omdrop::find(&std::env::var_os("PATH").unwrap_or_default())
}

// One send's stderr, read without blocking: owly's "\r<sent> / <total> bytes" steps become the job's byte count,
// every other line is kept for the reason.
struct Said {
    pipe: Option<std::process::ChildStderr>,
    step: Vec<u8>,
    kept: String,
    id: usize,
    total: usize,
    file: String,
    tx: Sender<OpMsg>,
}

impl Said {
    fn new(pipe: Option<std::process::ChildStderr>, id: usize, total: usize, file: String, tx: Sender<OpMsg>) -> Self {
        // A pipe left blocking would hold the job for as long as any descendant keeps stderr open.
        let pipe = pipe.filter(|p| crate::backend::nearbytool::nonblocking(p).is_ok());
        Self { pipe, step: Vec::new(), kept: String::new(), id, total, file, tx }
    }

    // Reads what the pipe holds now, up to one turn's worth; true once it has closed or failed.
    fn drain(&mut self) -> bool {
        let mut buf = [0u8; READ_CHUNK];
        let mut read = 0;
        while read < MAX_READ_TURN {
            let Some(pipe) = self.pipe.as_mut() else { return true };
            match pipe.read(&mut buf) {
                Ok(0) => return true,
                Ok(n) => {
                    self.feed(&buf[..n]);
                    read += n;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return false,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return true,
            }
        }
        false
    }

    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte != b'\r' && byte != b'\n' {
                self.step.push(byte);
                continue;
            }
            let text = String::from_utf8_lossy(&self.step).into_owned();
            self.step.clear();
            match airdrop::progress(&text) {
                Some((bytes, total_bytes)) => {
                    let _ = self.tx.send(OpMsg::JobEntries {
                        id: self.id,
                        done: 0,
                        total: self.total,
                        file: self.file.clone(),
                        bytes,
                        total_bytes,
                        item_bytes: bytes,
                    });
                }
                None if !text.trim().is_empty() => {
                    self.kept.push_str(&text);
                    self.kept.push('\n');
                }
                None => {}
            }
        }
    }

    // Reads until the pipe closes or `wait` passes, then closes it: a descendant holding it open delays the job
    // that long at most, and every line read before then is kept.
    fn finish(mut self, wait: Duration) -> String {
        let deadline = std::time::Instant::now() + wait;
        while !self.drain() && std::time::Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
        self.kept.push_str(&String::from_utf8_lossy(&self.step));
        self.kept
    }
}

// omdrop must hear SIGTERM to stop its sender's process group, which SIGKILL to the script would leave running.
fn stop(child: &mut std::process::Child, stack: Stack, said: &mut Said) {
    if stack == Stack::Omdrop {
        // The child is not reaped until try_wait sees it exit, so its pid is still its own.
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .stdin(Stdio::null())
            .status();
        let deadline = std::time::Instant::now() + TERM_GRACE;
        while std::time::Instant::now() < deadline {
            said.drain();
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(POLL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

// The verdict's code and reason; a cancel stops the send and ends as cancelled whatever it printed.
pub(crate) fn run(
    tool: Option<Tool>,
    id: usize,
    send: &Send,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
) -> (String, String) {
    let Some(Tool { stack, path: program }) = tool else {
        return ("missing".into(), String::new());
    };
    let argv = match stack {
        Stack::Owly => airdrop::send_argv(&program, &send.peer, &send.paths),
        Stack::Omdrop => omdrop::send_argv(&program, &send.peer, &send.paths),
    };
    // Omdrop's verdict is on stderr and its stdout can outgrow a pipe nobody reads until exit, so it goes nowhere.
    let stdout = match stack {
        Stack::Owly => Stdio::piped(),
        Stack::Omdrop => Stdio::null(),
    };
    let spawned = crate::backend::locale::command(&*flea::bytename::text_name(&argv[0]))
        .args(flea::bytename::args(&argv[1..]))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return ("failed".into(), crate::error::io_message(&error)),
    };
    let file = send
        .paths
        .first()
        .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
        .unwrap_or_default();
    let mut said = Said::new(child.stderr.take(), id, send.paths.len(), file, tx.clone());
    let mut stdout = child.stdout.take();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            stop(&mut child, stack, &mut said);
            said.finish(TERM_GRACE);
            return ("cancelled".into(), String::new());
        }
        said.drain();
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(POLL),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                said.finish(TERM_GRACE);
                return ("failed".into(), crate::error::io_message(&error));
            }
        }
    };
    let mut out = String::new();
    if let Some(stdout) = stdout.as_mut() {
        let _ = stdout.read_to_string(&mut out);
    }
    let err = said.finish(TERM_GRACE);
    use std::os::unix::process::ExitStatusExt;
    let code = status
        .code()
        .or_else(|| status.signal().map(|s| SIGNAL_EXIT_BASE + s))
        .unwrap_or(-1);
    let verdict = match stack {
        Stack::Owly => airdrop::verdict(&out, &err, code),
        Stack::Omdrop => omdrop::verdict(&err, code),
    };
    match verdict {
        Verdict::Delivered => ("delivered".into(), String::new()),
        Verdict::Declined => ("declined".into(), String::new()),
        Verdict::Failed(reason) => ("failed".into(), reason),
    }
}

// The failed job's sentence in the jobs list; backend job reasons are English, as LocalSend's "Send cancelled." is.
fn job_reason(code: &str, reason: &str) -> String {
    match code {
        _ if !reason.is_empty() => reason.to_string(),
        // The backend holds only the device's service ID; the window's status line names the device.
        "declined" => "The device declined the files.".into(),
        "cancelled" => "Send cancelled.".into(),
        "missing" => "AirDrop needs the owly or Omdrop package.".into(),
        _ => "AirDrop could not finish the transfer.".into(),
    }
}

pub(crate) fn background(
    id: usize,
    send: Send,
    cancel: std::sync::Arc<AtomicBool>,
    tx: Sender<OpMsg>,
) {
    let sources: Vec<PathBuf> = send.paths.iter().map(flea::bytename::exact).collect();
    let (code, reason) =
        match crate::backend::menu_actions::validate_sources(Some(&send.items), &sources) {
            Ok(()) => run(tool(), id, &send, &cancel, &tx),
            Err(error) => ("stale".into(), error),
        };
    let ok = code == "delivered";
    let _ = tx.send(OpMsg::JobDone {
        id,
        failed: if ok {
            Vec::new()
        } else {
            super::failures(&send.items)
        },
        error: if ok {
            String::new()
        } else {
            job_reason(&code, &reason)
        },
        line: sent_line(send.request, &code, &reason),
    });
}

#[cfg(test)]
#[path = "airdropsend_tests.rs"]
mod tests;
