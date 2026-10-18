// The flea-udisks helper client: one process per flash, the descriptor back over a socket pair.
use crate::backend::flashop::FlashReq;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::raw::c_int;
use std::time::{Duration, Instant};

// The packaged helper beside flea-gio-auth; FLEA_UDISKS overrides it, which is also the test seam.
const HELPER_PATH: &str = "/usr/lib/flea/flea-udisks";
// A drives or eject answer waits this long.
const HELPER_TIMEOUT: Duration = Duration::from_secs(30);
// The open leg waits on the user's own prompt, so it waits longer, and then ends the helper.
const OPEN_TIMEOUT: Duration = Duration::from_secs(600);
// How often a wait on a child's exit looks again.
const POLL_STEP: Duration = Duration::from_millis(20);
// One read off the helper's stderr pipe.
const DRAIN_BYTES: usize = 4096;
// The longest sentence of the helper's text kept for the log.
const ELIDE_BYTES: usize = 160;
// poll(2) POLLIN, one fd, EINTR retries; the only waiter this file needs.
const POLLIN: i16 = 1;
const EINTR: i32 = 4;
#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}
extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: c_int) -> c_int;
}

// What an open can end in before any byte moves; flashop maps each to its flashdone err.
pub enum OpenFail {
    Denied,
    Busy,
    Changed,
    Helper(String),
}

// The one-byte answers the helper sends over the socket, with a descriptor only on ok.
const STATUS_OK: u8 = b'O';
const STATUS_DENIED: u8 = b'D';
const STATUS_BUSY: u8 = b'B';
const STATUS_CHANGED: u8 = b'C';

pub fn helper_path() -> String {
    let over = std::env::var("FLEA_UDISKS").unwrap_or_default();
    if over.is_empty() {
        return HELPER_PATH.to_string();
    }
    over
}

// Under the bench feature the helper learns the marked root, so it may call a loop inside it a drive; any other build adds nothing.
fn seam_args(cmd: &mut std::process::Command, loop_root: Option<&std::path::Path>) {
    #[cfg(feature = "flash-test-seam")]
    {
        if let Some(root) = loop_root.and_then(crate::backend::flashseam::marked) {
            cmd.arg("--loop-root").arg(root);
        }
    }
    #[cfg(not(feature = "flash-test-seam"))]
    {
        let _ = (cmd, loop_root);
    }
}

// The helper run in drives mode for these disks, its stdout, or None when it did not run or answer.
fn drives_text(path: &str, devs: &[String], limit: Duration, loop_root: Option<&std::path::Path>) -> Option<String> {
    let mut cmd = crate::backend::locale::command(path);
    cmd.arg("drives");
    seam_args(&mut cmd, loop_root);
    for dev in devs {
        cmd.arg(dev);
    }
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().ok()?;
    wait_output(&mut child, limit).ok()
}

// The drives that survive the daemon's own system-disk judgement; a helper that fails leaves all.
pub fn surviving(devs: &[String]) -> Vec<String> {
    match drives_text(&helper_path(), devs, HELPER_TIMEOUT, crate::backend::flasheligible::seam_root().as_deref()) {
        Some(text) => apply_drives(devs, &text),
        None => devs.to_vec(),
    }
}

// Sample input: {"drives":[{"dev":"sda","system":false}]}; None when it is no object with a list.
fn parse_drives(text: &str) -> Option<Vec<(String, Option<bool>)>> {
    let doc = crate::jsondoc::parse(text).ok()?;
    let list = doc.get("drives")?.as_array()?;
    Some(list.iter().filter_map(|entry| Some((entry.get("dev")?.as_str()?.to_string(), entry.get("system").and_then(|s| s.as_bool())))).collect())
}

// Drops the drives the daemon calls system disks, keeping every drive when the answer is unreadable.
fn apply_drives(devs: &[String], text: &str) -> Vec<String> {
    let Some(listed) = parse_drives(text) else { return devs.to_vec() };
    devs.iter().filter(|dev| !listed.iter().any(|(name, system)| name == *dev && *system != Some(false))).cloned().collect()
}

// Layer 2 at flash time: true only when the helper names this exact drive present and not a system disk.
pub fn confirm(path: &str, dev: &str) -> bool {
    confirm_at(path, dev, HELPER_TIMEOUT)
}

pub(crate) fn confirm_at(path: &str, dev: &str, limit: Duration) -> bool {
    let Some(text) = drives_text(path, &[dev.to_string()], limit, crate::backend::flasheligible::seam_root().as_deref()) else { return false };
    parse_drives(&text).is_some_and(|listed| listed.iter().any(|(name, system)| name == dev && *system == Some(false)))
}

// The descriptor behind a flash, or the flashdone word for why it never came; spawned runs once the helper is up.
pub(crate) fn open_target(path: &str, req: &FlashReq, spawned: &mut dyn FnMut()) -> Result<std::fs::File, &'static str> {
    match open_device_at(path, &req.dev, &req.serial, req.bytes, spawned, OPEN_TIMEOUT) {
        Ok(fd) => Ok(std::fs::File::from(fd)),
        Err(OpenFail::Denied) => Err("denied"),
        Err(OpenFail::Busy) => Err("busy"),
        Err(OpenFail::Changed) => Err("changed"),
        Err(OpenFail::Helper(e)) => {
            eprintln!("flea: the udisks helper failed ({e})");
            Err("helper")
        }
    }
}

// Asks polkit first, unmounts, locks, opens, and hands back the exclusive descriptor.
pub(crate) fn open_device_at(path: &str, dev: &str, serial: &str, bytes: u64, spawned: &mut dyn FnMut(), limit: Duration) -> Result<OwnedFd, OpenFail> {
    if !std::path::Path::new(path).is_file() {
        return Err(OpenFail::Helper(format!("{path} is not installed")));
    }
    let (ours, theirs) = crate::backend::fdpass::pair().map_err(|e| OpenFail::Helper(e.to_string()))?;
    // The command owns the peer end until it drops, so it drops at once: a dead helper then hangs up the socket.
    let mut child = {
        let mut cmd = std::process::Command::new(path);
        cmd.arg("open");
        seam_args(&mut cmd, crate::backend::flasheligible::seam_root().as_deref());
        cmd.arg(dev).arg(serial).arg(bytes.to_string());
        cmd.stdin(std::process::Stdio::from(theirs)).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
        cmd.spawn().map_err(|e| OpenFail::Helper(e.to_string()))?
    };
    spawned();
    let start = Instant::now();
    let stderr = child.stderr.take();
    let answer = wait_fd(&ours, limit);
    let timed_out = answer.is_none() && start.elapsed() >= limit;
    // The log drains beside the reap, so a helper that answers and then fills the stderr pipe cannot hold it.
    let drainer = std::thread::spawn(move || drain(stderr));
    // The reap stays inside the same bound: past it the helper is killed and reaped.
    let deadline = start + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_STEP),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let logged = drainer.join().unwrap_or_default();
    match answer {
        Some((STATUS_OK, mut fds)) if !fds.is_empty() => Ok(fds.remove(0)),
        Some((STATUS_DENIED, _)) => {
            eprintln!("flea: the password prompt for /dev/{dev} was dismissed ({})", elide(&logged));
            Err(OpenFail::Denied)
        }
        Some((STATUS_BUSY, _)) => {
            eprintln!("flea: /dev/{dev} would not unmount ({})", elide(&logged));
            Err(OpenFail::Busy)
        }
        Some((STATUS_CHANGED, _)) => Err(OpenFail::Changed),
        _ => {
            let why = if timed_out { "timed out".to_string() } else { elide(&logged) };
            eprintln!("flea: the udisks helper {path} failed to open /dev/{dev} ({why})");
            Err(OpenFail::Helper(elide(&logged)))
        }
    }
}

// Unmounts the drive's new volumes, then powers it off; the rail's own verdict still decides.
pub fn eject(dev: &str) -> Result<(), String> {
    eject_at(&helper_path(), dev, HELPER_TIMEOUT)
}

pub(crate) fn eject_at(path: &str, dev: &str, limit: Duration) -> Result<(), String> {
    let mut cmd = crate::backend::locale::command(path);
    cmd.arg("eject");
    seam_args(&mut cmd, crate::backend::flasheligible::seam_root().as_deref());
    cmd.arg(dev);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    // Every unanswered path answers the protocol word, so the caller never forwards a free sentence.
    let mut child = cmd.spawn().map_err(|e| {
        eprintln!("flea: the eject helper {path} did not start ({e})");
        "error".to_string()
    })?;
    match wait_output(&mut child, limit) {
        Ok(text) => parse_eject(&text),
        Err(why) => {
            eprintln!("flea: the eject helper {path} failed ({why})");
            Err("error".to_string())
        }
    }
}

// Sample input: {"ok":false,"err":"busy"} answers busy; anything else unreadable or unnamed is error.
fn parse_eject(text: &str) -> Result<(), String> {
    let Ok(doc) = crate::jsondoc::parse(text) else { return Err("error".to_string()) };
    if doc.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(());
    }
    let err = doc.get("err").and_then(|v| v.as_str()).unwrap_or_default();
    Err(["busy", "gone", "changed"].iter().find(|word| **word == err).unwrap_or(&"error").to_string())
}

// One elided sentence for the log, never the daemon's whole paragraph, cut on a character boundary.
fn elide(text: &str) -> String {
    let line = text.lines().rfind(|l| !l.trim().is_empty()).unwrap_or_default().trim().to_string();
    if line.len() <= ELIDE_BYTES {
        return line;
    }
    let cut = (0..=ELIDE_BYTES).rev().find(|at| line.is_char_boundary(*at)).unwrap_or(0);
    format!("{}...", &line[..cut])
}

fn drain(pipe: Option<std::process::ChildStderr>) -> String {
    use std::io::Read;
    let mut out = String::new();
    if let Some(mut pipe) = pipe {
        let mut buf = [0u8; DRAIN_BYTES];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
                Err(_) => break,
            }
        }
    }
    out
}

// A full wait with a deadline for helpers that answer on stdout; Err names why: the limit killed the child, or it exited badly.
fn wait_output(child: &mut std::process::Child, limit: Duration) -> Result<String, String> {
    use std::io::Read;
    let mut out = String::new();
    let mut pipe = child.stdout.take();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap_or(None) {
            if let Some(pipe) = pipe.as_mut() {
                let mut rest = Vec::new();
                let _ = pipe.read_to_end(&mut rest);
                out.push_str(&String::from_utf8_lossy(&rest));
            }
            return if status.success() { Ok(out) } else { Err(status.to_string()) };
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("timed out".to_string());
        }
        std::thread::sleep(POLL_STEP);
    }
}

// Waits for the helper's one answer: a hangup or the limit ends the wait with None, the prompt's own time included.
fn wait_fd(sock: &OwnedFd, limit: Duration) -> Option<(u8, Vec<OwnedFd>)> {
    let raw = sock.as_raw_fd();
    let started = Instant::now();
    loop {
        let left = limit.saturating_sub(started.elapsed());
        if left.is_zero() {
            return None;
        }
        let mut slot = PollFd { fd: raw, events: POLLIN, revents: 0 };
        let wait_ms = left.as_millis().clamp(1, c_int::MAX as u128) as c_int;
        let ready = unsafe { poll(&mut slot, 1, wait_ms) };
        if ready < 0 && std::io::Error::last_os_error().raw_os_error() == Some(EINTR) {
            continue;
        }
        if ready < 0 {
            return None;
        }
        if ready == 0 {
            continue;
        }
        return match crate::backend::fdpass::recv(raw) {
            Ok(Some(got)) if !got.payload.is_empty() => Some((got.payload[0], got.fds)),
            _ => None,
        };
    }
}

#[cfg(test)]
#[path = "flashhelper_tests.rs"]
mod tests;
