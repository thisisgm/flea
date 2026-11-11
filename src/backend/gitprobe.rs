// Repository discovery never starts Git, and the status process sees only read-only mounts.
use flea::bytename::ExactText as _;
use super::gitstatus::budget::Budget;
use super::{jail, regfile, sandbox};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub const DEADLINE: Duration = Duration::from_secs(5);
const OUTPUT_LIMIT: usize = 32 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;
const METADATA_LIMIT_BYTES: usize = 4096;
const READ_CHUNK_BYTES: usize = 8192;
const DRAIN_ALLOCATION_OVERHEAD_BYTES: usize = 128;

#[derive(Debug)]
pub struct Repository {
    pub root: PathBuf,
    pub directory: PathBuf,
    git: PathBuf,
    common: PathBuf,
}

pub fn discover(directory: &Path) -> Result<Option<Repository>, String> {
    if !directory.is_absolute() {
        return Err("Git status needs an absolute directory.".into());
    }
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("Git status: {error}"))?;
    if !directory.is_dir() {
        return Err("Git status needs a directory.".into());
    }
    for root in directory.ancestors() {
        let dotgit = root.join(".git");
        let meta = match dotgit.metadata() {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Git status: {error}")),
        };
        let git = if meta.is_dir() {
            dotgit
        } else if meta.is_file() {
            let bytes = small_file(&dotgit)?;
            // Sample input: b"gitdir: ../.git/worktrees/linked\n".
            let name = bytes
                .strip_prefix(b"gitdir: ")
                .ok_or("Git status found an invalid gitdir file.")?;
            root.join(raw_path(name.strip_suffix(b"\n").unwrap_or(name)))
        } else {
            return Err("Git status found an unsupported .git entry.".into());
        };
        let git = git
            .canonicalize()
            .map_err(|error| format!("Git status: {error}"))?;
        let common_file = git.join("commondir");
        let common = if common_file
            .try_exists()
            .map_err(|error| format!("Git status: {error}"))?
        {
            let bytes = small_file(&common_file)?;
            // Sample input: b"../..\n", relative to the Git metadata directory.
            git.join(raw_path(bytes.strip_suffix(b"\n").unwrap_or(&bytes)))
                .canonicalize()
                .map_err(|error| format!("Git status: {error}"))?
        } else {
            git.clone()
        };
        if [&directory, &git, &common]
            .iter()
            .any(|path| path.to_str().is_none())
        {
            return Err("Git status needs UTF-8 repository directory names.".into());
        }
        return Ok(Some(Repository {
            root: root.to_path_buf(),
            directory: directory.clone(),
            git,
            common,
        }));
    }
    Ok(None)
}

fn raw_path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(bytes.to_vec()).into()
}

fn small_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    regfile::open_regular(path)
        .ok_or("Git status could not read repository metadata.")?
        .take((METADATA_LIMIT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Git status: {error}"))?;
    if bytes.len() > METADATA_LIMIT_BYTES {
        return Err("Git status repository metadata is too large.".into());
    }
    Ok(bytes)
}

pub fn argv(repo: &Repository) -> Vec<String> {
    let inner: Vec<String> = [
        "/usr/bin/git",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "status.renames=true",
        "status",
        "--porcelain=v2",
        "-z",
        "--branch",
        "--untracked-files=all",
        "--ignore-submodules=none",
        "--",
    ]
    .map(str::to_string)
    .into();
    let mut full = sandbox::wrap_readonly_extra(&inner, &[&repo.root, &repo.git, &repo.common]);
    let at = full.len() - inner.len();
    full.splice(
        at..at,
        [
            "--chdir".into(),
            repo.root.exact_text().into_owned(),
            "--setenv".into(),
            "GIT_OPTIONAL_LOCKS".into(),
            "0".into(),
            "--setenv".into(),
            "GIT_CONFIG_NOSYSTEM".into(),
            "1".into(),
            "--setenv".into(),
            "GIT_CONFIG_GLOBAL".into(),
            "/dev/null".into(),
            "--setenv".into(),
            "GIT_TERMINAL_PROMPT".into(),
            "0".into(),
        ],
    );
    sandbox::add_status(&mut full, inner.len());
    full
}

#[cfg(test)]
pub fn status(repo: &Repository) -> Result<super::gitstatus::Snapshot, String> {
    status_with(repo, &|| false)
}

pub(super) fn status_with(
    repo: &Repository,
    cancelled: &dyn Fn() -> bool,
) -> Result<super::gitstatus::Snapshot, String> {
    let budget = Budget::new(Instant::now() + DEADLINE, cancelled);
    budget.check()?;
    if !sandbox::available() {
        return Err("Git status sandbox is unavailable.".into());
    }
    let bytes = capture_budget(&argv(repo), &budget)?;
    super::gitstatus::parse_with(&bytes, &budget)
}

fn drain(reader: impl Read + Send + 'static, limit: usize) -> Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(limit + 1)
                .map_err(|_| "Git status could not allocate its snapshot.")?;
            if bytes.capacity() > limit + 1 {
                return Err("Git status could not allocate its snapshot.".into());
            }
            let mut reader = reader;
            let mut chunk = [0; READ_CHUNK_BYTES];
            loop {
                let remaining = (limit + 1 - bytes.len()).min(chunk.len());
                let count = match reader.read(&mut chunk[..remaining]) {
                    Ok(count) => count,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(format!("Git status: {error}")),
                };
                if count == 0 {
                    return Ok(bytes);
                }
                bytes.extend_from_slice(&chunk[..count]);
                if bytes.len() > limit {
                    return Err("Git status output is too large.".into());
                }
            }
        })();
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
pub(super) fn capture(full: &[String], timeout: Duration) -> Result<Vec<u8>, String> {
    capture_with(full, timeout, &|| false)
}

#[cfg(test)]
pub(super) fn capture_with(
    full: &[String],
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let budget = Budget::new(Instant::now() + timeout, cancelled);
    capture_budget(full, &budget)
}

fn capture_budget(full: &[String], budget: &Budget<'_>) -> Result<Vec<u8>, String> {
    // Reserve both complete drain buffers and allocation overhead before either thread starts.
    budget.claim(OUTPUT_LIMIT + 1 + STDERR_LIMIT + 1 + DRAIN_ALLOCATION_OVERHEAD_BYTES)?;
    let mut child = jail::spawn_jailed(full, |cmd| {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    })
    .map_err(|error| format!("Git status: {error}"))?;
    let stdout = drain(child.child.stdout.take().unwrap(), OUTPUT_LIMIT);
    let stderr = drain(child.child.stderr.take().unwrap(), STDERR_LIMIT);
    let (mut out, mut err, mut exit) = (None, None, None);
    let failure = loop {
        if let Err(error) = budget.check() {
            break error;
        }
        if out.is_none() {
            out = stdout.try_recv().ok();
        }
        if err.is_none() {
            err = stderr.try_recv().ok();
        }
        if let Some(Err(error)) = out
            .as_ref()
            .filter(|value| value.is_err())
            .or_else(|| err.as_ref().filter(|value| value.is_err()))
        {
            break error.clone();
        }
        match child.child.try_wait() {
            Ok(Some(status)) => exit = Some(status),
            Ok(None) => {}
            Err(error) => break format!("Git status: {error}"),
        }
        if let (Some(Ok(_)), Some(Ok(message)), Some(status)) = (&out, &err, exit) {
            if status.success() {
                budget.check()?;
                return out.take().unwrap();
            }
            return Err(format!(
                "Git status failed ({status}): {}",
                String::from_utf8_lossy(message).trim()
            ));
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    jail::kill_tree(&mut child);
    let message = err.or_else(|| stderr.recv_timeout(Duration::from_millis(100)).ok());
    match message {
        Some(Ok(bytes)) if !bytes.is_empty() => Err(format!(
            "{failure} {}",
            String::from_utf8_lossy(&bytes).trim()
        )),
        _ => Err(failure),
    }
}

#[cfg(test)]
#[path = "gitprobe_budget_tests.rs"]
mod budget_tests;
