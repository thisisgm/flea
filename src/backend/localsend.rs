// LocalSend ships a CLI, and it is a full-screen program: it draws what it discovered in a panel and
// takes arrow keys and Enter. Directive 71 is that Flea drives that CLI rather than opening the app,
// so this opens a pty of its own and drives it there. No shell is involved at any point, which is
// what keeps a file name with a quote or a space in it an argument rather than somebody else's word.
use super::localsendscreen::Screen;
use super::localsendtext::{could_not, parse_peers, progress, refusal, said, Peer};
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::raw::c_ulong;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
const KEY_UP: &[u8] = b"\x1b[A";
const KEY_DOWN: &[u8] = b"\x1b[B";
const KEY_ENTER: &[u8] = b"\r";
// A device answers a discovery announcement in well under a second on this LAN; measured at 1.2 s
// from process start to the first row, most of which is the CLI's own start.
const POLL: Duration = Duration::from_millis(100);
// A slower device answers after the first, so discovery waits this long after the last new row.
const SETTLE: Duration = Duration::from_millis(600);
// The CLI drops a transfer itself about 30 s after its bytes stop; this is the backstop past that.
const STALL: Duration = Duration::from_secs(60);
// The panel redraws a moved row well inside this; past it the key is taken as lost and the screen read again.
const REDRAW: Duration = Duration::from_secs(1);
// How often a wait on the screen looks again, a fraction of one redraw.
const GLANCE: Duration = Duration::from_millis(25);
const BUSY: &str = "LocalSend is busy with another transfer.";

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
    command.args(flea::bytename::args(args)).env("TERM", "xterm-256color")
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

// The CLI repaints constantly, so a thread drains the master onto a screen; it ends when the pty closes.
fn read_into(master: OwnedFd) -> (Arc<Mutex<Screen>>, std::thread::JoinHandle<()>) {
    let seen = Arc::new(Mutex::new(Screen::new(ROWS as usize, COLS as usize)));
    let sink = Arc::clone(&seen);
    let reader = std::thread::spawn(move || {
        let mut file = std::fs::File::from(master);
        let mut buf = [0u8; 4096];
        while let Ok(n) = file.read(&mut buf) {
            if n == 0 { break }
            if let Ok(mut screen) = sink.lock() { screen.feed(&buf[..n]) }
        }
    });
    (seen, reader)
}

fn look<T: Default>(seen: &Arc<Mutex<Screen>>, read: impl FnOnce(&Screen) -> T) -> T {
    seen.lock().map(|screen| read(&screen)).unwrap_or_default()
}

// Every run binds 53317, so Flea's own runs take turns on a flock; no runtime directory runs unlocked, as 0.4.0 did.
enum Turn { Held(Option<std::fs::File>), Busy, Cancelled }

fn take_turn(cancel: &AtomicBool, limit: Option<Duration>) -> Turn {
    let uid = super::manifestdir::current_uid();
    let file = super::manifestdir::qualifying_dirs(&super::manifestdir::candidate_dirs(uid), &[], uid).iter()
        .find_map(|dir| std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).mode(0o600).open(dir.join("localsend.lock")).ok());
    match file { Some(file) => wait_turn(file, cancel, limit), None => Turn::Held(None) }
}

fn wait_turn(file: std::fs::File, cancel: &AtomicBool, limit: Option<Duration>) -> Turn {
    let start = Instant::now();
    loop {
        match crate::uistore::try_lock_exclusive(&file) {
            Ok(true) => return Turn::Held(Some(file)),
            Ok(false) => {}
            Err(_) => return Turn::Held(None),
        }
        if cancel.load(Ordering::Relaxed) { return Turn::Cancelled }
        if limit.is_some_and(|limit| start.elapsed() >= limit) { return Turn::Busy }
        std::thread::sleep(POLL);
    }
}

// The devices last listed, so a menu opened while a send holds the CLI need not wait out the transfer.
static LAST_SEEN: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn remember(found: &[Peer]) {
    if let Ok(mut last) = LAST_SEEN.lock() { *last = found.iter().map(|p| (p.name.clone(), p.address.clone())).collect() }
}

fn remembered() -> Vec<Peer> {
    let last = LAST_SEEN.lock().map(|last| last.clone()).unwrap_or_default();
    last.into_iter().enumerate().map(|(i, (name, address))| Peer { index: i + 1, name, address }).collect()
}

// Keys go to the master, which is the terminal's input side. Writing them to the slave instead is a
// key the app never reads: it is the app's own output, so the write lands on its screen and nowhere
// else, which is why every press before this was silently lost.
fn press(master: &OwnedFd, keys: &[u8]) -> Result<(), String> {
    let mut writer = std::fs::File::from(master.try_clone().map_err(|e| format!("LocalSend stopped answering: {}.", crate::error::io_message(&e)))?);
    writer.write_all(keys).map_err(|e| format!("LocalSend stopped answering: {}.", crate::error::io_message(&e)))?;
    writer.flush().map_err(|e| format!("LocalSend stopped answering: {}.", crate::error::io_message(&e)))
}

// Sample: "S Clean Lemon: Sent 1 file (61 B, took 0s)", "S Clean Lemon: Declined".
// The prefix is the whole test, never a substring, so a device named "Sent Phone" is not a finished transfer.
pub fn sent(text: &str, name: &str) -> Option<Result<(), String>> {
    let said = said(text, name)?;
    if said.starts_with("Sent ") { return Some(Ok(())) }
    if said.starts_with("Declined") { return Some(Err(format!("{} declined the transfer.", name))) }
    // The receiver still holds an earlier transfer, typically one the network cut off under it.
    if said.starts_with("Blocked by another session") {
        return Some(Err(format!("{} is busy with another transfer. Try again when it ends.", name)))
    }
    if said.starts_with("Failed") { return Some(Err(could_not(name, said))) }
    None
}

fn stop(child: &mut Child) {
    let _ = crate::procsignal::signal_group(child.id() as i32, SIGKILL as i32);
    let _ = child.kill();
    let _ = child.wait();
}

// A run with no files opens the device panel by its D, and ends once no new device has answered for SETTLE.
pub fn peers(limit: Duration) -> Result<Vec<Peer>, String> {
    let never = AtomicBool::new(false);
    let _turn = match take_turn(&never, Some(Duration::ZERO)) {
        Turn::Held(file) => file,
        _ => {
            let last = remembered();
            if !last.is_empty() { return Ok(last) }
            match take_turn(&never, Some(limit)) {
                // The run just ahead is most often a discovery of its own, and its answer is this one's.
                Turn::Held(_) if !remembered().is_empty() => return Ok(remembered()),
                Turn::Held(file) => file,
                _ => return Err(BUSY.into()),
            }
        }
    };
    let (master, slave) = open_pty()?;
    let mut child = spawn(COMMAND, &[], &slave)?;
    let keys = master.try_clone()
        .map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    let (seen, _reader) = read_into(master);
    // The clock starts once the CLI is held, so a turn taken late is never read as no devices.
    let deadline = Instant::now() + limit;
    // The CLI has to be up before it has a key reader, so the panel is asked for after one poll.
    std::thread::sleep(POLL * 3);
    let _ = press(&keys, KEY_SHOW_DEVICES);
    let (mut found, mut quiet) = (Vec::new(), deadline);
    loop {
        let (now, refused) = look(&seen, |s| (parse_peers(&s.visible()), refusal(s.log())));
        if let Some(refusal) = refused {
            stop(&mut child);
            return Err(refusal)
        }
        if now.len() > found.len() {
            found = now;
            quiet = Instant::now() + SETTLE;
        }
        if Instant::now() >= quiet.min(deadline) {
            stop(&mut child);
            if !found.is_empty() { remember(&found) }
            return Ok(found)
        }
        std::thread::sleep(POLL);
    }
}

// A job may queue behind another transfer for as long as that takes, because its cancel ends the wait.
pub(crate) fn send_cancelled(name: &str, paths: &[String], cancel: &AtomicBool) -> Result<(), String> {
    send_with_cancel(name, paths, SEND_LIMIT, cancel, None)
}

pub fn send(name: &str, paths: &[String], limit: Duration) -> Result<(), String> {
    send_with_cancel(name, paths, limit, &AtomicBool::new(false), Some(limit))
}

fn gone(name: &str) -> String {
    format!("{} is not answering on this network any more.", name)
}

// Enter waits until the panel shows the wanted row lit, so a list that changed under the keys never sends elsewhere.
fn choose(keys: &OwnedFd, seen: &Arc<Mutex<Screen>>, name: &str, deadline: Instant) -> Result<(), String> {
    let mut pressed = false;
    while Instant::now() < deadline {
        let (lit, listed) = look(seen, |s| (s.chosen(), parse_peers(&s.visible())));
        let at = lit.as_deref().map(parse_peers).and_then(|row| row.into_iter().next());
        let Some(want) = listed.iter().find(|p| p.name == name).map(|p| p.index) else { return Err(gone(name)) };
        let key = match at {
            Some(at) if at.name == name => return Ok(()),
            Some(at) if want > at.index => KEY_DOWN,
            Some(_) => KEY_UP,
            // A read between a redraw's two rows: the old one plain, the new one not lit yet.
            None if pressed => {
                std::thread::sleep(GLANCE);
                continue
            }
            // No row drawn lit at all: the count from the top is all there is to go on.
            None => {
                for _ in 1..want {
                    press(keys, KEY_DOWN)?;
                    std::thread::sleep(POLL);
                }
                return Ok(())
            }
        };
        press(keys, key)?;
        pressed = true;
        // One key per redraw, so a slow repaint is never mistaken for a key that was lost.
        let redrawn = Instant::now() + REDRAW;
        while look(seen, |s| s.chosen()).is_none_or(|row| lit.as_deref() == Some(row.as_str())) && Instant::now() < redrawn {
            std::thread::sleep(GLANCE);
        }
    }
    Err(gone(name))
}

fn send_with_cancel(name: &str, paths: &[String], limit: Duration, cancel: &AtomicBool, wait: Option<Duration>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("LocalSend was given nothing to send.".into());
    }
    // localsend-cli takes files only and refuses a folder with "Not a file", so the folder is named here.
    if let Some(folder) = paths.iter().find(|p| flea::bytename::view(p).is_dir()) {
        let leaf = flea::bytename::view(folder).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.clone());
        return Err(format!("{} is a folder, and LocalSend sends only files.", leaf));
    }
    let mut args: Vec<String> = Vec::new();
    for path in paths {
        args.push("-f".into());
        args.push(path.clone());
    }
    let _turn = match take_turn(cancel, wait) {
        Turn::Held(file) => file,
        Turn::Busy => return Err(BUSY.into()),
        Turn::Cancelled => return Err("Send cancelled.".into()),
    };
    let (master, slave) = open_pty()?;
    let mut child = spawn(COMMAND, &args, &slave)?;
    // The CLI holds the only slave end from here, so the pty closes, and the reader ends, when it exits.
    drop(slave);
    let keys = master.try_clone()
        .map_err(|e| format!("LocalSend could not start: {}.", crate::error::io_message(&e)))?;
    let (seen, reader) = read_into(master);
    // The clock starts once the CLI is held, so a send queued late still gets a whole search and accept.
    let deadline = Instant::now() + limit;
    let mut found = false;
    while Instant::now() < deadline && !found {
        if cancel.load(Ordering::Relaxed) {
            stop(&mut child);
            return Err("Send cancelled.".into());
        }
        let (listed, refused) = look(&seen, |s| (parse_peers(&s.visible()), refusal(s.log())));
        if let Some(refusal) = refused {
            stop(&mut child);
            return Err(refusal)
        }
        found = listed.iter().any(|p| p.name == name);
        if found { remember(&listed) } else { std::thread::sleep(POLL) }
    }
    let chosen = if found { choose(&keys, &seen, name, deadline) } else { Err(gone(name)) };
    if let Err(error) = chosen.and_then(|()| press(&keys, KEY_ENTER)) {
        stop(&mut child);
        return Err(error)
    }
    // Only the CLI's own Sent line is a success; the accept has the rest of `limit`, and moving bytes only a STALL.
    let mut watch = Watch::default();
    loop {
        if cancel.load(Ordering::Relaxed) {
            stop(&mut child);
            return Err("Send cancelled.".into());
        }
        let (verdict, now) = look(&seen, |s| (sent(s.log(), name), progress(s.log(), name)));
        if let Some(verdict) = verdict {
            stop(&mut child);
            return verdict
        }
        watch.see(now);
        // A CLI that ended without its Sent line sent nothing, whatever its exit code; its last words say why.
        if let Ok(Some(_)) = child.try_wait() {
            let drained = Instant::now() + REDRAW;
            while !reader.is_finished() && Instant::now() < drained { std::thread::sleep(GLANCE) }
            return look(&seen, |s| sent(s.log(), name).or_else(|| said(s.log(), name).map(|said| Err(could_not(name, said)))))
                .unwrap_or_else(|| Err(format!("LocalSend stopped before sending to {}.", name)))
        }
        if let Some(late) = watch.late(name, deadline) {
            stop(&mut child);
            return Err(late)
        }
        std::thread::sleep(POLL);
    }
}

// The first progress line is drawn while the accept is awaited; a later change is movement and restarts the stall.
// Samples: "0 B / 209.7 MB" while awaited, "12.5 MB / 209.7 MB" once moving.
#[derive(Default)]
struct Watch { waiting: Option<String>, moved: Option<(String, Instant)> }

impl Watch {
    fn see(&mut self, now: Option<String>) {
        let Some(now) = now else { return };
        if self.waiting.is_none() {
            self.waiting = Some(now);
        } else if self.waiting.as_ref() != Some(&now) && self.moved.as_ref().is_none_or(|(last, _)| *last != now) {
            self.moved = Some((now, Instant::now()));
        }
    }

    fn late(&self, name: &str, deadline: Instant) -> Option<String> {
        match &self.moved {
            None if Instant::now() >= deadline => Some(format!("{} did not accept the transfer.", name)),
            Some((_, at)) if at.elapsed() >= STALL => Some(format!("{} stopped receiving the transfer.", name)),
            _ => None,
        }
    }
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
#[path = "localsend_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "localsend_wording_tests.rs"]
mod wording;
