//! One child process per job, run off the UI thread; its start and its outcome come back over a channel.
use super::secret::Secret;
use async_channel::{Receiver, Sender};
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// The most output kept from one stream; the rest is drained and dropped.
pub const OUTPUT_CAP: usize = 1 << 20;

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
        Self { argv, env: vec![("LC_ALL".into(), "C".into())], stdin: None }
    }

    pub fn plain(argv: &[&str]) -> Self {
        Self { argv: argv.iter().map(|s| s.to_string()).collect(), env: Vec::new(), stdin: None }
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
        Self { code, ..Self::default() }
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
        (Self { events, control: Arc::default() }, send)
    }

    pub fn kill(&self) {
        self.control.kill.store(true, Ordering::SeqCst);
    }

    /// A handle with no process behind it and a probe that reads whether its kill was asked for.
    #[cfg(test)]
    pub fn probed() -> (Self, Sender<Event>, KillProbe) {
        let (send, events) = async_channel::unbounded();
        let control = Arc::<Control>::default();
        (Self { events, control: control.clone() }, send, KillProbe(control))
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
        let spawned = thread::Builder::new().name("flea-device-job".into()).spawn(move || run(job, &worker, &send));
        if spawned.is_err() {
            let (handle, send) = Handle::detached();
            let _ = send.try_send(Event::Exited(Outcome { code: -1, ..Outcome::default() }));
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
    command.args(args).envs(job.env.iter().map(|(k, v)| (k, v))).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Its own group, so a kill also ends what a shell job started and nothing is left holding the pipes.
    command.process_group(0);
    command.stdin(if job.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = send.send_blocking(Event::Exited(Outcome { code: -1, stdout: String::new(), stderr: error.to_string() }));
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
    let out = child.stdout.take().map(|s| thread::spawn(move || drain(s)));
    let err = child.stderr.take().map(|s| thread::spawn(move || drain(s)));
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
    let collect = |reader: Option<thread::JoinHandle<String>>| reader.and_then(|r| r.join().ok()).unwrap_or_default();
    let outcome = Outcome {
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        stdout: collect(out),
        stderr: collect(err),
    };
    let _ = send.send_blocking(Event::Exited(outcome));
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGKILL: i32 = 9;

/// SIGKILL to the child's group; a pid that is not a real leader above 1 is never signalled.
fn kill_group(pid: u32) {
    let Ok(leader) = i32::try_from(pid) else { return };
    if leader > 1 {
        // SAFETY: kill(2) takes plain integers, and a negative leader above 1 names only that group.
        unsafe { kill(-leader, SIGKILL) };
    }
}

fn drain(mut stream: impl Read) -> String {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(n) = stream.read(&mut chunk) {
        if n == 0 {
            break;
        }
        let room = OUTPUT_CAP.saturating_sub(kept.len());
        kept.extend_from_slice(&chunk[..n.min(room)]);
    }
    String::from_utf8_lossy(&kept).into_owned()
}
