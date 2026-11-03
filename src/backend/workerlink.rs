// The backend's side of `flea --thumb-worker`: one worker for the pool, started on the first video that qualifies and retired for good at its first failure; see AGENTS.md "Thumbnail worker".
use crate::backend::child::Ran;
use crate::backend::fdpass;
use crate::backend::sandbox;
use crate::backend::thumbspec::Spec;
use crate::backend::thumbworker::{FAILED, NOT_STARTED, NO_LANDLOCK, NO_LIBRARY, READY, REQUEST_BYTES, SONAME, SUCCEEDED};
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

    // Some only for a thumbnail the worker made or an input that is not a file to judge; None sends the job down the exec path.
    pub fn generate(&self, input: &Path, output: &Path, size: u32, film_strip: bool, limit: Duration) -> Option<Ran> {
        if matches!(*self.state.lock().unwrap(), State::Gone) {
            return None;
        }
        // corner: an input that no longer opens, or a fifo or device swapped in after the listing, judges nothing, and the exec path would fail or hang on it.
        let Ok(input) = std::fs::OpenOptions::new().read(true).custom_flags(O_NONBLOCK | O_NOCTTY).open(input) else {
            return Some(Ran::NotStarted);
        };
        if !input.metadata().is_ok_and(|m| m.is_file()) {
            return Some(Ran::NotStarted);
        }
        let output = std::fs::OpenOptions::new().write(true).custom_flags(O_NOFOLLOW | O_NOCTTY).open(output).ok()?;
        let (reply, theirs) = fdpass::pair().ok()?;
        let mut payload = [0u8; REQUEST_BYTES];
        payload[..4].copy_from_slice(&size.to_le_bytes());
        payload[4] = u8::from(film_strip);
        if !self.send(&payload, &[input.as_raw_fd(), output.as_raw_fd(), theirs.as_raw_fd()]) {
            return None;
        }
        // Only the worker may hold the far end, or its death would never read as a closed socket here.
        drop(theirs);
        match read_byte(&reply, limit + REPLY_GRACE) {
            Heard::Byte(SUCCEEDED) => Some(Ran::Succeeded),
            // The exec path judges this file again, so only the thumbnailer program ever records a failure.
            Heard::Byte(FAILED) => None,
            // An N, no verdict at all or an unknown byte says something about the worker and nothing about this file.
            heard => {
                WorkerLink::retire(&mut self.state.lock().unwrap(), gone_because(&heard));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::thumbs::THUMB_SIZE;
    use std::process::Command;

    // A stand-in jail that was never sandboxed, so retiring it kills only its own child.
    fn dummy_jailed(child: std::process::Child) -> crate::backend::jail::Jailed {
        crate::backend::jail::Jailed {
            child,
            sandbox_pid: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(crate::backend::jail::GONE)),
        }
    }

    // Stands in for the worker: answers the one request it is sent with `verdict`, on the reply socket that request carried.
    fn answered_by(verdict: u8) -> (WorkerLink, std::thread::JoinHandle<()>) {
        let (mine, theirs) = fdpass::pair().unwrap();
        let child = Command::new("true").spawn().unwrap();
        let link = WorkerLink { state: Mutex::new(State::Running { jailed: dummy_jailed(child), requests: mine }) };
        let answering = std::thread::spawn(move || {
            let request = fdpass::recv(theirs.as_raw_fd()).unwrap().expect("a request");
            let reply = request.fds.into_iter().nth(2).expect("a reply socket");
            fdpass::send_byte(&reply, verdict).unwrap();
        });
        (link, answering)
    }

    #[test]
    fn only_a_thumbnail_is_final_and_a_machine_failure_retires_the_worker() {
        let dir = TestDir::new("worker-verdicts");
        let input = dir.file("clip.mp4", "not a video");
        let output = dir.file("out.png", "");
        for (verdict, published, keeps_serving) in [(SUCCEEDED, true, true), (FAILED, false, true), (NOT_STARTED, false, false)] {
            let (link, answering) = answered_by(verdict);
            let got = link.generate(&input, &output, THUMB_SIZE, true, Duration::from_secs(1));
            answering.join().unwrap();
            assert_eq!(matches!(got, Some(Ran::Succeeded)), published, "verdict {}", verdict as char);
            assert_eq!(got.is_none(), !published, "verdict {} must send the job down the exec path", verdict as char);
            assert_eq!(matches!(*link.state.lock().unwrap(), State::Running { .. }), keeps_serving, "verdict {}", verdict as char);
        }
    }

    // A non-blocking open returns at once; this bounds a regression to a failure rather than a hung suite.
    const HUNG: Duration = Duration::from_secs(5);

    #[test]
    fn arm_oflags_output_symlinks_never_reach_the_worker() {
        let dir = TestDir::new("arm-oflags-worker-output");
        let input = dir.file("clip.mp4", "not a video");
        let target = dir.file("target.png", "leave this file alone");
        let (mine, theirs) = fdpass::pair().unwrap();
        let child = Command::new("true").spawn().unwrap();
        let link = WorkerLink {
            state: Mutex::new(State::Running {
                jailed: dummy_jailed(child),
                requests: mine,
            }),
        };
        for name in ["target.png", "absent.png"] {
            let output = dir.join(&format!("{name}.link"));
            std::os::unix::fs::symlink(name, &output).unwrap();
            assert!(link
                .generate(&input, &output, THUMB_SIZE, true, Duration::ZERO)
                .is_none());
            assert!(
                matches!(read_byte(&theirs, Duration::ZERO), Heard::Silence),
                "output symlink reached the worker"
            );
            assert!(output.symlink_metadata().unwrap().is_symlink());
        }
        assert_eq!(
            std::fs::read_to_string(target).unwrap(),
            "leave this file alone"
        );
        assert!(!dir.join("absent.png").exists());
        assert!(matches!(*link.state.lock().unwrap(), State::Running { .. }));

        let output = dir.file("regular.png", "");
        let (link, answering) = answered_by(SUCCEEDED);
        assert!(matches!(
            link.generate(&input, &output, THUMB_SIZE, true, Duration::from_secs(1)),
            Some(Ran::Succeeded)
        ));
        answering.join().unwrap();
    }

    #[test]
    fn an_input_that_is_not_a_file_to_judge_never_reaches_the_worker() {
        let dir = TestDir::new("worker-not-a-file");
        let fifo = dir.join("swapped.mp4");
        assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success(), "mkfifo failed");
        let output = dir.file("out.png", "");
        let (mine, theirs) = fdpass::pair().unwrap();
        let child = Command::new("true").spawn().unwrap();
        let link = std::sync::Arc::new(WorkerLink { state: Mutex::new(State::Running { jailed: dummy_jailed(child), requests: mine }) });
        for input in [fifo, dir.join("vanished.mp4")] {
            let (done, answer) = std::sync::mpsc::channel();
            let (link, output, shown) = (std::sync::Arc::clone(&link), output.clone(), input.display().to_string());
            std::thread::spawn(move || done.send(link.generate(&input, &output, THUMB_SIZE, true, Duration::from_secs(1))).unwrap());
            let got = answer.recv_timeout(HUNG).unwrap_or_else(|_| panic!("generate never returned for {}", shown));
            assert!(matches!(got, Some(Ran::NotStarted)), "{} was not answered as judging nothing", shown);
        }
        assert!(matches!(read_byte(&theirs, Duration::ZERO), Heard::Silence), "a request reached the worker");
        assert!(matches!(*link.state.lock().unwrap(), State::Running { .. }), "judging nothing retired the worker");
    }

    #[test]
    fn a_worker_that_is_not_serving_says_why() {
        assert_eq!(why_not(&Heard::Byte(NO_LIBRARY)), "libffmpegthumbnailer.so.4 did not load");
        assert_eq!(why_not(&Heard::Byte(NO_LANDLOCK)), "this kernel has no Landlock that can deny a truncation");
        assert_eq!(why_not(&Heard::Closed), "it exited before it answered");
        assert_eq!(why_not(&Heard::Silence), "it did not answer within 5 s");
        assert_eq!(why_not(&Heard::Broken(std::io::Error::other("a test error"))), "its socket failed: a test error");
    }

    #[test]
    fn a_wait_names_what_it_heard() {
        let (mine, theirs) = fdpass::pair().unwrap();
        fdpass::send(theirs.as_raw_fd(), &[READY, READY], &[]).unwrap();
        assert!(matches!(read_byte(&mine, Duration::from_secs(1)), Heard::Broken(_)), "a two-byte packet is a broken socket");
        fdpass::send_byte(&theirs, READY).unwrap();
        assert!(matches!(read_byte(&mine, Duration::from_secs(1)), Heard::Byte(READY)));
        assert!(matches!(read_byte(&mine, Duration::ZERO), Heard::Silence), "nothing was sent");
        drop(theirs);
        assert!(matches!(read_byte(&mine, Duration::from_secs(1)), Heard::Closed));
        let not_a_socket = OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
        assert!(matches!(read_byte(&not_a_socket, Duration::from_secs(1)), Heard::Broken(_)), "recvmsg on a file that is not a socket fails");
    }

    #[test]
    fn a_retired_worker_says_whether_the_machine_or_the_worker_failed() {
        assert_eq!(gone_because(&Heard::Byte(NOT_STARTED)), "failed a job on this machine");
        assert_eq!(gone_because(&Heard::Closed), "stopped answering");
        assert_eq!(gone_because(&Heard::Byte(b'?')), "stopped answering");
    }

    fn spec(exec: &str) -> Spec {
        Spec { exec: exec.split(' ').map(str::to_string).collect() }
    }

    #[test]
    fn the_shipped_exec_line_is_the_worker_shape() {
        assert_eq!(worker_shape(&spec("ffmpegthumbnailer -i %i -o %o -s %s -f")), Some(true));
        assert_eq!(worker_shape(&spec("/usr/bin/ffmpegthumbnailer -s %s -i %i -o %o")), Some(false));
    }

    #[test]
    fn any_other_program_or_flag_stays_on_the_exec_path() {
        for exec in [
            "glycin-thumbnailer -i %i -o %o -s %s",
            "ffmpegthumbnailerx -i %i -o %o -s %s",
            "ffmpegthumbnailer -i %i -o %o -s %s -t 20",
            "ffmpegthumbnailer -i %u -o %o -s %s",
            "ffmpegthumbnailer -i %i -o %o",
            "ffmpegthumbnailer -i %i -o %o -s %s -f -f",
            "ffmpegthumbnailer -i %i -i %i -o %o -s %s",
            "ffmpegthumbnailer -i",
        ] {
            assert_eq!(worker_shape(&spec(exec)), None, "{} took the worker", exec);
        }
    }
}
