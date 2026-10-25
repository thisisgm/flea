use super::{
    parse::{self, Hit, Record},
    Ask, Outcome, Snapshot,
};
use crate::backend::{jail, sandbox};
use flea::ui_logic_sr1::ContentsMatch;
use std::collections::{HashMap, HashSet};
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
        tool.to_string_lossy().into_owned(),
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
    let root = Path::new(&ask.path);
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
    let Some((mut hits, scanned)) = result? else {
        return Ok(None);
    };
    hits.sort_by(|a, b| a.path.cmp(&b.path));
    let unsafe_names: HashSet<String> = hits
        .iter()
        .filter(|hit| std::str::from_utf8(&hit.path).is_err())
        .map(|hit| String::from_utf8_lossy(&hit.path).into_owned())
        .collect();
    let matches = hits
        .into_iter()
        .enumerate()
        .map(|(row, hit)| {
            let path = String::from_utf8_lossy(&hit.path).into_owned();
            let operable = !unsafe_names.contains(&path);
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
    }))
}

fn collect(
    child: &mut jail::Jailed,
    stop: &AtomicBool,
    started: Instant,
    reaped: &mut bool,
) -> Result<Option<(Vec<Hit>, usize)>, String> {
    let mut stdout = child.child.stdout.take().ok_or("missing ripgrep stdout")?;
    let mut stderr = child.child.stderr.take().ok_or("missing ripgrep stderr")?;
    nonblocking(stdout.as_raw_fd()).map_err(|error| error.to_string())?;
    nonblocking(stderr.as_raw_fd()).map_err(|error| error.to_string())?;
    let mut pending = Vec::new();
    let mut checked = 0;
    let mut errors = Vec::new();
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
                if matches!(status.code(), Some(0 | 1))
                    && summary.is_some()
                    && pending.is_empty()
                    && errors.is_empty()
                {
                    return Ok(Some((hits.into_values().collect(), summary.unwrap_or(0))));
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
