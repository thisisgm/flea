// LocalSend ships a CLI, and it is a full-screen program: it draws what it discovered in a panel and
// takes arrow keys and Enter. Directive 71 is that Flea drives that CLI rather than opening the app,
// so this opens a pty of its own and drives it there. No shell is involved at any point, which is
// what keeps a file name with a quote or a space in it an argument rather than somebody else's word.
use super::localsendtext::{parse_peers, refusal, strip_ansi, Peer};
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::raw::c_ulong;
use std::os::unix::process::CommandExt;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// The CLI the package installs. The bare "localsend" beside it is the GTK app and is never run here.
const COMMAND: &str = "localsend-cli";
// The panel is drawn to fit its terminal, so the pty is opened at a size that holds the device list
// rather than the zero-by-zero a pty starts at, which makes the CLI draw nothing at all.
const ROWS: u16 = 40;
const COLS: u16 = 120;
// Keys the panel answers to: its own footer says up and down navigate, Enter sends, and D is the
// global hotkey that opens the panel on a run that carries no files.
const KEY_SHOW_DEVICES: &[u8] = b"D";
const KEY_DOWN: &[u8] = b"\x1b[B";
const KEY_ENTER: &[u8] = b"\r";
// A device answers a discovery announcement in well under a second on this LAN; measured at 1.2 s
// from process start to the first row, most of which is the CLI's own start.
const POLL: Duration = Duration::from_millis(100);

// Linux ioctl numbers. TIOCSWINSZ gives the pty its size, TIOCSCTTY makes it the child's terminal.
const TIOCSWINSZ: usize = 0x5414;
const TIOCSCTTY: usize = 0x540E;
// prctl(2) PR_SET_PDEATHSIG ends the CLI when the thread that started it dies, SIGKILL included.
const PR_SET_PDEATHSIG: i32 = 1;
const SIGKILL: std::os::raw::c_ulong = 9;
const ESRCH: i32 = 3; // Linux errno.h, for a parent that died before PDEATHSIG could be armed.

#[repr(C)]
struct WinSize {
    rows: u16,
    cols: u16,
    x_pixels: u16,
    y_pixels: u16,
}

// std already links the system libc, so these are declared the way src/backend/child.rs declares its own.
extern "C" {
    fn posix_openpt(flags: i32) -> i32;
    fn grantpt(fd: i32) -> i32;
    fn unlockpt(fd: i32) -> i32;
    fn ptsname_r(fd: i32, buf: *mut u8, len: usize) -> i32;
    fn ioctl(fd: i32, request: usize, ...) -> i32;
    fn setsid() -> i32;
    fn getppid() -> i32;
    fn prctl(option: i32, ...) -> i32;
}

// The person reads the cause; the log names the step that met it, since several calls can refuse in the same way.
fn setup_fault(step: &str, cause: &std::io::Error) -> String { eprintln!("flea: LocalSend terminal setup, {}: {}", step, cause); format!("LocalSend could not start: {}.", crate::error::io_message(cause)) }

fn open_pty() -> Result<(OwnedFd, std::fs::File), String> {
    // O_RDWR | O_NOCTTY | O_CLOEXEC: the terminal belongs to the child, and the master must not ride into it.
    const O_RDWR_NOCTTY_CLOEXEC: i32 = 0o2 | 0o400 | 0o2000000;
    let raw = unsafe { posix_openpt(O_RDWR_NOCTTY_CLOEXEC) };
    if raw < 0 { return Err(setup_fault("posix_openpt", &std::io::Error::last_os_error())) }
    let master = unsafe { OwnedFd::from_raw_fd(raw) };
    if unsafe { grantpt(raw) } < 0 { return Err(setup_fault("grantpt", &std::io::Error::last_os_error())) }
    if unsafe { unlockpt(raw) } < 0 { return Err(setup_fault("unlockpt", &std::io::Error::last_os_error())) }
    let mut name = [0u8; 256];
    // ptsname_r answers its errno as the return value and leaves errno alone.
    let named = unsafe { ptsname_r(raw, name.as_mut_ptr(), name.len()) };
    if named != 0 { return Err(setup_fault("ptsname_r", &std::io::Error::from_raw_os_error(named))) }
    let end = name.iter().position(|b| *b == 0).unwrap_or(0);
    let path = String::from_utf8_lossy(&name[..end]).into_owned();
    let size = WinSize { rows: ROWS, cols: COLS, x_pixels: 0, y_pixels: 0 };
    if unsafe { ioctl(raw, TIOCSWINSZ, &size as *const WinSize) } < 0 {
        return Err(setup_fault("TIOCSWINSZ", &std::io::Error::last_os_error()))
    }
    let slave = std::fs::OpenOptions::new().read(true).write(true).open(&path)
        .map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    Ok((master, slave))
}

// No --port: measured on the box, a run on any other port hears nothing at all, because LocalSend
// announces to the multicast group on 53317 and a CLI bound elsewhere never receives those
// announcements. A box already running its own LocalSend is told so instead, by refusal() below.
// The program parameter is a test seam; every production caller names COMMAND.
fn spawn(program: &str, args: &[String], slave: &std::fs::File) -> Result<Child, String> {
    let stdin = slave.try_clone().map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    let stdout = slave.try_clone().map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    let stderr = slave.try_clone().map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    // The hidden terminal is parsed for device rows and English transfer status words.
    let mut command = crate::backend::locale::command(program);
    // A pty is not a terminal until something says which one: the backend inherits no TERM, and
    // without one the CLI draws its frame and never refreshes the device list inside it.
    command.args(args).env("TERM", "xterm-256color")
        .stdin(Stdio::from(stdin)).stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr));
    // Read before the fork, because a backend that died in between could never deliver the signal.
    let parent = std::process::id();
    unsafe {
        // A session of its own, then this pty as its controlling terminal: without one the CLI reads
        // no keys at all, and with Flea's own session it would take Flea's.
        command.pre_exec(move || {
            if prctl(PR_SET_PDEATHSIG, SIGKILL as c_ulong, 0 as c_ulong, 0 as c_ulong, 0 as c_ulong) < 0 { return Err(std::io::Error::last_os_error()) }
            if getppid() != parent as i32 { return Err(std::io::Error::from_raw_os_error(ESRCH)) }
            if setsid() < 0 { return Err(std::io::Error::last_os_error()) }
            if ioctl(0, TIOCSCTTY, 0) < 0 { return Err(std::io::Error::last_os_error()) }
            Ok(())
        });
    }
    command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("{} is not installed.", program),
        _ => format!("{} could not start: {}.", program, crate::error::io_message(&e)),
    })
}

// The reader owns the master for the length of the run: the CLI repaints constantly, so its output is
// drained on a thread of its own and the panel is read from what has arrived so far.
fn read_into(master: OwnedFd) -> Arc<Mutex<String>> {
    let seen = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&seen);
    std::thread::spawn(move || {
        let mut file = std::fs::File::from(master);
        let mut buf = [0u8; 4096];
        while let Ok(n) = file.read(&mut buf) {
            if n == 0 { break }
            if let Ok(mut held) = sink.lock() {
                held.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
    });
    seen
}

fn text_of(seen: &Arc<Mutex<String>>) -> String {
    match seen.lock() {
        Ok(held) => strip_ansi(&held),
        Err(_) => String::new(),
    }
}

// Keys go to the master, which is the terminal's input side. Writing them to the slave instead is a
// key the app never reads: it is the app's own output, so the write lands on its screen and nowhere
// else, which is why every press before this was silently lost.
fn press(master: &OwnedFd, keys: &[u8]) -> Result<(), String> {
    let mut writer = std::fs::File::from(master.try_clone().map_err(|e| format!("LocalSend stopped answering: {}.", crate::error::io_message(&e)))?);
    writer.write_all(keys).map_err(|e| format!("LocalSend stopped answering: {}.", crate::error::io_message(&e)))?;
    writer.flush().map_err(|e| format!("LocalSend stopped answering: {}.", crate::error::io_message(&e)))
}

// Sample event lines, exactly as the CLI prints them under its panel:
// "S Clean Lemon: Sent 1 file (61 B, took 0s)" and "D [1] Clean Lemon (192.168.21.23)".
// The prefix is the whole test, never a substring: a device named "Sent Phone" would otherwise be
// read as a completed transfer the moment it was discovered, and "Failed Phone" as a refusal.
pub fn sent(text: &str, name: &str) -> Option<Result<(), String>> {
    let head = format!("S {}: ", name);
    let line = text.lines().map(str::trim).rev().find(|l| l.starts_with(&head))?;
    let said = line[head.len()..].trim();
    if said.starts_with("Sent ") { return Some(Ok(())) }
    if said.starts_with("Failed") {
        return Some(Err(format!("LocalSend could not send to {}: {}.", name, said.trim_end_matches('.'))))
    }
    None
}

fn stop(child: &mut Child) {
    let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL as i32);
    let _ = child.kill();
    let _ = child.wait();
}

// The devices this box can see right now. A run that carries no files starts in receive mode, so the
// panel is asked for by its own D and the run ends as soon as the answer is in.
pub fn peers(limit: Duration) -> Result<Vec<Peer>, String> {
    let (master, slave) = open_pty()?;
    let mut child = spawn(COMMAND, &[], &slave)?;
    let keys = master.try_clone()
        .map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    let seen = read_into(master);
    let deadline = Instant::now() + limit;
    // The CLI has to be up before it has a key reader, so the panel is asked for after one poll.
    std::thread::sleep(POLL * 3);
    let _ = press(&keys, KEY_SHOW_DEVICES);
    loop {
        let found = parse_peers(&text_of(&seen));
        if !found.is_empty() {
            stop(&mut child);
            return Ok(found)
        }
        if let Some(refusal) = refusal(&text_of(&seen)) {
            stop(&mut child);
            return Err(refusal)
        }
        if Instant::now() >= deadline {
            stop(&mut child);
            return Ok(Vec::new())
        }
        std::thread::sleep(POLL);
    }
}

// One send, to the device the front end named, driven on the CLI's own terminal. Two things were
// measured here the hard way, both by driving it: keys go to the MASTER side of the pty, because the
// slave is the app's own output and a key written there is never read, and the device the CLI found
// arrives as an incremental screen update, so the stream is read whole rather than frame by frame.
// The row is reached with the arrows the CLI's own footer names, Enter sends, and the CLI says so.
pub(crate) fn send_cancelled(
    name: &str,
    paths: &[String],
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    send_with_cancel(name, paths, SEND_LIMIT, cancel)
}

pub fn send(name: &str, paths: &[String], limit: Duration) -> Result<(), String> {
    send_with_cancel(
        name,
        paths,
        limit,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

fn send_with_cancel(
    name: &str,
    paths: &[String],
    limit: Duration,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("LocalSend was given nothing to send.".into());
    }
    let mut args: Vec<String> = Vec::new();
    for path in paths {
        args.push("-f".into());
        args.push(path.clone());
    }
    let (master, slave) = open_pty()?;
    let mut child = spawn(COMMAND, &args, &slave)?;
    let keys = master.try_clone()
        .map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    let seen = read_into(master);
    let deadline = Instant::now() + limit;
    let mut index = None;
    while Instant::now() < deadline {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            stop(&mut child);
            return Err("Send cancelled.".into());
        }
        let text = text_of(&seen);
        if let Some(refusal) = refusal(&text) {
            stop(&mut child);
            return Err(refusal)
        }
        if let Some(peer) = parse_peers(&text).into_iter().find(|p| p.name == name) {
            index = Some(peer.index);
            break
        }
        std::thread::sleep(POLL);
    }
    let index = match index {
        Some(index) => index,
        None => {
            stop(&mut child);
            return Err(format!("{} is not answering on this network any more.", name))
        }
    };
    // The list opens on its first device, so the row wanted is that many presses further down.
    for _ in 1..index {
        press(&keys, KEY_DOWN)?;
        std::thread::sleep(POLL);
    }
    press(&keys, KEY_ENTER)?;
    // The CLI reports the transfer on its own event line, "S <device>: Sent 1 file (61 B, took 0s)",
    // and that line is the only success this side can honestly report.
    while Instant::now() < deadline {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            stop(&mut child);
            return Err("Send cancelled.".into());
        }
        let text = text_of(&seen);
        if let Some(verdict) = sent(&text, name) {
            stop(&mut child);
            return verdict
        }
        // The CLI ending is not a transfer. Only its own Sent line is one, so a run that ended
        // without printing one sent nothing, whatever its exit code was.
        if let Ok(Some(_)) = child.try_wait() {
            return match sent(&text_of(&seen), name) {
                Some(verdict) => verdict,
                None => Err(format!("LocalSend stopped before sending to {}.", name)),
            }
        }
        std::thread::sleep(POLL);
    }
    stop(&mut child);
    Err(format!("{} did not accept the transfer.", name))
}

// How long each leg may take. Discovery is a CLI start plus one announcement round trip, measured at
// 1.2 s on this box; a send waits for the other machine's own accept, which is a person.
const PEERS_LIMIT: Duration = Duration::from_secs(4);
const SEND_LIMIT: Duration = Duration::from_secs(45);

// The one line the client hears, for either leg. Sample: {"t":"localsendpeers","id":3,"peers":[
// {"name":"Clean Lemon","address":"192.168.21.23"}],"reason":""}
pub fn request(op: String, peer: String, paths: Vec<String>, id: usize, replies: std::sync::mpsc::Sender<crate::backend::opsreq::OpMsg>) {
    std::thread::spawn(move || {
        let line = answer(&op, &peer, &paths, id);
        let _ = replies.send(crate::backend::opsreq::OpMsg::Meta { line });
    });
}

pub fn answer(op: &str, peer: &str, paths: &[String], id: usize) -> String {
    if op == "send" {
        return match send(peer, paths, SEND_LIMIT) {
            Ok(()) => format!(r#"{{"t":"localsendsent","id":{},"ok":true,"reason":""}}"#, id),
            Err(reason) => format!(r#"{{"t":"localsendsent","id":{},"ok":false,"reason":"{}"}}"#, id, crate::json::escape(&reason)),
        }
    }
    match peers(PEERS_LIMIT) {
        Ok(found) => {
            let rows: Vec<String> = found.iter()
                .map(|p| format!(r#"{{"name":"{}","address":"{}"}}"#, crate::json::escape(&p.name), crate::json::escape(&p.address)))
                .collect();
            let reason = if rows.is_empty() { "no devices are answering on this network" } else { "" };
            format!(r#"{{"t":"localsendpeers","id":{},"peers":[{}],"reason":"{}"}}"#, id, rows.join(","), reason)
        }
        Err(reason) => format!(r#"{{"t":"localsendpeers","id":{},"peers":[],"reason":"{}"}}"#, id, crate::json::escape(&reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A review finding on the first drive that worked: a substring test reads a device named after
    // the event as the event itself, and a CLI that ended is not a CLI that sent.
    #[test]
    fn only_the_transfer_s_own_event_line_is_a_transfer() {
        let discovered = "D [1] Sent Phone (10.0.0.4)\nD [2] Failed Phone (10.0.0.5)";
        assert!(sent(discovered, "Sent Phone").is_none());
        assert!(sent(discovered, "Failed Phone").is_none());
        assert!(sent("S Sent Phone: Sent 1 file (61 B, took 0s)", "Sent Phone").expect("a verdict").is_ok());
        let refused = sent("S Clean Lemon: Failed to send: connection refused", "Clean Lemon").expect("a verdict");
        assert!(refused.unwrap_err().contains("connection refused"));
        // The peer's own name is what anchors it, so another device's line is not this one's.
        assert!(sent("S Other Box: Sent 1 file (61 B, took 0s)", "Clean Lemon").is_none());
    }

    #[test]
    fn nothing_to_send_is_refused_before_a_terminal_is_opened() {
        assert!(send("Clean Lemon", &[], Duration::from_millis(1)).is_err());
    }

    // A child that inherited the pty master kept the terminal alive after the backend died.
    #[test]
    fn the_cli_does_not_inherit_the_terminal_master() {
        let (master, slave) = open_pty().expect("a pty");
        let mut child = spawn("/usr/bin/sleep", &["600".to_string()], &slave).expect("a stand-in CLI");
        // O_CLOEXEC closes the descriptor at exec, so the scan waits until the stand-in is itself.
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::fs::read_to_string(format!("/proc/{}/comm", child.id())).map(|c| c.trim() != "sleep").unwrap_or(true) {
            assert!(Instant::now() < deadline, "the stand-in CLI never exec'd");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(child.try_wait().expect("try_wait").is_none(), "the stand-in CLI exited before the scan");
        let mut inherited = Vec::new();
        for fd in std::fs::read_dir(format!("/proc/{}/fd", child.id())).expect("the child's descriptor table") {
            let fd = fd.expect("a descriptor entry");
            // sleep closes its locale files right after exec, so a listed descriptor can vanish; an inherited master never would.
            let target = match std::fs::read_link(fd.path()) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                other => other.expect("a descriptor target"),
            };
            if target.to_string_lossy().contains("ptmx") { inherited.push(fd.path()) }
        }
        stop(&mut child);
        drop(master);
        assert!(inherited.is_empty(), "the CLI holds the terminal master at {:?}", inherited);
    }
    const LIFECYCLE_MARKER: &str = "FLEA_LOCALSEND_LIFECYCLE_PROBE";
    const LIFECYCLE_TEST: &str = "backend::localsend::tests::a_child_cli_does_not_outlive_its_parent";

    // Field 3 (state) and field 22 (start time) of /proc/pid/stat, split after the last ')' so a comm cannot shift them.
    // Sample input: "4242 (localsend) S 4100 4242 4100 0 -1 4194560 812 0 0 0 3 1 0 0 20 0 4 0 9876543 ..." -> ("S", 9876543).
    fn proc_stat(pid: i32) -> Option<(String, u64)> {
        let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(ESRCH) => return None,
            other => other.unwrap_or_else(|e| panic!("pid {pid} exists but its stat is unreadable: {e}")),
        };
        let mut fields = stat.rsplit(')').next().expect("a stat tail").split_whitespace();
        let state = fields.next().expect("a stat state").to_string();
        Some((state, fields.nth(18).expect("a stat start time").parse().expect("a numeric start time")))
    }

    // A reused pid has another start time and a zombie waits to be reaped, so neither is a CLI that survived.
    fn child_running(pid: i32, start: u64) -> bool {
        proc_stat(pid).is_some_and(|(state, began)| state != "Z" && began == start)
    }

    #[test]
    fn a_live_pid_with_another_start_time_is_not_the_child() {
        let mut sleeper = std::process::Command::new("sleep").arg("5").spawn().expect("a sleep");
        let pid = sleeper.id() as i32;
        let start = proc_stat(pid).expect("the sleep's stat").1;
        let (other, own) = (child_running(pid, start + 1), child_running(pid, start));
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert!(!other, "a live pid with another start time was read as the child");
        assert!(own, "a live pid with its own start time was not read as the child");
    }

    // The probe ends with a stand-in behind, like a killed backend; only PDEATHSIG can end the HUP-ignoring child.
    #[test]
    fn a_child_cli_does_not_outlive_its_parent() {
        if let Some(marker) = std::env::var_os(LIFECYCLE_MARKER) {
            let marker = std::path::PathBuf::from(marker);
            let ready = marker.with_extension("ready");
            let (_master, slave) = open_pty().expect("a pty");
            let child = spawn("/bin/sh", &["-c".to_string(), "trap '' HUP; : > \"$1\"; exec sleep 600".to_string(),
                "localsend-life".to_string(), ready.to_string_lossy().into_owned()], &slave)
                .expect("the probe starts a stand-in CLI");
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready.exists() {
                assert!(Instant::now() < deadline, "the stand-in never armed its HUP ignore");
                std::thread::sleep(Duration::from_millis(10));
            }
            let start = proc_stat(child.id() as i32).expect("the stand-in's stat").1;
            std::fs::write(&marker, format!("{} {}", child.id(), start)).expect("the probe writes its child's identity");
            // The stand-in must outlive this probe, so its handle is relinquished rather than reaped.
            std::mem::forget(child);
            return;
        }
        let dir = crate::backend::testdir::TestDir::new("localsend-life");
        let marker = dir.path().join("child.id");
        let out = std::process::Command::new(std::env::current_exe().expect("a test binary knows its own path"))
            .args(["--exact", "--test-threads=1", "--nocapture", LIFECYCLE_TEST])
            .env(LIFECYCLE_MARKER, &marker)
            .output()
            .expect("the test binary re-executes");
        let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "the probe child failed: {}", report);
        // Renaming the test would filter the child to nothing and exit 0, so it has to say it ran one.
        assert!(report.contains("1 passed"), "the probe child ran no test: {}", report);
        let identity = std::fs::read_to_string(&marker).expect("the probe wrote its child's identity");
        let (pid, start) = identity.split_once(' ').expect("a pid and a start time");
        let (pid, start): (i32, u64) = (pid.parse().expect("a pid"), start.trim().parse().expect("a start time"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while child_running(pid, start) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        // Only the pid that started at `start` is killed, so a reused pid is never touched.
        if child_running(pid, start) {
            let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
            let killed = Instant::now() + Duration::from_secs(5);
            while child_running(pid, start) && Instant::now() < killed {
                std::thread::sleep(Duration::from_millis(25));
            }
            panic!("a CLI survived the backend's death: pid {pid}");
        }
    }
}

#[cfg(test)]
#[path = "localsend_wording_tests.rs"]
mod wording;
