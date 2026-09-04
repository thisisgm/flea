use crate::backend::cgroup::{Events, Job, Scope, LIMITS};
use crate::backend::child::{wait_for_exit, Ran};
use crate::backend::fd;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(not(test))]
const SETUP_LIMIT: Duration = Duration::from_secs(5);
#[cfg(test)]
const SETUP_LIMIT: Duration = Duration::from_millis(500);
const CLEANUP_LIMIT: Duration = Duration::from_secs(2);
const ERROR_MAX: usize = 4096;

pub fn run(scope: &mut Scope, argv: &[String], timeout: Duration, exec_error: Option<&Path>) -> (Ran, Events) {
    run_job(scope, argv, timeout, LIMITS, None, exec_error)
}

pub(crate) fn run_for_test(scope: &mut Scope, argv: &[String], timeout: Duration, limits: crate::backend::cgroup::Limits, placement_pid: Option<u32>) -> (Ran, Events) {
    run_job(scope, argv, timeout, limits, placement_pid, None)
}

fn run_job(scope: &mut Scope, argv: &[String], timeout: Duration, limits: crate::backend::cgroup::Limits, placement_pid: Option<u32>, exec_error: Option<&Path>) -> (Ran, Events) {
    let job = match scope.job(limits) {
        Ok(job) => job,
        Err(error) => {
            eprintln!("flea: thumbnail cgroup setup failed: {error}");
            return (Ran::NotStarted, Events::default());
        }
    };
    let (mut gate, mut release, mut errors) = match spawn(argv) {
        Ok(parts) => parts,
        Err(error) => {
            eprintln!("flea: thumbnail gate start failed: {error}");
            return finish(job, None, Ran::NotStarted);
        }
    };
    let pid = placement_pid.unwrap_or_else(|| gate.id());
    if let Err(error) = job.place(pid).and_then(|_| release.write_all(&[1])) {
        eprintln!("flea: thumbnail gate placement or release failed: {error}");
        return finish(job, Some(&mut gate), Ran::NotStarted);
    }
    drop(release);
    match fd::read_to_end_until(&mut errors, ERROR_MAX, Instant::now() + SETUP_LIMIT) {
        Ok(bytes) if bytes.is_empty() => {}
        Ok(bytes) => {
            eprintln!("flea: thumbnail gate exec failed: {}", String::from_utf8_lossy(&bytes));
            return finish(job, Some(&mut gate), Ran::NotStarted);
        }
        Err(error) => {
            eprintln!("flea: thumbnail gate acknowledgement failed: {error}");
            return finish(job, Some(&mut gate), Ran::NotStarted);
        }
    }
    let ran = match wait_for_exit(&mut gate, timeout) {
        Ok(Some(status)) if status.success() => Ran::Succeeded,
        Ok(Some(_)) | Ok(None) => Ran::Failed,
        Err(_) => Ran::NotStarted,
    };
    let ran = final_exec_result(exec_error, ran);
    finish(job, Some(&mut gate), ran)
}

fn final_exec_result(path: Option<&Path>, ran: Ran) -> Ran {
    let Some(path) = path else { return ran };
    match std::fs::read(path) {
        Ok(bytes) if bytes.is_empty() => ran,
        Ok(bytes) => {
            eprintln!("flea: thumbnail decoder exec failed: {}", String::from_utf8_lossy(&bytes));
            Ran::NotStarted
        }
        Err(error) => {
            eprintln!("flea: thumbnail decoder exec result failed: {error}");
            Ran::NotStarted
        }
    }
}

fn finish(job: Job, child: Option<&mut Child>, ran: Ran) -> (Ran, Events) {
    let accounting = job.finish();
    let reaped = child.map_or(Ok(()), reap);
    match (accounting, reaped) {
        (Ok(events), Ok(())) => (ran, events),
        (accounting, reaped) => {
            let error = accounting.err().or_else(|| reaped.err()).unwrap();
            eprintln!("flea: thumbnail cgroup cleanup failed: {error}");
            (Ran::NotStarted, Events::default())
        }
    }
}

fn reap(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    child.kill()?;
    match wait_for_exit(child, CLEANUP_LIMIT)? {
        Some(_) => Ok(()),
        None => Err(io::Error::new(io::ErrorKind::TimedOut, "gate reap deadline")),
    }
}

fn spawn(argv: &[String]) -> io::Result<(Child, ChildStdin, File)> {
    let exe = std::env::current_exe()?;
    let pipe = fd::pipe()?;
    let error_fd = pipe.write.as_raw_fd();
    let mut command = Command::new(exe);
    configure(&mut command, argv);
    command.env("FLEA_GATE_ERROR_FD", error_fd.to_string());
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    unsafe {
        command.pre_exec(move || fd::inherit_on_exec(error_fd));
    }
    let mut child = command.spawn()?;
    drop(pipe.write);
    let release = match child.stdin.take() {
        Some(release) => release,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("gate input missing"));
        }
    };
    Ok((child, release, File::from(pipe.read)))
}

fn configure(command: &mut Command, argv: &[String]) {
    #[cfg(not(test))]
    {
        command.arg("--sandbox-gate").arg("--").args(argv);
    }
    #[cfg(test)]
    {
        command.args(["--exact", "backend::sandbox_broker::tests::sandbox_gate_child", "--nocapture"]).env("FLEA_GATE_TEST_ARGV", argv.join("\u{1f}"));
    }
}
