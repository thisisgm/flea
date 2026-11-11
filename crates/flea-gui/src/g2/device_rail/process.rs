//! One child process per job, run off the UI thread; its start and its outcome come back over a channel.
use super::secret::Secret;
#[path = "process_input.rs"]
mod input;
#[path = "process_io.rs"]
mod output;
use async_channel::{Receiver, Sender};
use input::Input;
use output::Pipe;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The most output kept from one stream; the rest is drained and dropped.
pub const OUTPUT_CAP: usize = 1 << 20;

/// Escaped descendants cannot hold the runner after the direct child exits.
const DRAIN_AFTER_EXIT: Duration = Duration::from_millis(100);

#[cfg(test)]
#[path = "process_withhold_tests.rs"]
mod withhold_tests;

/// A command line, its added environment, and bytes for its stdin that are never on argv.
#[derive(Debug, Default)]
pub struct Job {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Secret>,
}

impl Job {
    /// A C-locale job, so every parsed answer is the untranslated one.
    pub fn c_locale(argv: Vec<String>) -> Self {
        Self {
            argv,
            env: vec![("LC_ALL".into(), "C".into())],
            stdin: None,
        }
    }

    pub fn plain(argv: &[&str]) -> Self {
        Self {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            env: Vec::new(),
            stdin: None,
        }
    }
}

/// How a child ended: its exit code, -1 for a signal or a spawn that failed, and what it printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    pub fn code(code: i32) -> Self {
        Self {
            code,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Started,
    Exited(Outcome),
}

#[derive(Default)]
struct Control {
    kill: AtomicBool,
    withhold: AtomicBool,
}

/// The running job's controls and its event stream.
pub struct Handle {
    pub events: Receiver<Event>,
    control: Arc<Control>,
}

impl Handle {
    /// A handle with no process behind it, for a runner that only records jobs.
    pub fn detached() -> (Self, Sender<Event>) {
        let (send, events) = async_channel::unbounded();
        (
            Self {
                events,
                control: Arc::default(),
            },
            send,
        )
    }

    pub fn kill(&self) {
        self.control.kill.store(true, Ordering::SeqCst);
    }

    /// A handle with no process behind it and a probe that reads whether its kill was asked for.
    #[cfg(test)]
    pub fn probed() -> (Self, Sender<Event>, KillProbe) {
        let (send, events) = async_channel::unbounded();
        let control = Arc::<Control>::default();
        (
            Self {
                events,
                control: control.clone(),
            },
            send,
            KillProbe(control),
        )
    }

    /// Closes stdin with nothing written, if the child has not been handed its input yet.
    pub fn withhold_input(&self) {
        self.control.withhold.store(true, Ordering::SeqCst);
    }

    pub fn killed(&self) -> bool {
        self.control.kill.load(Ordering::SeqCst)
    }

    pub fn withheld(&self) -> bool {
        self.control.withhold.load(Ordering::SeqCst)
    }
}

/// One running job per slot, each start numbered, so a late exit of an older job never clears a newer one.
#[derive(Default)]
pub struct Slot {
    run: u64,
    handle: Option<Handle>,
}

impl Slot {
    /// Holds a new job's handle and returns the number its exit must carry.
    pub fn start(&mut self, handle: Handle) -> u64 {
        self.run += 1;
        self.handle = Some(handle);
        self.run
    }

    pub fn handle(&self) -> Option<&Handle> {
        self.handle.as_ref()
    }

    /// Kills the held job and retires its number, so its exit is never taken as an answer.
    pub fn kill(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.kill();
        }
        self.run += 1;
    }

    /// Kills the held job but keeps its number, so its exit still arrives as the current job's.
    pub fn stop(&self) {
        if let Some(handle) = &self.handle {
            handle.kill();
        }
    }

    /// True when job `run` is the one held, which then ends; false for an exit a later start or a kill retired.
    pub fn ended(&mut self, run: u64) -> bool {
        if run != self.run {
            return false;
        }
        self.handle = None;
        true
    }
}

/// Reads, from a test, whether the job behind a probed handle was killed.
#[cfg(test)]
pub struct KillProbe(Arc<Control>);

#[cfg(test)]
impl KillProbe {
    pub fn killed(&self) -> bool {
        self.0.kill.load(Ordering::SeqCst)
    }
}

/// Starts jobs; the GUI uses threads, tests record jobs and answer them by hand.
pub trait Runner {
    fn start(&self, job: Job) -> Handle;
}

/// One worker thread per child, polling for exit so a kill lands within one poll.
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadRunner;

impl Runner for ThreadRunner {
    fn start(&self, job: Job) -> Handle {
        let (send, events) = async_channel::unbounded();
        let control = Arc::new(Control::default());
        let worker = control.clone();
        let spawned = thread::Builder::new()
            .name("flea-device-job".into())
            .spawn(move || run(job, &worker, &send));
        if spawned.is_err() {
            let (handle, send) = Handle::detached();
            let _ = send.try_send(Event::Exited(Outcome {
                code: -1,
                ..Outcome::default()
            }));
            return handle;
        }
        Handle { events, control }
    }
}

fn run(mut job: Job, control: &Control, send: &Sender<Event>) {
    let Some((program, args)) = job.argv.split_first() else {
        let _ = send.send_blocking(Event::Exited(Outcome::code(-1)));
        return;
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(job.env.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own group, so a cancel also ends descendants that have not escaped it.
    command.process_group(0);
    command.stdin(if job.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = send.send_blocking(Event::Exited(Outcome {
                code: -1,
                stdout: String::new(),
                stderr: error.to_string(),
            }));
            return;
        }
    };
    let _ = send.send_blocking(Event::Started);
    let mut input = Input::new(child.stdin.take(), job.stdin.take());
    let mut pipes = match output::pipes(&mut child) {
        Ok(pipes) => pipes,
        Err(error) => {
            drop(input);
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            let _ = send.send_blocking(Event::Exited(Outcome {
                code: -1,
                stderr: format!("nonblocking output pipe setup failed: {error}"),
                ..Outcome::default()
            }));
            return;
        }
    };
    let mut exited_at = None;
    let mut cancelled = false;
    loop {
        if input
            .as_ref()
            .is_some_and(|i| i.withheld(control.withhold.load(Ordering::SeqCst)))
        {
            input = None;
        }
        if control.kill.load(Ordering::SeqCst) && !cancelled {
            input = None;
            kill_group(child.id());
            let _ = child.kill();
            cancelled = true;
        }
        if exited_at.is_none() {
            match leader_exited(child.id()) {
                Ok(false) => {}
                result => {
                    // A natural exit leaves the group alone, since setsid(1) forks before its child escapes it.
                    input = None;
                    if cancelled || result.is_err() {
                        kill_group(child.id());
                    }
                    if result.is_err() {
                        let _ = child.kill();
                    }
                    exited_at = Some(Instant::now());
                }
            }
        }
        if let Some(at) = exited_at {
            if !pipes.iter().any(Pipe::open) {
                break;
            }
            if at.elapsed() >= DRAIN_AFTER_EXIT {
                output::drain_buffered(&mut pipes);
                break;
            }
        }
        output::read_ready(&mut pipes, &mut input);
    }
    let [stdout, stderr] = pipes.map(Pipe::into_text);
    let status = child.wait().ok();
    let outcome = Outcome {
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        stdout,
        stderr,
    };
    let _ = send.send_blocking(Event::Exited(outcome));
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn waitid(kind: i32, id: u32, info: *mut std::ffi::c_void, options: i32) -> i32;
}

const SIGKILL: i32 = 9;

/// Observe a Linux child without reaping it, keeping its process group id reserved.
fn leader_exited(pid: u32) -> io::Result<bool> {
    const P_PID: i32 = 1;
    const WNOHANG: i32 = 1;
    const WEXITED: i32 = 4;
    const WNOWAIT: i32 = 0x01000000;
    // Linux siginfo_t is 128 bytes; u64 supplies its required alignment.
    let mut info = [0u64; 16];
    // SAFETY: pid is our unreaped child; info holds the entire siginfo_t the call writes.
    let rc = unsafe {
        waitid(
            P_PID,
            pid,
            info.as_mut_ptr().cast(),
            WEXITED | WNOHANG | WNOWAIT,
        )
    };
    if rc == 0 {
        // Linux clears si_signo when no exit is available, as in clip::own::owner_live.
        Ok(info[0] != 0)
    } else {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(error)
        }
    }
}

/// SIGKILL to the child's group; a pid that is not a real leader above 1 is never signalled.
fn kill_group(pid: u32) {
    let Ok(leader) = i32::try_from(pid) else {
        return;
    };
    if leader > 1 {
        // SAFETY: kill(2) takes plain integers, and a negative leader above 1 names only that group.
        unsafe { kill(-leader, SIGKILL) };
    }
}
