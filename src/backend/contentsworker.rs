use flea::bytename::ExactText as _;
use super::{
    parse::{self, Hit, Record},
    Ask, Outcome, Snapshot,
};
use crate::backend::{jail, sandbox, sort::name_order};
use flea::ui_logic_sr1::ContentsMatch;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const RESULT_LIMIT: usize = 64 * 1024 * 1024;
const ERROR_LIMIT: usize = 64 * 1024;
const WALL_LIMIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);

#[cfg(test)]
#[derive(Default)]
pub(crate) struct Probe {
    pub launcher: std::sync::atomic::AtomicI32,
    pub sandbox: std::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicI32>>>,
    pub hold: AtomicBool,
    pub before_spawn: AtomicBool,
    pub reached: AtomicBool,
    pub reaped: AtomicBool,
}

fn nonblocking(fd: i32) -> std::io::Result<()> {
    extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    const GET_FLAGS: i32 = 3;
    const SET_FLAGS: i32 = 4;
    let flags = unsafe { fcntl(fd, GET_FLAGS) };
    if flags < 0 || unsafe { fcntl(fd, SET_FLAGS, flags | crate::oflags::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// rg's line for a path it could not open or read names the path, then the OS error: "rg: ./Locked: Permission denied (os error 13)".
fn skipped_path(line: &[u8]) -> bool {
    let Some(code) = line
        .strip_prefix(b"rg: ")
        .and_then(|rest| rest.strip_suffix(b")"))
    else {
        return false;
    };
    let Some(at) = code.windows(10).rposition(|part| part == b"(os error ") else {
        return false;
    };
    let digits = &code[at + 10..];
    // A line with no path before its first ": ", as "rg: No space left on device (os error 28)", is rg's own failure.
    let named = code[..at]
        .windows(2)
        .position(|part| part == b": ")
        .is_some_and(|end| end > 0);
    named && !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

/// rg's warning for an ignore file line it cannot parse, prefixed only on the first: "rg: ./.ignore: line 1: error parsing glob 'a{b': ..." then "./.ignore: line 3: ...".
fn ignore_warning(line: &[u8]) -> bool {
    const LINE: &[u8] = b": line ";
    let rest = line.strip_prefix(b"rg: ").unwrap_or(line);
    let Some(at) = rest.windows(LINE.len()).position(|part| part == LINE) else {
        return false;
    };
    let tail = &rest[at + LINE.len()..];
    let digits = tail.iter().take_while(|byte| byte.is_ascii_digit()).count();
    at > 0 && digits > 0 && tail[digits..].starts_with(b": error parsing glob ")
}

fn program() -> Result<PathBuf, String> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|path| path.is_absolute())
        .map(|path| path.join("rg"))
        .find(|path| jail::executable_file(path))
        .ok_or_else(|| super::message("search.contents.missing", ""))
}

fn argv(ask: &Ask, tool: &Path) -> Vec<String> {
    let mut inner = vec![
        tool.exact_text().into_owned(),
        "--no-config".into(),
        "--json".into(),
        "--fixed-strings".into(),
        "--max-count=1".into(),
        "--no-mmap".into(),
        "--threads=1".into(),
        "--no-require-git".into(),
        "--color=never".into(),
        "--encoding=none".into(),
    ];
    if ask.hidden {
        inner.push("--hidden".into());
    }
    inner.extend(["--".into(), ask.query.clone(), ".".into()]);
    inner
}

pub(super) fn run(
    ask: &Ask,
    stop: &AtomicBool,
    #[cfg(test)] tool: Option<PathBuf>,
    #[cfg(test)] probe: &Probe,
) -> Outcome {
    let result = execute(
        ask,
        stop,
        #[cfg(test)]
        tool,
        #[cfg(test)]
        probe,
    );
    #[cfg(test)]
    probe.reaped.store(true, Ordering::Release);
    match result {
        Ok(Some(snapshot)) => Outcome::Ready(snapshot),
        Ok(None) => Outcome::Cancelled,
        Err(error) => Outcome::Failed(error),
    }
}

fn execute(
    ask: &Ask,
    stop: &AtomicBool,
    #[cfg(test)] tool: Option<PathBuf>,
    #[cfg(test)] probe: &Probe,
) -> Result<Option<Snapshot>, String> {
    let started = Instant::now();
    if stop.load(Ordering::Acquire) {
        return Ok(None);
    }
    let root = flea::bytename::view(&ask.path);
    let root = &*root;
    let meta = root.metadata().map_err(|error| error.to_string())?;
    if !meta.is_dir() {
        return Err("not a directory".into());
    }
    #[cfg(not(test))]
    let tool = program()?;
    #[cfg(test)]
    let tool = tool.map_or_else(program, Ok)?;
    if !jail::executable_file(&tool) {
        return Err(super::message("search.contents.missing", ""));
    }
    let inner = argv(ask, &tool);
    let mut full = sandbox::wrap_readonly_extra(&inner, &[root, &tool]);
    sandbox::add_status(&mut full, inner.len());
    #[cfg(test)]
    if probe.before_spawn.load(Ordering::Acquire) {
        probe.reached.store(true, Ordering::Release);
        while probe.hold.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
            std::thread::sleep(POLL);
        }
    }
    if stop.load(Ordering::Acquire) {
        return Ok(None);
    }
    let mut child = jail::spawn_jailed(&full, |command| {
        command
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    })
    .map_err(|error| error.to_string())?;
    #[cfg(test)]
    {
        probe
            .launcher
            .store(child.child.id() as i32, Ordering::Release);
        *probe.sandbox.lock().unwrap() = Some(child.sandbox_pid.clone());
        if !probe.before_spawn.load(Ordering::Acquire) {
            probe.reached.store(true, Ordering::Release);
            while probe.hold.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
                std::thread::sleep(POLL);
            }
        }
    }
    let mut reaped = false;
    let result = collect(&mut child, stop, started, &mut reaped);
    if !reaped {
        jail::kill_tree(&mut child);
    }
    let Some((mut hits, scanned, partial)) = result? else {
        return Ok(None);
    };
    hits.sort_by(|a, b| ranked(&a.path, &b.path));
    let matches = hits
        .into_iter()
        .enumerate()
        .map(|(row, hit)| {
            let path =
                flea::bytename::path_text(Path::new(OsStr::from_bytes(&hit.path))).into_owned();
            let operable = true;
            ContentsMatch {
                row,
                path,
                path_bytes: hit.path,
                line_no: hit.line_no,
                line: hit.text,
                operable,
            }
        })
        .collect();
    Ok(Some(Snapshot {
        ask: ask.clone(),
        matches,
        scanned,
        dev: meta.dev(),
        writable: super::super::ops::dir_writable(root),
        ms: started.elapsed().as_secs_f64() * 1000.,
        partial,
    }))
}

// CHANGES.md search-ranked: nearest the searched folder first, then by the file's own name, then by its folders.
pub(super) fn ranked(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    fn depth(path: &[u8]) -> usize {
        path.iter().filter(|&&byte| byte == b'/').count()
    }
    fn name(path: &[u8]) -> &[u8] {
        path.rsplit(|&byte| byte == b'/').next().unwrap_or(path)
    }
    depth(a)
        .cmp(&depth(b))
        .then_with(|| name_order(name(a), name(b)))
        .then_with(|| name_order(a, b))
}

fn collect(
    child: &mut jail::Jailed,
    stop: &AtomicBool,
    started: Instant,
    reaped: &mut bool,
) -> Result<Option<(Vec<Hit>, usize, bool)>, String> {
    let mut stdout = child.child.stdout.take().ok_or("missing ripgrep stdout")?;
    let mut stderr = child.child.stderr.take().ok_or("missing ripgrep stderr")?;
    nonblocking(stdout.as_raw_fd()).map_err(|error| error.to_string())?;
    nonblocking(stderr.as_raw_fd()).map_err(|error| error.to_string())?;
    let mut pending = Vec::new();
    let mut checked = 0;
    let mut errors = Vec::new();
    // The stderr line still arriving, and whether a finished one named anything but a path rg skipped.
    let mut line = Vec::new();
    let mut foreign = false;
    let mut hits: HashMap<Vec<u8>, Hit> = HashMap::new();
    let mut bytes = 0usize;
    let mut summary = None;
    let mut eof = false;
    let mut err_eof = false;
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(None);
        }
        if started.elapsed() >= WALL_LIMIT {
            return Err(super::message("search.contents.timeout", ""));
        }
        let mut chunk = [0; 8192];
        let mut progress = false;
        match stdout.read(&mut chunk) {
            Ok(0) => eof = true,
            Ok(n) => {
                progress = true;
                pending.extend_from_slice(&chunk[..n]);
                while let Some(end) = pending[checked..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map(|relative| checked + relative)
                {
                    if end > RESULT_LIMIT {
                        return Err(super::message("search.contents.limit", ""));
                    }
                    match parse::parse(&pending[..end])? {
                        Record::Hit(hit) => {
                            if !hits.contains_key(&hit.path) {
                                bytes = bytes.saturating_add(hit.path.len() + hit.text.len());
                                if bytes > RESULT_LIMIT {
                                    return Err(super::message("search.contents.limit", ""));
                                }
                                hits.insert(hit.path.clone(), hit);
                            }
                        }
                        Record::End { path, binary: true } => {
                            hits.remove(&path);
                        }
                        Record::Summary(searches) => summary = Some(searches),
                        _ => {}
                    }
                    pending.drain(..=end);
                    checked = 0;
                }
                checked = pending.len();
                if pending.len() > RESULT_LIMIT {
                    return Err(super::message("search.contents.limit", ""));
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.to_string()),
        }
        match stderr.read(&mut chunk) {
            Ok(0) => err_eof = true,
            Ok(n) => {
                progress = true;
                if errors.len() < ERROR_LIMIT {
                    errors.extend_from_slice(&chunk[..n.min(ERROR_LIMIT - errors.len())]);
                }
                for byte in &chunk[..n] {
                    if *byte == b'\n' {
                        foreign |= !skipped_path(&line) && !ignore_warning(&line);
                        line.clear();
                    } else if line.len() < ERROR_LIMIT {
                        line.push(*byte);
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.to_string()),
        }
        if eof && err_eof {
            if let Some(status) = child.child.try_wait().map_err(|error| error.to_string())? {
                *reaped = true;
                foreign |= !line.is_empty() && !skipped_path(&line) && !ignore_warning(&line);
                // Exit 2 after the summary, with only skipped-path or ignore-glob lines, is a finished walk that skipped what it could not read, as the names walk does.
                let skipped = match status.code() {
                    Some(0 | 1) => Some(false),
                    Some(2) if !errors.is_empty() && !foreign => Some(true),
                    _ => None,
                };
                if let (Some(skipped), Some(searches), true) =
                    (skipped, summary, pending.is_empty())
                {
                    return Ok(Some((hits.into_values().collect(), searches, skipped)));
                }
                return Err(format!(
                    "ripgrep {status}: {}",
                    String::from_utf8_lossy(&errors).trim()
                ));
            }
        }
        if !progress {
            std::thread::sleep(POLL);
        }
    }
}
