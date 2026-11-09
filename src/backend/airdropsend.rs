// One AirDrop send: `owly send` (https://github.com/t4t5/owly, MIT) as a job with owly's byte progress, cancel and the device's answer.
use crate::backend::menu_actions::Selected;
use crate::backend::opsreq::OpMsg;
use flea::ui_logic_sd1::airdrop::{self, Verdict};
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(100);
// A shell reports a run killed by signal N as exit 128 + N.
const SIGNAL_EXIT_BASE: i32 = 128;
const READ_CHUNK: usize = 4096;

pub(crate) struct Send {
    // The service ID `owly browse` printed, which `owly send --to` takes.
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

fn owly() -> Option<String> {
    airdrop::find(&std::env::var_os("PATH").unwrap_or_default())
}

// owly redraws "\r<sent> / <total> bytes" on stderr; every step becomes the job's byte count, the rest is kept for the reason.
fn read_progress(
    mut stderr: impl Read,
    id: usize,
    total: usize,
    file: String,
    tx: Sender<OpMsg>,
) -> String {
    let (mut kept, mut step, mut buf) = (String::new(), Vec::new(), [0u8; READ_CHUNK]);
    while let Ok(n) = stderr.read(&mut buf) {
        if n == 0 {
            break;
        }
        for &byte in &buf[..n] {
            if byte != b'\r' && byte != b'\n' {
                step.push(byte);
                continue;
            }
            let text = String::from_utf8_lossy(&step).into_owned();
            step.clear();
            match airdrop::progress(&text) {
                Some((bytes, total_bytes)) => {
                    let _ = tx.send(OpMsg::JobEntries {
                        id,
                        done: 0,
                        total,
                        file: file.clone(),
                        bytes,
                        total_bytes,
                        item_bytes: bytes,
                    });
                }
                None if !text.trim().is_empty() => {
                    kept.push_str(&text);
                    kept.push('\n');
                }
                None => {}
            }
        }
    }
    kept.push_str(&String::from_utf8_lossy(&step));
    kept
}

// The verdict's code and reason; a cancel kills owly and ends as cancelled whatever it printed.
pub(crate) fn run(
    program: Option<String>,
    id: usize,
    send: &Send,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
) -> (String, String) {
    let Some(program) = program else {
        return ("missing".into(), String::new());
    };
    let argv = airdrop::send_argv(&program, &send.peer, &send.paths);
    let spawned = crate::backend::locale::command(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
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
    let (stderr, total, progress_tx) = (child.stderr.take(), send.paths.len(), tx.clone());
    let reader = std::thread::spawn(move || {
        stderr
            .map(|s| read_progress(s, id, total, file, progress_tx))
            .unwrap_or_default()
    });
    let mut stdout = child.stdout.take();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return ("cancelled".into(), String::new());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(POLL),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return ("failed".into(), crate::error::io_message(&error));
            }
        }
    };
    let mut out = String::new();
    if let Some(stdout) = stdout.as_mut() {
        let _ = stdout.read_to_string(&mut out);
    }
    let err = reader.join().unwrap_or_default();
    use std::os::unix::process::ExitStatusExt;
    let code = status
        .code()
        .or_else(|| status.signal().map(|s| SIGNAL_EXIT_BASE + s))
        .unwrap_or(-1);
    match airdrop::verdict(&out, &err, code) {
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
        "missing" => "AirDrop needs the owly package.".into(),
        _ => "AirDrop could not finish the transfer.".into(),
    }
}

pub(crate) fn background(
    id: usize,
    send: Send,
    cancel: std::sync::Arc<AtomicBool>,
    tx: Sender<OpMsg>,
) {
    let sources: Vec<PathBuf> = send.paths.iter().map(PathBuf::from).collect();
    let (code, reason) =
        match crate::backend::menu_actions::validate_sources(Some(&send.items), &sources) {
            Ok(()) => run(owly(), id, &send, &cancel, &tx),
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
