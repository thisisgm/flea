//! One measured child per run: an entrant walks the tree, then the harness index takes every entry.
use std::io::{BufRead, BufReader};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const ADAPTERS: [&str; 8] = [
    "flea-search",
    "std-serial",
    "ignore-parallel",
    "ignore-serial",
    "jwalk",
    "dua-core",
    "fd",
    "ripgrep",
];

/// Walk drains and counts; index also copies each relative path into one shared arena layout.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Walk,
    Index,
}

/// The harness-owned index every entrant fills: one byte arena, end offsets and a kind byte per entry.
#[derive(Default)]
pub struct Sink {
    index: bool,
    pub files: u64,
    pub dirs: u64,
    bytes: Vec<u8>,
    ends: Vec<usize>,
    kinds: Vec<u8>,
}

impl Sink {
    fn new(mode: Mode) -> Self {
        Sink {
            index: mode == Mode::Index,
            ..Sink::default()
        }
    }
    fn push(&mut self, rel: &[u8], dir: bool) {
        if dir {
            self.dirs += 1
        } else {
            self.files += 1
        }
        if self.index {
            self.bytes.extend_from_slice(rel);
            self.ends.push(self.bytes.len());
            self.kinds.push(if dir { b'd' } else { b'f' });
        }
    }
    fn merge(&mut self, other: Sink) {
        let base = self.bytes.len();
        self.files += other.files;
        self.dirs += other.dirs;
        self.bytes.extend_from_slice(&other.bytes);
        self.ends.extend(other.ends.iter().map(|end| end + base));
        self.kinds.extend_from_slice(&other.kinds);
    }
    /// Items for the coverage digest, built only after the measured clock stops.
    pub fn items(&self) -> Vec<(Vec<u8>, u8)> {
        let mut start = 0;
        self.ends
            .iter()
            .zip(&self.kinds)
            .map(|(end, kind)| {
                let item = (self.bytes[start..*end].to_vec(), *kind);
                start = *end;
                item
            })
            .collect()
    }
}

pub struct Outcome {
    pub sink: Sink,
    pub elapsed_ns: u128,
    pub errors: u64,
    /// Own plus reaped-tool CPU at the stage clock stop, before any coverage work.
    pub cpu_us: i64,
    /// Own VmHWM at the stage clock stop; exec starts a fresh mm, so no parent pages are counted.
    pub hwm_kib: i64,
    /// External tool: own VmHWM just before spawn, and the tool's wait4 peak (a floor of the former).
    pub spawn_hwm_kib: i64,
    pub tool_maxrss_kib: i64,
}

/// VmHWM of this process in KiB.
pub fn hwm_kib() -> i64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(-1)
}

fn cpu_us() -> i64 {
    let micros = |t: libc::timeval| t.tv_sec * 1_000_000 + t.tv_usec;
    let mut total = 0;
    for who in [libc::RUSAGE_SELF, libc::RUSAGE_CHILDREN] {
        // SAFETY: rusage is plain data and getrusage only writes it.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        // SAFETY: valid pointer to a zeroed rusage.
        if unsafe { libc::getrusage(who, &mut usage) } == 0 {
            total += micros(usage.ru_utime) + micros(usage.ru_stime);
        }
    }
    total
}

fn relative<'a>(tree: &Path, path: &'a Path) -> &'a [u8] {
    path.strip_prefix(tree)
        .map(|p| p.as_os_str().as_bytes())
        .unwrap_or(path.as_os_str().as_bytes())
}

pub fn run(
    name: &str,
    tree: &Path,
    threads: usize,
    mode: Mode,
    tool: Option<&Path>,
) -> Result<Outcome, String> {
    if name == "flea-search" {
        return flea_search(tree, mode);
    }
    let started = Instant::now();
    let mut tool_peak = (0, 0);
    let (sink, errors) = match name {
        "std-serial" => std_serial(tree, mode),
        "ignore-parallel" => ignore_parallel(tree, threads, mode),
        "ignore-serial" => ignore_serial(tree, mode),
        "jwalk" => jwalk_walk(tree, threads, mode),
        "dua-core" => dua_walk(tree, threads, mode),
        "fd" => external(
            tool.ok_or("fd needs --tool")?,
            &fd_args(threads, tree),
            tree,
            mode,
            true,
            &mut tool_peak,
        )?,
        "ripgrep" => external(
            tool.ok_or("ripgrep needs --tool")?,
            &rg_args(threads, tree),
            tree,
            mode,
            false,
            &mut tool_peak,
        )?,
        other => return Err(format!("unknown adapter {other}")),
    };
    let elapsed_ns = started.elapsed().as_nanos();
    Ok(Outcome {
        sink,
        elapsed_ns,
        errors,
        cpu_us: cpu_us(),
        hwm_kib: hwm_kib(),
        spawn_hwm_kib: tool_peak.0,
        tool_maxrss_kib: tool_peak.1,
    })
}

fn std_serial(tree: &Path, mode: Mode) -> (Sink, u64) {
    let (mut sink, mut errors, mut pending) = (Sink::new(mode), 0, vec![tree.to_path_buf()]);
    while let Some(dir) = pending.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            errors += 1;
            continue;
        };
        for entry in read {
            let Ok(entry) = entry else {
                errors += 1;
                continue;
            };
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let path = entry.path();
            sink.push(relative(tree, &path), is_dir);
            if is_dir {
                pending.push(path);
            }
        }
    }
    (sink, errors)
}

fn ignore_builder(tree: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(tree);
    builder
        .standard_filters(false)
        .hidden(false)
        .follow_links(false);
    builder
}

fn ignore_serial(tree: &Path, mode: Mode) -> (Sink, u64) {
    let (mut sink, mut errors) = (Sink::new(mode), 0);
    for entry in ignore_builder(tree).build() {
        match entry {
            Ok(entry) if entry.depth() > 0 => {
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                sink.push(relative(tree, entry.path()), is_dir);
            }
            Ok(_) => {}
            Err(_) => errors += 1,
        }
    }
    (sink, errors)
}

struct Visitor<'s> {
    sink: Option<Sink>,
    tree: &'s Path,
    out: &'s Mutex<Vec<Sink>>,
    errors: &'s AtomicU64,
}

impl ignore::ParallelVisitor for Visitor<'_> {
    fn visit(&mut self, entry: Result<ignore::DirEntry, ignore::Error>) -> ignore::WalkState {
        match entry {
            Ok(entry) if entry.depth() > 0 => {
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                if let Some(sink) = self.sink.as_mut() {
                    sink.push(relative(self.tree, entry.path()), is_dir);
                }
            }
            Ok(_) => {}
            Err(_) => {
                self.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        ignore::WalkState::Continue
    }
}

impl Drop for Visitor<'_> {
    fn drop(&mut self) {
        if let (Some(sink), Ok(mut out)) = (self.sink.take(), self.out.lock()) {
            out.push(sink);
        }
    }
}

struct Builder<'s> {
    mode: Mode,
    tree: &'s Path,
    out: &'s Mutex<Vec<Sink>>,
    errors: &'s AtomicU64,
}

impl<'s> ignore::ParallelVisitorBuilder<'s> for Builder<'s> {
    fn build(&mut self) -> Box<dyn ignore::ParallelVisitor + 's> {
        Box::new(Visitor {
            sink: Some(Sink::new(self.mode)),
            tree: self.tree,
            out: self.out,
            errors: self.errors,
        })
    }
}

fn ignore_parallel(tree: &Path, threads: usize, mode: Mode) -> (Sink, u64) {
    let (out, errors) = (Mutex::new(Vec::new()), AtomicU64::new(0));
    let mut builder = Builder {
        mode,
        tree,
        out: &out,
        errors: &errors,
    };
    ignore_builder(tree)
        .threads(threads)
        .build_parallel()
        .visit(&mut builder);
    let mut sink = Sink::new(mode);
    for part in out.into_inner().unwrap_or_default() {
        sink.merge(part);
    }
    (sink, errors.into_inner())
}

fn jwalk_walk(tree: &Path, threads: usize, mode: Mode) -> (Sink, u64) {
    let (mut sink, mut errors) = (Sink::new(mode), 0);
    let walk = jwalk::WalkDir::new(tree)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(threads));
    for entry in walk {
        match entry {
            Ok(entry) if entry.depth > 0 => {
                let path = entry.parent_path.join(&entry.file_name);
                sink.push(relative(tree, &path), entry.file_type.is_dir());
            }
            Ok(_) => {}
            Err(_) => errors += 1,
        }
    }
    (sink, errors)
}

fn dua_walk(tree: &Path, threads: usize, mode: Mode) -> (Sink, u64) {
    let (mut sink, mut errors) = (Sink::new(mode), 0);
    let options = dua_core::Options::default().skip_metadata();
    for entry in dua_core::walk(tree, threads, dua_core::Order::Completion, options, |_| {
        true
    }) {
        match entry {
            Ok(entry) if entry.depth > 0 => {
                let path = entry.parent_path.join(&entry.file_name);
                sink.push(relative(tree, &path), entry.file_type.is_dir());
            }
            Ok(_) => {}
            Err(_) => errors += 1,
        }
    }
    (sink, errors)
}

/// fd: unrestricted (hidden and ignore files off), no symlink descent, NUL records, directories end in a slash.
pub fn fd_args(threads: usize, tree: &Path) -> Vec<String> {
    let tree = tree.display().to_string();
    vec![
        "--unrestricted".into(),
        "--print0".into(),
        "--color=never".into(),
        format!("--threads={threads}"),
        "--base-directory".into(),
        tree,
        ".".into(),
    ]
}

/// ripgrep: file listing only, every filter off, no symlink descent, NUL records.
pub fn rg_args(threads: usize, tree: &Path) -> Vec<String> {
    let tree = tree.display().to_string();
    vec![
        "--files".into(),
        "-uuu".into(),
        "--null".into(),
        format!("--threads={threads}"),
        tree,
    ]
}

fn external(
    tool: &Path,
    args: &[String],
    tree: &Path,
    mode: Mode,
    cwd_relative: bool,
    peak: &mut (i64, i64),
) -> Result<(Sink, u64), String> {
    peak.0 = hwm_kib();
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", tool.display()))?;
    let mut reader = BufReader::with_capacity(1 << 20, child.stdout.take().ok_or("no stdout")?);
    let (mut sink, mut record) = (Sink::new(mode), Vec::with_capacity(512));
    let prefix = [tree.as_os_str().as_bytes(), b"/"].concat();
    while reader
        .read_until(0, &mut record)
        .map_err(|e| e.to_string())?
        > 0
    {
        record.pop_if(|b| *b == 0);
        let dir = record.last() == Some(&b'/');
        let mut rel = &record[..if dir { record.len() - 1 } else { record.len() }];
        rel = rel.strip_prefix(b"./".as_slice()).unwrap_or(rel);
        if !cwd_relative {
            rel = rel.strip_prefix(prefix.as_slice()).unwrap_or(rel);
        }
        sink.push(rel, dir);
        record.clear();
    }
    drop(reader);
    let (status, usage) = crate::measure::rusage_wait(child.id() as libc::pid_t)?;
    peak.1 = usage.ru_maxrss;
    Ok((sink, u64::from(status != 0)))
}

// The product keeps its compressed index during the timed walk; coverage decoding follows its clock.
fn flea_search(tree: &Path, mode: Mode) -> Result<Outcome, String> {
    if mode != Mode::Index {
        return Err("flea-search always builds its index; use index mode".into());
    }
    let binary = std::env::var("FLEA_SEARCH_BIN").map_err(|_| "FLEA_SEARCH_BIN required")?;
    let output = Command::new(binary)
        .args(["--search-stage", "walk"])
        .arg(tree)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let number = |key: &str| value[key].as_u64().ok_or(format!("missing {key}"));
    let mut sink = Sink::new(mode);
    for item in value["identities"].as_array().ok_or("missing identities")? {
        let raw = item[0].as_str().ok_or("bad raw identity")?;
        if raw.len() % 2 != 0 {
            return Err("odd hex identity".into());
        }
        let path: Result<Vec<_>, _> = (0..raw.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&raw[i..i + 2], 16))
            .collect();
        sink.push(&path.map_err(|e| e.to_string())?, item[1] == "d");
    }
    Ok(Outcome {
        sink,
        elapsed_ns: number("elapsed_ns")? as u128,
        errors: number("errors")?,
        cpu_us: -1,
        hwm_kib: -1,
        spawn_hwm_kib: -1,
        tool_maxrss_kib: -1,
    })
}
