// A cancel during bwrap startup needs --json-status-fd: it names the sandbox init so both die.
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

// No pid yet: kill_tree waits up to STATUS_WAIT_MS for one before any kill.
pub(crate) const GONE: i32 = -1;
// The status pipe closed with no pid, so kill_tree stops waiting and kills only the launcher.
pub(crate) const NO_PID: i32 = 0;
const SIGKILL: i32 = 9;
// pipe2 O_CLOEXEC, so a spawn from another thread mid-jail never inherits the status pipe.
const O_CLOEXEC: i32 = 0o2000000;
// fcntl F_SETFD, to clear CLOEXEC where dup2(fd, fd) would leave it set.
const F_SETFD: i32 = 2;
// A cancel before bwrap wrote the init pid waits this long for it, so the kill lands while bwrap can still prove ownership.
pub(crate) const STATUS_WAIT_MS: u64 = 200;
const STATUS_STEP_MS: u64 = 5;
// How long kill_tree waits for a killed pid to vanish; the cancel tests pin their promptness to this same bound.
pub(crate) const WAIT_GONE_SECS: u64 = 2;
const WAIT_GONE_STEP_MS: u64 = 10;
// The execute bits of st_mode; a launcher needs any one of them.
const ANY_EXECUTE_BIT: u32 = 0o111;

extern "C" {
    fn pipe2(fds: *mut i32, flags: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn setpgid(pid: i32, pgid: i32) -> i32;
}

pub struct Jailed {
    pub child: std::process::Child,
    pub sandbox_pid: Arc<AtomicI32>,
}

// Sample input: b"{\"child-pid\": 1234}\n" answers 1234; the tool never writes this pipe.
pub fn parse_child_pid(buf: &[u8]) -> Option<i32> {
    let key = b"\"child-pid\"";
    let mut i = 0;
    while i + key.len() <= buf.len() {
        if &buf[i..i + key.len()] == key {
            let mut j = i + key.len();
            while j < buf.len() && !buf[j].is_ascii_digit() {
                j += 1;
            }
            let start = j;
            while j < buf.len() && buf[j].is_ascii_digit() {
                j += 1;
            }
            if start < j {
                if let Ok(text) = std::str::from_utf8(&buf[start..j]) {
                    if let Ok(pid) = text.parse::<i32>() {
                        if pid > 0 {
                            return Some(pid);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}

// Sample /proc/<pid>/stat: "123 (bwrap) S 456 ..." so the ppid is the second field after the last ")".
fn ppid_of_stat(text: &str) -> Option<i32> {
    let after = text.rsplit_once(')')?.1;
    let mut fields = after.split_whitespace();
    fields.next()?;
    fields.next()?.parse::<i32>().ok()
}

fn stat_ppid(pid: i32) -> Option<i32> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    ppid_of_stat(&text)
}

// A forged child-pid must never be signalled, so a reported pid is killed only while its parent is this jail's own bwrap.
fn owned_by(pid: i32, bwrap: i32) -> bool {
    stat_ppid(pid) == Some(bwrap)
}

// SIGKILLs a reported sandbox pid only while its parent is still this jail's bwrap; anything else is refused.
pub(crate) fn kill_sandbox(out: &Arc<AtomicI32>, bwrap: i32) {
    let pid = out.load(Ordering::SeqCst);
    if pid > 0 && owned_by(pid, bwrap) {
        let _ = crate::procsignal::signal_process(pid, SIGKILL);
    }
}

fn status_reader(fd: OwnedFd, out: Arc<AtomicI32>) {
    use std::io::Read;
    let mut file = std::fs::File::from(fd);
    let mut buf = Vec::new();
    let mut tmp = [0u8; 512];
    // EOF with no pid means bwrap left without writing, so 0 ends the wait at once; every kill path already requires pid > 0.
    let mut saw_pid = false;
    loop {
        match file.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pid) = parse_child_pid(&buf) {
                    saw_pid = true;
                    out.store(pid, Ordering::SeqCst);
                }
            }
            Err(_) => break,
        }
    }
    if !saw_pid {
        out.store(NO_PID, Ordering::SeqCst);
    }
}

// Sample argv: ["prlimit","bwrap","--json-status-fd","7",...] with STATUS_FD open.
pub fn spawn_jailed(full: &[String], setup: impl FnOnce(&mut std::process::Command)) -> std::io::Result<Jailed> {
    if full.is_empty() {
        return Err(std::io::Error::other("empty jail argv"));
    }
    let launcher = launcher_path(&full[0])?;
    // Our prlimit wrappers attach limit values to their flags; the first non-option is the launcher.
    let nested = if std::path::Path::new(&full[0]).file_name()
        == Some(std::ffi::OsStr::new(crate::backend::sandbox::PRLIMIT)) {
        full.iter().enumerate().skip(1).find(|(_, arg)| !arg.starts_with('-'))
            .map(|(at, program)| launcher_path(program).map(|path| (at, path))).transpose()?
    } else { None };
    let mut fds = [0, 0];
    if unsafe { pipe2(fds.as_mut_ptr(), O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write_fd = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    let read_raw = read_fd.as_raw_fd();
    let write_raw = write_fd.as_raw_fd();
    let status_fd = crate::backend::sandbox::STATUS_FD;
    let mut cmd = crate::backend::locale::command(launcher);
    if let Some((at, path)) = nested {
        cmd.args(flea::bytename::args(&full[1..at]))
            .arg(path)
            .args(flea::bytename::args(&full[at + 1..]));
    } else {
        cmd.args(flea::bytename::args(&full[1..]));
    }
    setup(&mut cmd);
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(move || {
            close(read_raw);
            if write_raw == status_fd {
                // dup2(fd, fd) leaves CLOEXEC set, so only this child clears it.
                if fcntl(status_fd, F_SETFD, 0 as c_int) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            } else {
                // dup2 clears CLOEXEC on the new fd, so only bwrap's status fd stays open past exec.
                if dup2(write_raw, status_fd) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                close(write_raw);
            }
            setpgid(0, 0);
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    drop(write_fd);
    let pid = Arc::new(AtomicI32::new(GONE));
    let reader_pid = Arc::clone(&pid);
    // A reader that never starts leaves the child running, so it is killed here and the caller answers NotStarted.
    if std::thread::Builder::new()
        .name("flea-jail-status".to_string())
        .spawn(move || status_reader(read_fd, reader_pid))
        .is_err()
    {
        let mut jailed = Jailed { child, sandbox_pid: pid };
        kill_tree(&mut jailed);
        return Err(std::io::Error::other("the jail status reader could not start"));
    }
    Ok(Jailed { child, sandbox_pid: pid })
}

fn launcher_path(program: &str) -> std::io::Result<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    let directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .filter(|directory| directory.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    let program = Path::new(program);
    let launcher = if program.is_absolute() {
        Some(program.to_owned())
    } else if program.components().count() == 1 {
        directories
            .iter()
            .map(|directory| directory.join(program))
            .find(|candidate| executable_file(candidate))
    } else {
        None
    };
    let launcher = launcher.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Sandbox launcher is unavailable",
        )
    })?;
    Ok(launcher)
}

// A launcher lookup's criterion: a regular file, or a link to one, with any execute bit set.
pub(crate) fn executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & ANY_EXECUTE_BIT != 0)
}

// Kills bwrap and the sandbox init, reaps bwrap within a bound, and leaves the Jailed finished.
pub fn kill_tree(jailed: &mut Jailed) {
    let bwrap = jailed.child.id() as i32;
    // Ownership is proven before any kill: after the reap the kernel reparents the init, so a later check refuses every time.
    let first = wait_for_pid(&jailed.sandbox_pid);
    let mut killed = GONE;
    if first > 0 && owned_by(first, bwrap) {
        let _ = crate::procsignal::signal_process(first, SIGKILL);
        killed = first;
    }
    let pid = jailed.child.id() as i32;
    let _ = crate::procsignal::signal_group(pid, SIGKILL);
    let _ = jailed.child.kill();
    // A held reap may finish on another thread, so no caller waits on or signals this child again.
    crate::backend::jailreap::reap_or_hand_off(&mut jailed.child);
    wait_gone(killed);
}

fn wait_for_pid(out: &Arc<AtomicI32>) -> i32 {
    for _ in 0..STATUS_WAIT_MS / STATUS_STEP_MS {
        let pid = out.load(Ordering::SeqCst);
        if pid != GONE {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(STATUS_STEP_MS));
    }
    out.load(Ordering::SeqCst)
}

fn wait_gone(pid: i32) {
    if pid <= 0 {
        return;
    }
    for _ in 0..WAIT_GONE_SECS * 1000 / WAIT_GONE_STEP_MS {
        if crate::procsignal::signal_process(pid, 0).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(WAIT_GONE_STEP_MS));
    }
}

#[cfg(test)]
#[path = "jail_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "jail_path_tests.rs"]
mod path_tests;
