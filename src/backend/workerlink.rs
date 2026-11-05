// The backend's side of `flea --thumb-worker`: one worker for the pool, started on the first video that qualifies and retired for good at its first failure; see AGENTS.md "Thumbnail worker".
use crate::backend::child::Ran;
use crate::backend::fdpass;
use crate::backend::sandbox;
use crate::backend::thumbspec::Spec;
use crate::backend::thumbreq::identity;
use crate::backend::thumbworker::{FAILED, NOT_STARTED, NO_LANDLOCK, NO_LIBRARY, READY, REQUEST_BYTES, SONAME, SUCCEEDED, VIDEO_CANCELLED};
use crate::oflags::{O_NOFOLLOW, O_NONBLOCK};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

// The worker answers READY after one dlopen and one Landlock probe, measured at tens of milliseconds.
const READY_LIMIT: Duration = Duration::from_secs(5);
// The worker enforces the job's own deadline, so the backend only waits this much longer for its word.
const REPLY_GRACE: Duration = Duration::from_secs(5);
// open(2) flags: never block on a fifo swapped in after the listing, never follow a link at the temp's name.
const O_NOCTTY: i32 = 0o400;
// poll(2) POLLIN, and EINTR, which is a retry.
const POLLIN: i16 = 1;
const EINTR: i32 = 4;
// The operator's way back to the exec path, and the battery's way to run it on purpose.
const OFF_SWITCH: &str = "FLEA_THUMB_WORKER";

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}

enum State {
    Unstarted,
    Running { jailed: crate::backend::jail::Jailed, requests: OwnedFd },
    Gone,
}

pub(super) struct Cancelled;

pub struct WorkerLink {
    state: Mutex<State>,
}

// Sample Exec, the one ffmpegthumbnailer ships: "ffmpegthumbnailer -i %i -o %o -s %s -f"; any other program, flag or spelling stays on the exec path.
pub fn worker_shape(spec: &Spec) -> Option<bool> {
    let mut tokens = spec.exec.iter().map(String::as_str);
    let program = tokens.next()?;
    if Path::new(program).file_name()?.to_str()? != "ffmpegthumbnailer" {
        return None;
    }
    let (mut input, mut output, mut size, mut film_strip) = (false, false, false, false);
    while let Some(flag) = tokens.next() {
        let seen = match flag {
            "-i" if tokens.next() == Some("%i") => &mut input,
            "-o" if tokens.next() == Some("%o") => &mut output,
            "-s" if tokens.next() == Some("%s") => &mut size,
            "-f" => &mut film_strip,
            _ => return None,
        };
        if *seen {
            return None;
        }
        *seen = true;
    }
    (input && output && size).then_some(film_strip)
}

// What one wait on a socket heard, kept apart so a message never has to guess an exit from a timeout.
enum Heard {
    Byte(u8),
    Silence,
    Closed,
    Broken(std::io::Error),
}

// Why a worker that did not answer READY is not serving.
fn why_not(heard: &Heard) -> String {
    match heard {
        Heard::Byte(NO_LIBRARY) => format!("{} did not load", SONAME.to_string_lossy()),
        Heard::Byte(NO_LANDLOCK) => String::from("this kernel has no Landlock that can deny a truncation"),
        Heard::Byte(other) => format!("it answered the unknown byte {}", other),
        Heard::Silence => format!("it did not answer within {} s", READY_LIMIT.as_secs()),
        Heard::Closed => String::from("it exited before it answered"),
        Heard::Broken(e) => format!("its socket failed: {}", e),
    }
}

// Why a job's answer retires the worker: an N is a failure of this machine inside the child, and anything else but S or F is the worker gone.
fn gone_because(heard: &Heard) -> &'static str {
    match heard {
        Heard::Byte(NOT_STARTED) => "failed a job on this machine",
        _ => "stopped answering",
    }
}

// Waits for one byte on a socket until the limit; a packet that is not one byte is a broken socket.
fn read_byte(sock: &OwnedFd, limit: Duration) -> Heard {
    let deadline = std::time::Instant::now() + limit;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let ms = left.as_millis().saturating_add(1).min(i32::MAX as u128) as i32;
        let mut fds = PollFd { fd: sock.as_raw_fd(), events: POLLIN, revents: 0 };
        let ready = unsafe { poll(&mut fds, 1, ms) };
        if ready == 0 {
            return Heard::Silence;
        }
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(EINTR) {
                continue;
            }
            return Heard::Broken(error);
        }
        return match fdpass::recv(sock.as_raw_fd()) {
            Ok(Some(message)) if message.payload.len() == 1 => Heard::Byte(message.payload[0]),
            Ok(Some(_)) => Heard::Broken(std::io::Error::new(std::io::ErrorKind::InvalidData, "a reply was not one byte")),
            Ok(None) => Heard::Closed,
            Err(e) => Heard::Broken(e),
        };
    }
}

impl WorkerLink {
    pub fn new() -> WorkerLink {
        let off = std::env::var(OFF_SWITCH).is_ok_and(|v| v == "off");
        WorkerLink { state: Mutex::new(if off { State::Gone } else { State::Unstarted }) }
    }

    /// Check eligibility without waiting for the pool's worker to start.
    pub(super) fn may_accept(&self) -> bool {
        !self.state.try_lock().is_ok_and(|state| matches!(*state, State::Gone))
    }

    // Started under the lock, so pool threads meeting their first video together start one worker between them.
    fn spawn() -> Result<(crate::backend::jail::Jailed, OwnedFd), String> {
        let exe = std::fs::canonicalize("/proc/self/exe").map_err(|e| format!("/proc/self/exe did not resolve: {}", e))?;
        let exe_text = exe.to_str().ok_or("the flea executable's path is not UTF-8")?.to_string();
        let (mine, theirs) = fdpass::pair().map_err(|e| format!("no socket pair: {}", e))?;
        let inner = vec![exe_text, "--thumb-worker".to_string()];
        let mut argv = sandbox::wrap_worker(&inner, &exe);
        sandbox::add_status(&mut argv, inner.len());
        // corner: bwrap's --die-with-parent follows the thread that spawns it, and a pool thread lives as long as the backend.
        let mut jailed = crate::backend::jail::spawn_jailed(&argv, |cmd| {
            cmd.stdin(Stdio::from(theirs));
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        })
        .map_err(|e| format!("bwrap did not start: {}", e))?;
        let heard = read_byte(&mine, READY_LIMIT);
        if matches!(heard, Heard::Byte(READY)) {
            return Ok((jailed, mine));
        }
        crate::backend::jail::kill_tree(&mut jailed);
        Err(why_not(&heard))
    }

    // Holding the lock across the send keeps a request whole and lets one failure retire the worker for every thread.
    fn send(&self, payload: &[u8], fds: &[i32]) -> bool {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, State::Unstarted) {
            *state = match WorkerLink::spawn() {
                Ok((jailed, requests)) => State::Running { jailed, requests },
                Err(why) => {
                    eprintln!("flea: the thumbnail worker did not start ({}), so videos use the thumbnailer program", why);
                    State::Gone
                }
            };
        }
        let sent = match &*state {
            State::Running { requests, .. } => fdpass::send(requests.as_raw_fd(), payload, fds).is_ok(),
            _ => return false,
        };
        if !sent {
            WorkerLink::retire(&mut state, "stopped taking requests");
        }
        sent
    }

    fn retire(state: &mut State, why: &str) {
        if let State::Running { jailed, .. } = state {
            crate::backend::jail::kill_tree(jailed);
            eprintln!("flea: the thumbnail worker {}, so videos use the thumbnailer program", why);
        }
        *state = State::Gone;
    }

    /// Serve an eligible video, or leave it to the exec path if the worker declines it.
    pub(super) fn generate_for(&self, input: &Path, output: &Path, size: u32, film_strip: bool, limit: Duration, identity: Option<&identity::Shared>) -> Result<Option<Ran>, Cancelled> {
        let result = self.generate_request(input, output, size, film_strip, limit, identity);
        if identity.is_some_and(identity::finish_video) { Err(Cancelled) } else { result }
    }

    /// Dispatch one job, checking cancellation before opening files and before sending the request.
    fn generate_request(&self, input: &Path, output: &Path, size: u32, film_strip: bool, limit: Duration, identity: Option<&identity::Shared>) -> Result<Option<Ran>, Cancelled> {
        if identity.is_some_and(identity::video_cancelled) {
            return Err(Cancelled);
        }
        if matches!(*self.state.lock().unwrap(), State::Gone) {
            return Ok(None);
        }
        // corner: an input that no longer opens, or a fifo or device swapped in after the listing, judges nothing, and the exec path would fail or hang on it.
        let Ok(input) = std::fs::OpenOptions::new().read(true).custom_flags(O_NONBLOCK | O_NOCTTY).open(input) else {
            return Ok(Some(Ran::NotStarted));
        };
        if !input.metadata().is_ok_and(|m| m.is_file()) {
            return Ok(Some(Ran::NotStarted));
        }
        let Ok(output) = std::fs::OpenOptions::new().write(true).custom_flags(O_NOFOLLOW | O_NOCTTY).open(output) else { return Ok(None) };
        let Ok((reply, theirs)) = fdpass::pair() else { return Ok(None) };
        if identity.is_some_and(|shared| !identity::arm_video(shared, &reply)) {
            return Err(Cancelled);
        }
        let mut payload = [0u8; REQUEST_BYTES];
        payload[..4].copy_from_slice(&size.to_le_bytes());
        payload[4] = u8::from(film_strip);
        let sent = self.send(&payload, &[input.as_raw_fd(), output.as_raw_fd(), theirs.as_raw_fd()]);
        // Only the worker may hold the far end, or its death would never read as a closed socket here.
        drop(theirs);
        let heard = sent.then(|| read_byte(&reply, limit + REPLY_GRACE));
        let result = match heard {
            Some(Heard::Byte(SUCCEEDED)) => Some(Ran::Succeeded),
            Some(Heard::Byte(VIDEO_CANCELLED)) => return Err(Cancelled),
            // The exec path judges this file again, so only the thumbnailer program ever records a failure.
            Some(Heard::Byte(FAILED)) | None => None,
            // An N, no verdict at all or an unknown byte says something about the worker and nothing about this file.
            Some(heard) => {
                WorkerLink::retire(&mut self.state.lock().unwrap(), gone_because(&heard));
                None
            }
        };
        Ok(result)
    }

    #[cfg(test)]
    fn generate(&self, input: &Path, output: &Path, size: u32, film_strip: bool, limit: Duration) -> Option<Ran> {
        self.generate_for(input, output, size, film_strip, limit, None).ok().flatten()
    }
}

#[cfg(test)]
#[path = "workerlink_tests.rs"]
mod tests;
