//! One child process per job, run off the UI thread; its start and its outcome come back over a channel.
use super::secret::Secret;
use async_channel::{Receiver, Sender};
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The most output kept from one stream; the rest is drained and dropped.
pub const OUTPUT_CAP: usize = 1 << 20;

/// How long a killed job waits for its pipes to close before it closes them from its own end.
const ABANDON_AFTER: Duration = Duration::from_secs(1);

/// The longest one wait for output lasts, so a kill lands within one wait.
const WAIT: Duration = Duration::from_millis(5);

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
    // Its own group, so a kill also ends what a shell job started and nothing is left holding the pipes.
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
    if let (Some(mut input), Some(mut secret)) = (child.stdin.take(), job.stdin.take()) {
        if !control.withhold.load(Ordering::SeqCst) {
            let _ = input.write_all(secret.expose());
        }
        secret.clear();
    }
    let mut pipes = [
        Pipe::new(child.stdout.take()),
        Pipe::new(child.stderr.take()),
    ];
    // Pipes close before the leader is reaped, so a kill never names a group id the kernel has freed.
    let mut killed = None;
    while pipes.iter().any(Pipe::open) {
        if control.kill.load(Ordering::SeqCst) {
            kill_group(child.id());
            // A pipe held outside the group outlives the kill, so this end closes it instead.
            if killed.get_or_insert_with(Instant::now).elapsed() >= ABANDON_AFTER {
                break;
            }
        }
        read_ready(&mut pipes);
    }
    let status = loop {
        if control.kill.load(Ordering::SeqCst) {
            kill_group(child.id());
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => break None,
        }
    };
    let [stdout, stderr] = pipes.map(Pipe::into_text);
    let outcome = Outcome {
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        stdout,
        stderr,
    };
    let _ = send.send_blocking(Event::Exited(outcome));
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
}

const SIGKILL: i32 = 9;
const POLLIN: i16 = 1;

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

/// One output pipe of a job and what it printed, kept up to OUTPUT_CAP.
struct Pipe {
    file: Option<File>,
    kept: Vec<u8>,
}

impl Pipe {
    fn new(stream: Option<impl Into<OwnedFd>>) -> Self {
        Self {
            file: stream.map(|stream| File::from(stream.into())),
            kept: Vec::new(),
        }
    }

    fn open(&self) -> bool {
        self.file.is_some()
    }

    /// Reads once after poll marks the pipe ready; end of file or a read error closes it.
    fn read(&mut self) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let mut chunk = [0u8; 8192];
        match file.read(&mut chunk) {
            Ok(n) if n > 0 => {
                let room = OUTPUT_CAP.saturating_sub(self.kept.len());
                self.kept.extend_from_slice(&chunk[..n.min(room)]);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            _ => self.file = None,
        }
    }

    fn into_text(self) -> String {
        String::from_utf8_lossy(&self.kept).into_owned()
    }
}

/// Waits up to WAIT for an open pipe to have output or close, then reads each that does.
fn read_ready(pipes: &mut [Pipe; 2]) {
    // poll(2) skips a record whose fd is negative, so a closed pipe stays in place as -1.
    let mut fds = pipes.each_ref().map(|pipe| PollFd {
        fd: pipe.file.as_ref().map_or(-1, |file| file.as_raw_fd()),
        events: POLLIN,
        revents: 0,
    });
    // SAFETY: fds is a live array of two records, and each open fd stays owned by its pipe for the call.
    let ready = unsafe { poll(fds.as_mut_ptr(), fds.len(), WAIT.as_millis() as i32) };
    if ready < 0 {
        thread::sleep(WAIT);
        return;
    }
    for (pipe, fd) in pipes.iter_mut().zip(fds) {
        if fd.revents != 0 {
            pipe.read();
        }
    }
}
