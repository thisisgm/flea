use crate::backend::cgroup::{Events, Scope};
use crate::backend::child::Ran;
use crate::backend::{fd, sandbox_frame, sandbox_gate, systemd_scope};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const READY: u8 = 0x52;
const STARTUP_LIMIT: Duration = Duration::from_secs(5);
#[cfg(not(test))]
const SUPERVISOR_SLACK: Duration = Duration::from_secs(8);
#[cfg(test)]
const SUPERVISOR_SLACK: Duration = Duration::from_secs(1);
const PR_SET_PDEATHSIG: i32 = 1;
const SIGKILL: i32 = 9;

extern "C" {
    fn close(fd: RawFd) -> i32;
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
    fn getppid() -> i32;
}

pub struct Broker {
    child: Child,
    output: File,
    healthy: bool,
}

pub enum Client {
    Fresh,
    Ready(Broker),
    Disabled,
}

impl Client {
    pub fn new() -> Self {
        Self::Fresh
    }

    pub fn run(&mut self, argv: &[String], timeout: Duration) -> Ran {
        self.run_request(argv, timeout, None)
    }

    pub fn run_checked(&mut self, argv: &[String], timeout: Duration, error: &Path) -> Ran {
        self.run_request(argv, timeout, Some(error))
    }

    fn run_request(&mut self, argv: &[String], timeout: Duration, error: Option<&Path>) -> Ran {
        if matches!(self, Self::Disabled) {
            return Ran::NotStarted;
        }
        if matches!(self, Self::Fresh) {
            match start() {
                Ok(broker) => *self = Self::Ready(broker),
                Err(error) => {
                    eprintln!("flea: thumbnail sandbox scope failed: {error}");
                    *self = Self::Disabled;
                    return Ran::NotStarted;
                }
            }
        }
        let Self::Ready(broker) = self else {
            return Ran::NotStarted;
        };
        let ran = broker.run_request(argv, timeout, error);
        if !broker.healthy {
            *self = Self::Fresh;
        }
        ran
    }
}

impl Broker {
    #[cfg(test)]
    fn run(&mut self, argv: &[String], timeout: Duration) -> Ran {
        self.run_request(argv, timeout, None)
    }

    fn run_request(&mut self, argv: &[String], timeout: Duration, error: Option<&Path>) -> Ran {
        match exchange(&mut self.child, &mut self.output, argv, timeout, error) {
            Ok((ran, events)) if events.max == 0 && events.oom == 0 && events.oom_kill == 0 => ran,
            Ok((_ran, events)) => {
                eprintln!("flea: thumbnail memory boundary reported max={}, oom={}, oom_kill={}", events.max, events.oom, events.oom_kill);
                Ran::Failed
            }
            Err(error) => {
                eprintln!("flea: thumbnail sandbox broker lost: {error}");
                self.stop();
                self.healthy = false;
                Ran::NotStarted
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn stop_for_test(&mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn start() -> io::Result<Broker> {
    let exe = std::env::current_exe()?;
    let pipe = fd::pipe()?;
    let output_fd = pipe.write.as_raw_fd();
    let inner = broker_command(exe.to_string_lossy().into_owned());
    let argv = systemd_scope::delegated(&inner);
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command.env("FLEA_BROKER_OUTPUT_FD", output_fd.to_string());
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit());
    unsafe {
        command.pre_exec(move || fd::inherit_on_exec(output_fd));
    }
    let child = command.spawn()?;
    drop(pipe.write);
    let mut broker = Broker { child, output: File::from(pipe.read), healthy: true };
    let mut ready = [0u8; 1];
    if let Err(error) = fd::read_exact_until(&mut broker.output, &mut ready, Instant::now() + STARTUP_LIMIT) {
        broker.stop();
        return Err(error);
    }
    if ready != [READY] {
        broker.stop();
        return Err(io::Error::other("invalid broker ready frame"));
    }
    Ok(broker)
}

fn broker_command(exe: String) -> Vec<String> {
    #[cfg(not(test))]
    return vec![exe, "--sandbox-broker".to_string()];
    #[cfg(test)]
    return vec![exe, "--exact".to_string(), "backend::sandbox_broker::tests::sandbox_broker_child".to_string(), "--nocapture".to_string()];
}

fn exchange(child: &mut Child, output: &mut File, argv: &[String], timeout: Duration, error: Option<&Path>) -> io::Result<(Ran, Events)> {
    let input = child.stdin.as_mut().ok_or_else(|| io::Error::other("broker input closed"))?;
    sandbox_frame::write_request(input, argv, timeout, error)?;
    input.flush()?;
    let mut bytes = [0u8; sandbox_frame::RESULT_BYTES];
    fd::read_exact_until(output, &mut bytes, Instant::now() + timeout + SUPERVISOR_SLACK)?;
    sandbox_frame::read_result(&mut bytes.as_slice())
}

pub fn broker_main() -> i32 {
    if connect_broker_output().and_then(|_| install_parent_death_signal()).is_err() {
        return 2;
    }
    let mut scope = match Scope::enter() {
        Ok(scope) => scope,
        Err(error) => {
            eprintln!("flea: thumbnail sandbox scope failed: {error}");
            return 2;
        }
    };
    let mut output = io::stdout().lock();
    if output.write_all(&[READY]).and_then(|_| output.flush()).is_err() {
        return 2;
    }
    let mut input = io::stdin().lock();
    loop {
        let request = match sandbox_frame::read_request(&mut input) {
            Ok(Some(request)) => request,
            Ok(None) => return 0,
            Err(_) => return 2,
        };
        #[cfg(test)]
        if request.argv == ["__flea_broker_crash__"] {
            return 86;
        }
        #[cfg(test)]
        if request.argv == ["__flea_broker_stall__"] {
            std::thread::sleep(Duration::from_secs(60));
            return 86;
        }
        let error = request.exec_error.as_deref().map(Path::new);
        let (ran, events) = sandbox_gate::run(&mut scope, &request.argv, request.timeout, error);
        if sandbox_frame::write_result(&mut output, ran, &events).and_then(|_| output.flush()).is_err() {
            return 2;
        }
    }
}

fn connect_broker_output() -> io::Result<()> {
    let raw: RawFd = std::env::var("FLEA_BROKER_OUTPUT_FD").ok().and_then(|value| value.parse().ok()).ok_or_else(|| io::Error::other("broker output descriptor is missing"))?;
    if let Err(error) = fd::duplicate_to(raw, 1) {
        unsafe { close(raw) };
        return Err(error);
    }
    if raw != 1 {
        unsafe { close(raw) };
    }
    Ok(())
}

fn install_parent_death_signal() -> io::Result<()> {
    let parent = unsafe { getppid() };
    if unsafe { prctl(PR_SET_PDEATHSIG, SIGKILL as u64, 0, 0, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { getppid() } != parent || parent == 1 {
        return Err(io::Error::other("broker parent exited during setup"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "sandbox_broker_tests.rs"]
mod tests;
