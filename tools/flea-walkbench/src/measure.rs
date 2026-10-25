//! Fresh-process trials: CLOCK_MONOTONIC wall, wait4 CPU and peak RSS, cache gate and coverage checks.
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

/// One measured child process, as raw values; nothing is normalised or capped.
/// user_us, sys_us and maxrss_kib are the wait4 view of the whole child, including post-clock coverage work.
pub struct Trial {
    pub wall_ns: u128,
    pub stage_ns: u128,
    pub user_us: i64,
    pub sys_us: i64,
    pub maxrss_kib: i64,
    pub stage_cpu_us: i64,
    pub hwm_kib: i64,
    pub spawn_hwm_kib: i64,
    pub tool_maxrss_kib: i64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    pub digest: String,
    pub exit: i32,
}

pub fn rusage_wait(pid: libc::pid_t) -> Result<(i32, libc::rusage), String> {
    let mut status = 0;
    // SAFETY: rusage is plain data and wait4 fills it for our own child.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: pid is a child we spawned and have not yet reaped.
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
    if reaped != pid {
        return Err(format!("wait4 failed: {}", std::io::Error::last_os_error()));
    }
    Ok((status, usage))
}

/// Runs `flea-walkbench adapter ...` as a fresh process and reads its one result line.
pub fn trial(
    exe: &Path,
    adapter: &str,
    tree: &Path,
    threads: usize,
    mode: &str,
    tool: Option<&Path>,
) -> Result<Trial, String> {
    let mut command = Command::new(exe);
    command
        .args([
            "adapter",
            adapter,
            "--mode",
            mode,
            "--threads",
            &threads.to_string(),
            "--tree",
        ])
        .arg(tree);
    if let Some(tool) = tool {
        command.arg("--tool").arg(tool);
    }
    let started = Instant::now();
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    let pid = child.id() as libc::pid_t;
    let mut stdout = child.stdout.ok_or("no child stdout")?;
    let mut line = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut line).map_err(|e| e.to_string())?;
    let (status, usage) = rusage_wait(pid)?;
    let wall_ns = started.elapsed().as_nanos();
    let exit = std::process::ExitStatus::from_raw(status)
        .code()
        .unwrap_or(-1);
    let fields: Vec<&str> = line.trim_end().split('\t').collect();
    if fields.len() != 10 || fields[0] != "RESULT" {
        return Err(format!(
            "{adapter}: malformed child result {line:?} exit {exit}"
        ));
    }
    let number = |i: usize| {
        fields[i]
            .parse::<u128>()
            .map_err(|e| format!("{adapter}: field {i}: {e}"))
    };
    let micros = |t: libc::timeval| t.tv_sec * 1_000_000 + t.tv_usec;
    Ok(Trial {
        wall_ns,
        stage_ns: number(1)?,
        files: number(2)? as u64,
        dirs: number(3)? as u64,
        errors: number(4)? as u64,
        digest: fields[5].to_string(),
        user_us: micros(usage.ru_utime),
        sys_us: micros(usage.ru_stime),
        maxrss_kib: usage.ru_maxrss,
        stage_cpu_us: fields[6]
            .parse()
            .map_err(|e| format!("{adapter}: cpu: {e}"))?,
        hwm_kib: fields[7]
            .parse()
            .map_err(|e| format!("{adapter}: hwm: {e}"))?,
        spawn_hwm_kib: fields[8]
            .parse()
            .map_err(|e| format!("{adapter}: spawn hwm: {e}"))?,
        tool_maxrss_kib: fields[9]
            .parse()
            .map_err(|e| format!("{adapter}: tool rss: {e}"))?,
        exit,
    })
}

/// Cold runs need the controller's page-cache drop hook; without one the run refuses rather than mislabel warm data.
pub fn cache_gate(cache: &str, hook: Option<&str>) -> Result<&'static str, String> {
    match (cache, hook) {
        ("warm", _) => Ok("warm"),
        ("cold", Some(hook)) => {
            let status = Command::new("/bin/sh")
                .arg("-c")
                .arg(hook)
                .stdin(Stdio::null())
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok("cold-hook-ok")
            } else {
                Err(format!("cold hook failed: {status}"))
            }
        }
        ("cold", None) => Err(
            "cold cache needs --cold-hook from the controller; refusing to label warm data cold"
                .into(),
        ),
        (other, _) => Err(format!("unknown cache mode {other}")),
    }
}

/// Coverage status against the census: ok, or the first way the entrant's set differs.
pub fn coverage(
    trial: &Trial,
    files: u64,
    dirs: u64,
    full: &str,
    files_only: &str,
    lists_dirs: bool,
    index: bool,
) -> &'static str {
    if trial.exit != 0 || trial.errors > 0 {
        return "error";
    }
    if trial.files != files || (lists_dirs && trial.dirs != dirs) {
        return "count-mismatch";
    }
    if index && trial.digest != if lists_dirs { full } else { files_only } {
        return "identity-mismatch";
    }
    "ok"
}
