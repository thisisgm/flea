// flea --clip-own: the detached owner behind one copy; it prints ready, then serves sends until cancelled.
use super::content;
use super::control;
use super::format;
use super::owner;
use super::wire::Conn;
use std::collections::HashMap;
use std::os::raw::{c_int, c_void};
use std::os::unix::process::CommandExt;
use std::time::Duration;

extern "C" {
    fn waitid(kind: c_int, id: u32, info: *mut c_void, options: c_int) -> c_int;
    fn setsid() -> c_int;
}

const SIGTERM: c_int = 15;
const WNOHANG: c_int = 1;

// A mapped pid is alive or unreaped: the waiter removes it under this lock before reaping.
static OWNERS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, u32>>> = std::sync::OnceLock::new();

fn owners() -> &'static std::sync::Mutex<HashMap<String, u32>> {
    OWNERS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn remember(token: &str, pid: u32) {
    owners().lock().unwrap_or_else(|e| e.into_inner()).insert(token.to_string(), pid);
}

fn forget(token: &str, pid: u32) {
    let mut owners = owners().lock().unwrap_or_else(|e| e.into_inner());
    if owners.get(token) == Some(&pid) {
        owners.remove(token);
    }
}

// Kills the owner this process started for the token; a stale or foreign token answers false and signals nothing.
pub fn withdraw(token: &str) -> bool {
    let mut owned = owners().lock().unwrap_or_else(|e| e.into_inner());
    let Some(&pid) = owned.get(token) else { return false; };
    // The lock keeps the pid unreaped while checking its exit and signalling only a live owner.
    if !owner_live(pid) {
        return false;
    }
    if crate::procsignal::signal_process(pid as c_int, SIGTERM).is_err() {
        owned.remove(token);
        return false;
    }
    true
}

// stdin is a pipe the parent writes, yet no read runs past this cap.
const STDIN_CHUNK: usize = 64 * 1024;
const STDIN_CAP: u64 = 64 * 1024 * 1024;

// One NUL-separated path list off stdin; the CLI's set shape without its operation word.
pub fn nul_paths(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut parts: Vec<&[u8]> = bytes.split(|b| *b == 0).collect();
    if parts.last() == Some(&b"".as_slice()) {
        parts.pop();
    }
    let mut paths = Vec::with_capacity(parts.len());
    for p in parts {
        match std::str::from_utf8(p) {
            Ok(s) => paths.push(s.to_string()),
            Err(_) => return Err("a clipboard path is not text".to_string()),
        }
    }
    Ok(paths)
}

pub fn read_capped_stdin() -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut stdin = std::io::stdin().lock();
    let mut chunk = [0u8; STDIN_CHUNK];
    loop {
        match stdin.read(&mut chunk) {
            Ok(0) => return Ok(out),
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if out.len() as u64 > STDIN_CAP {
                    return Err("the selection is too large for the clipboard (64 MiB at most)".to_string());
                }
            }
            Err(e) => return Err(format!("the clipboard paths could not be read ({})", e)),
        }
    }
}

pub fn run() -> i32 {
    match run_inner() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

fn run_inner() -> Result<(), String> {
    let payload = read_capped_stdin()?;
    let (op, token, paths, with_content) = split_payload(&payload)?;
    // Read before the connection, so a slow disk never holds a compositor round trip open.
    let content = if with_content { content::read(&paths) } else { None };
    let mut conn = Conn::connect()?;
    control::handshake(&mut conn)?;
    let mut owner = owner::own_with(conn, &op, &paths, &token, content)?;
    println!("ready");
    use std::io::Write;
    std::io::stdout().flush().map_err(|e| format!("the ready line could not be written ({})", e))?;
    // From here no pipe of the parent is held open, as wl-copy leaves none either.
    owner::detach_stdio();
    owner::serve_owner(&mut owner)?;
    Ok(())
}

// Sample input: "copy\0ab12...(32 hex)\0/tmp/a\0/tmp/b\0" gives ("copy", the token, ["/tmp/a", "/tmp/b"], false);
// "copy\0ab12...\0+content\0/tmp/a.png\0" asks for that file's own bytes beside the path list.
pub(crate) fn split_payload(payload: &[u8]) -> Result<(String, String, Vec<String>, bool), String> {
    let mut parts: Vec<&[u8]> = payload.split(|b| *b == 0).collect();
    if parts.last() == Some(&b"".as_slice()) {
        parts.pop();
    }
    let [op, token, paths @ ..] = parts.as_slice() else {
        return Err("the clipboard payload names no operation".to_string());
    };
    let op = std::str::from_utf8(op).map_err(|_| "the clipboard operation is not text".to_string())?;
    if !format::is_op(op) {
        return Err("the clipboard operation is copy or cut".to_string());
    }
    let token = std::str::from_utf8(token).map_err(|_| "the clipboard token is not text".to_string())?;
    if !format::is_token(token) {
        return Err("the clipboard token is 32 hex chars".to_string());
    }
    let (with_content, paths) = match paths {
        [flag, rest @ ..] if *flag == content::FLAG.as_bytes() => (true, rest),
        _ => (false, paths),
    };
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        out.push(std::str::from_utf8(p).map_err(|_| "a clipboard path is not text".to_string())?.to_string());
    }
    format::check_selection(op, &out)?;
    Ok((op.to_string(), token.to_string(), out, with_content))
}

// One copy's owner, detached into its own session; the copy outlives the window that made it.
pub fn spawn_owner(op: &str, paths: &[String]) -> Result<String, String> {
    format::check_selection(op, paths)?;
    let exe = std::env::current_exe().map_err(|e| format!("the clipboard owner could not start ({})", e))?;
    spawn_owner_with(&exe, op, paths, |rx| rx.recv_timeout(OWNER_READY_WAIT))
}

// A copy that also offers the one file's own bytes, for the shelf's Copy to Clipboard.
pub fn spawn_owner_content(paths: &[String]) -> Result<String, String> {
    format::check_selection("copy", paths)?;
    let exe = std::env::current_exe().map_err(|e| format!("the clipboard owner could not start ({})", e))?;
    spawn_owner_as(&exe, "copy", paths, true, |rx| rx.recv_timeout(OWNER_READY_WAIT))
}

const OWNER_READY_WAIT: Duration = Duration::from_secs(2);
// The owner's first stdout line: whether the read finished, and what it held.
type Ready = (bool, String);

// The program and the ready wait are parameters so a test starts a real child and ends its wait at once.
pub(crate) fn spawn_owner_with(
    exe: &std::path::Path, op: &str, paths: &[String],
    ready: impl FnOnce(&std::sync::mpsc::Receiver<Ready>) -> Result<Ready, std::sync::mpsc::RecvTimeoutError>,
) -> Result<String, String> {
    spawn_owner_as(exe, op, paths, false, ready)
}

// The one spawn behind both: with_content puts the content flag ahead of the paths in the payload.
pub(crate) fn spawn_owner_as(
    exe: &std::path::Path, op: &str, paths: &[String], with_content: bool,
    ready: impl FnOnce(&std::sync::mpsc::Receiver<Ready>) -> Result<Ready, std::sync::mpsc::RecvTimeoutError>,
) -> Result<String, String> {
    let token = format::make_token()?;
    let mut child = unsafe {
        std::process::Command::new(exe)
        .arg("--clip-own")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .pre_exec(|| {
            if setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        })
        .spawn()
    }.map_err(|e| format!("the clipboard owner could not start ({})", e))?;
    // Every exit after a spawn kills then reaps the child once, so no path leaves a zombie.
    let abandon = |mut child: std::process::Child| {
        let _ = child.kill();
        let _ = child.wait();
    };
    let mut payload = format!("{}\0{}\0", op, token).into_bytes();
    if with_content {
        payload.extend_from_slice(content::FLAG.as_bytes());
        payload.push(0);
    }
    for p in paths {
        payload.extend_from_slice(p.as_bytes());
        payload.push(0);
    }
    use std::io::Write;
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            abandon(child);
            return Err("the clipboard owner could not start (no stdin)".to_string());
        }
    };
    if stdin.write_all(&payload).is_err() {
        abandon(child);
        return Err("the clipboard owner could not start (no payload)".to_string());
    }
    drop(stdin);
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            abandon(child);
            return Err("the clipboard owner could not start (no stdout)".to_string());
        }
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut line = String::new();
        let done = std::io::BufReader::new(stdout).read_line(&mut line).is_ok();
        let _ = tx.send((done, line));
    });
    match ready(&rx) {
        Ok((true, line)) if line.trim_end() == "ready" => {}
        _ => {
            let stderr = child.stderr.take();
            abandon(child);
            let error = stderr.and_then(|stderr| {
                use std::io::BufRead;
                let mut line = String::new();
                std::io::BufReader::new(stderr).read_line(&mut line).ok()?;
                // Sample input: "flea: this compositor does not offer clipboard access\n".
                let line = line.trim_end();
                let error = line.strip_prefix("flea: ").unwrap_or(line);
                (!error.is_empty()).then(|| error.to_string())
            });
            return Err(error.unwrap_or_else(|| "the clipboard owner did not answer".to_string()));
        }
    }
    start_reaper(child, token.clone());
    Ok(token)
}

// A thread reaps the owner after it runs until replaced, so the caller never waits on it.
pub(crate) fn start_reaper(child: std::process::Child, token: String) -> std::thread::JoinHandle<()> {
    remember(&token, child.id());
    std::thread::spawn(move || reap_owner(child, &token, || {}))
}

// waitid with WNOWAIT sees an exit without reaping it, so the pid stays unrecyclable until forget.
const P_PID: c_int = 1;
const WEXITED: c_int = 4;
const WNOWAIT: c_int = 0x01000000;
const EINTR: i32 = 4;
const SIGINFO_WORDS: usize = 16;

fn owner_live(pid: u32) -> bool {
    loop {
        let mut info = [0u64; SIGINFO_WORDS];
        let result = unsafe { waitid(P_PID, pid, info.as_mut_ptr().cast(), WEXITED | WNOHANG | WNOWAIT) };
        if result == 0 {
            // Linux clears si_signo at the start of siginfo when no exit is available.
            return info[0] == 0;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
            return false;
        }
    }
}

pub(crate) fn wait_for_exit(pid: u32) {
    let mut info = [0u64; SIGINFO_WORDS];
    loop {
        if unsafe { waitid(P_PID, pid, info.as_mut_ptr().cast(), WEXITED | WNOWAIT) } == 0 {
            return;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
            return;
        }
    }
}

fn reap_owner(mut child: std::process::Child, token: &str, after_exit: impl FnOnce()) {
    let pid = child.id();
    wait_for_exit(pid);
    after_exit();
    forget(token, pid);
    let _ = child.wait();
}

#[cfg(test)]
#[path = "own_tests.rs"]
mod tests;
