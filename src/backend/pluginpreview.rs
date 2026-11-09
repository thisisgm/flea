//! A previewer plugin runs once per request inside the thumbnail sandbox, never in the lasting runtime.
use super::jail::{self, Jailed};
use super::plugins::{self, Env, Installed};
use super::{mime, sandbox};
use flea::jsondoc::Json;
use flea::plugin_runtime::{protocol, Reply};
use flea::ui_logic_mn2::plugins::mime_match;
use flea::ui_logic_st2::manifest::{Invalid, Manifest};
use std::io::{BufRead, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

// The one selected file and the private output folder, at fixed paths inside the sandbox.
pub(super) const INPUT: &str = "/in/file";
pub(super) const OUTPUT: &str = "/out";
const PNG_NAME: &str = "preview.png";
// A preview is a small answer, so its CPU, address space and file size caps sit well under the decoder's.
const CPU_SECONDS: u32 = 10;
const ADDRESS_SPACE_BYTES: u64 = 512 << 20;
const FILE_BYTES: u64 = 32 << 20;
const OPEN_FILES: u32 = 64;
pub(super) const REQUEST_DEADLINE: Duration = Duration::from_secs(10);
// The PNG's header must name a picture no larger than this on either side.
const MAX_SIDE: u32 = 8192;
const MAX_TEXT_LINES: usize = 1000;
// Shown pictures kept for the GUI to load; older ones are removed as new ones land.
const KEPT: usize = 8;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const F_DUPFD_CLOEXEC: i32 = 1030;
const F_SETFD: i32 = 2;

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

static NEXT: AtomicU64 = AtomicU64::new(0);
static TYPES: OnceLock<mime::Db> = OnceLock::new();

#[cfg(test)]
pub(super) static TEST_ROOT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub(super) struct Limits {
    pub ready: Duration,
    pub request: Duration,
    pub root: PathBuf,
}

impl Limits {
    fn of(env: &Env) -> Self {
        Self {
            ready: env.ready_deadline,
            request: REQUEST_DEADLINE,
            root: root(),
        }
    }
}

fn root() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_ROOT.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        return root;
    }
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|p| p.join("flea-plugin-preview"))
        .unwrap_or_else(|| crate::userfile::cache_home().join("flea/plugin-preview"))
}

#[path = "pluginpreview_sandbox.rs"]
mod sandbox_args;
pub(super) use sandbox_args::argv;

struct Work(PathBuf);

impl Work {
    fn new(root: &Path) -> Result<Self, String> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder
            .create(root.join("shown"))
            .map_err(|e| e.to_string())?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("work-{}-{n}", std::process::id()));
        builder.recursive(false);
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        // Only this request's own folder under the preview root, never an empty or relative path.
        if self.0.is_absolute()
            && self
                .0
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("work-"))
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn read_line(output: &mut impl BufRead) -> Result<Json, String> {
    let mut line = Vec::new();
    Read::take(&mut *output, protocol::MAX_MESSAGE as u64 + 1)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if line.last() != Some(&b'\n') {
        return Err(if line.len() > protocol::MAX_MESSAGE {
            "plugin message exceeds 64 KiB".into()
        } else {
            "plugin closed stdout".into()
        });
    }
    line.pop();
    protocol::parse(&line)
}

// Sends every line, then reads up to `want` answers, each against its own deadline; the jail always ends.
fn exchange(
    jailed: &mut Jailed,
    lines: Vec<String>,
    limits: &Limits,
    want: usize,
) -> Result<Vec<Json>, String> {
    let input = Arc::new(Mutex::new(
        jailed.child.stdin.take().ok_or("missing plugin stdin")?,
    ));
    let output = jailed.child.stdout.take().ok_or("missing plugin stdout")?;
    let writer = input.clone();
    std::thread::spawn(move || {
        let mut input = writer.lock().unwrap_or_else(|p| p.into_inner());
        for line in lines {
            if input
                .write_all(line.as_bytes())
                .and_then(|_| input.write_all(b"\n"))
                .is_err()
            {
                break;
            }
        }
    });
    let (send, receive) = mpsc::sync_channel(want);
    std::thread::spawn(move || {
        let mut output = std::io::BufReader::new(output);
        for _ in 0..want {
            let line = read_line(&mut output);
            let stop = line.is_err();
            if send.send(line).is_err() || stop {
                break;
            }
        }
    });
    let mut answers = Vec::new();
    for at in 0..want {
        let deadline = Instant::now()
            + if at == 0 {
                limits.ready
            } else {
                limits.request
            };
        match receive.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => answers.push(line?),
            Err(_) => return Err("plugin timed out".into()),
        }
        if at == 0 {
            protocol::ready(&answers[0])?;
        }
    }
    drop(input);
    Ok(answers)
}

fn spawn(
    folder: &Path,
    entry: &Path,
    input: Option<&std::fs::File>,
    work: &Path,
    arguments: &[&str],
) -> Result<Jailed, String> {
    // The private copy stays above the status slot, and only this child clears its close-on-exec flag.
    let pinned = input
        .map(|file| {
            let fd = unsafe { fcntl(file.as_raw_fd(), F_DUPFD_CLOEXEC, sandbox::STATUS_FD + 1) };
            if fd < 0 {
                Err(std::io::Error::last_os_error().to_string())
            } else {
                Ok(unsafe { OwnedFd::from_raw_fd(fd) })
            }
        })
        .transpose()?;
    let input_fd = pinned.as_ref().map(AsRawFd::as_raw_fd);
    let mut full = argv(folder, entry, input_fd, work);
    full.extend(arguments.iter().map(|arg| arg.to_string()));
    jail::spawn_jailed(&full, |command| {
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        unsafe {
            command.pre_exec(move || {
                if input_fd.is_some_and(|fd| fcntl(fd, F_SETFD, 0) < 0) {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    })
    .map_err(|e| e.to_string())
}

fn run(
    env: &Env,
    found: &Installed,
    input: Option<&std::fs::File>,
    request: Option<Json>,
    limits: &Limits,
) -> Result<(Option<Json>, Work), String> {
    if !sandbox::available() {
        return Err("previewer requires bwrap and prlimit".into());
    }
    let manifest = &found.plugin.manifest;
    let folder = found
        .folder
        .as_ref()
        .ok_or("previewer must be installed from a folder")?;
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    let entry = super::pluginfs::entry(&folder, &manifest.run).map_err(|e| e.detail)?;
    let work = Work::new(&limits.root)?;
    let mut jailed = spawn(&folder, &entry, input, &work.0, &[])?;
    let mut lines = vec![plugins::hello(env, manifest)];
    lines.extend(request.as_ref().map(flea::ui_logic_st2::wire::compact));
    let answers = exchange(
        &mut jailed,
        lines,
        limits,
        1 + usize::from(request.is_some()),
    );
    jail::kill_tree(&mut jailed);
    Ok((answers?.into_iter().nth(1), work))
}

// The enable check and the GUI's ready ask: hello and ready inside the sandbox, with no input bound.
pub(super) fn check(env: &Env, found: &Installed) -> Result<(), Invalid> {
    run(env, found, None, None, &Limits::of(env))
        .map(|_| ())
        .map_err(|e| Invalid::new("start", e))
}

fn selected(manifest: &Manifest, request: &Json) -> Result<std::fs::File, Invalid> {
    let refuse = |why: &str| Invalid::new("request", why);
    let path = match request.get("paths").and_then(Json::as_array) {
        Some([Json::Str(path)]) => flea::bytename::view(path),
        _ => return Err(refuse("preview takes exactly one selected file")),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind = TYPES
        .get_or_init(mime::Db::load)
        .lookup(&name)
        .unwrap_or("application/octet-stream");
    let globs = manifest
        .block("previewer")
        .and_then(|b| b.get("mime"))
        .and_then(Json::as_array)
        .unwrap_or(&[]);
    if !globs
        .iter()
        .filter_map(Json::as_str)
        .any(|glob| mime_match(glob, kind))
    {
        return Err(refuse("previewer does not handle this type"));
    }
    super::regfile::open_regular(&path)
        .ok_or_else(|| refuse("preview input must be a regular file"))
}

// Plain text only: no control characters but newline and tab, and a bounded line count.
fn plain(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .take(MAX_TEXT_LINES)
        .map(|line| {
            line.chars()
                .filter(|c| *c == '\t' || !c.is_control())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn picture(work: &Path, root: &Path) -> Result<(PathBuf, u32, u32), String> {
    let path = work.join(PNG_NAME);
    let refuse = || "invalid plugin preview picture".to_string();
    let mut input =
        super::regfile::open_if_regular(&path, crate::oflags::O_NOFOLLOW).map_err(|_| refuse())?;
    if input.metadata().map_err(|_| refuse())?.len() > FILE_BYTES {
        return Err(refuse());
    }
    let mut head = [0u8; 24];
    input.read_exact(&mut head).map_err(|_| refuse())?;
    if !head.starts_with(PNG_SIGNATURE) {
        return Err(refuse());
    }
    let (w, h) = super::imagesize::from_header(&head).ok_or_else(refuse)?;
    if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
        return Err(refuse());
    }
    png::check(&input, root)?;
    let shown = retention::publish(&path, root)?;
    Ok((shown, w, h))
}

#[path = "pluginpreview_png.rs"]
mod png;

#[path = "pluginpreview_retention.rs"]
mod retention;

pub(super) fn preview(env: &Env, found: &Installed, request: &Json) -> Result<Reply, Invalid> {
    preview_with(env, found, request, &Limits::of(env))
}

pub(super) fn preview_with(
    env: &Env,
    found: &Installed,
    request: &Json,
    limits: &Limits,
) -> Result<Reply, Invalid> {
    let manifest = &found.plugin.manifest;
    let input = selected(manifest, request)?;
    let number = |key: &str| {
        request
            .get(key)
            .and_then(Json::as_f64)
            .filter(|n| (1.0..=8192.0).contains(n))
            .map_or(900.0, f64::round)
    };
    let message = Json::Obj(vec![
        ("op".into(), Json::Str("preview".into())),
        ("path".into(), Json::Str(INPUT.into())),
        ("outDir".into(), Json::Str(OUTPUT.into())),
        ("width".into(), Json::Num(number("width").to_string())),
        ("height".into(), Json::Num(number("height").to_string())),
    ]);
    let failed = |e: String| Invalid::new("start", e);
    let (answer, work) = run(env, found, Some(&input), Some(message), limits).map_err(failed)?;
    let answer = answer.ok_or_else(|| failed("plugin did not answer".into()))?;
    protocol::response(&answer, "preview").map_err(failed)?;
    if answer.get("op").and_then(Json::as_str) == Some("error") {
        return Ok(Reply {
            message: answer,
            progress: Vec::new(),
        });
    }
    let declared = manifest
        .block("previewer")
        .and_then(|b| b.get("output"))
        .and_then(Json::as_str);
    let op = ("op".to_string(), Json::Str("preview".into()));
    let message = match (declared, answer.get("text").and_then(Json::as_str)) {
        (Some("text"), Some(text)) => Json::Obj(vec![op, ("text".into(), Json::Str(plain(text)))]),
        (Some("png"), None) => {
            let (path, w, h) = picture(&work.0, &limits.root).map_err(failed)?;
            Json::Obj(vec![
                op,
                ("png".into(), Json::Str(path.to_string_lossy().into_owned())),
                ("width".into(), Json::Num(w.to_string())),
                ("height".into(), Json::Num(h.to_string())),
            ])
        }
        _ => {
            return Err(failed(
                "plugin preview does not match its declared output".into(),
            ))
        }
    };
    Ok(Reply {
        message,
        progress: Vec::new(),
    })
}

#[cfg(test)]
#[path = "pluginpreview_picture_tests.rs"]
mod picture_tests;

#[cfg(test)]
#[path = "pluginpreview_race_tests.rs"]
mod race_tests;
