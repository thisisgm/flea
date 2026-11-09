//! The production backend: the GUI binary re-run as its own decoder helper inside bubblewrap.
use super::engine::{feed, Backend, FeedSender, Playback, Request};
use super::helper::{Options, FLAG, INPUT};
use flea::ui_logic_media_preview::frame::{parse_header, Command, Event, Failure, Record, HEADER};
use gpui::RenderImage;
use std::ffi::c_int;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};

const O_NONBLOCK: c_int = 0o4000;
const F_SETFD: c_int = 2;
const FD_CLOEXEC: c_int = 1;
// close_range's flag that marks rather than closes, so std's exec error pipe lives until the exec.
const CLOSE_RANGE_CLOEXEC: c_int = 4;
// The fallback loop's bound, for kernels older than close_range.
const FALLBACK_FD_MAX: c_int = 1024;
// Where the sandbox mounts the helper binary.
const HELPER: &str = "/flea-media";

extern "C" {
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn close_range(first: u32, last: u32, flags: c_int) -> c_int;
}

/// Starts helpers from one executable, the running GUI binary unless a test names another.
pub struct Sandbox {
    pub executable: PathBuf,
    pub bwrap: PathBuf,
}

impl Sandbox {
    pub fn current() -> io::Result<Self> {
        Ok(Self {
            executable: std::env::current_exe()?,
            bwrap: "bwrap".into(),
        })
    }
}

/// The bubblewrap words for one helper: no network, no home, one read-only input, and the sound server's socket alone.
pub fn arguments(
    request: &Request,
    executable: &Path,
    input_fd: c_int,
    pulse: Option<&Path>,
) -> Vec<String> {
    let mut words: Vec<String> = [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/etc",
        "/etc",
        // fontconfig finds its cache instead of logging an error per font dir; -try skips a box without one.
        "--ro-bind-try",
        "/var/cache/fontconfig",
        "/var/cache/fontconfig",
        "--symlink",
        "usr/lib",
        "/lib",
        "--ro-bind-try",
        "/lib64",
        "/lib64",
        "--symlink",
        "usr/bin",
        "/bin",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/run",
        "--setenv",
        "LC_ALL",
        "C",
    ]
    .map(String::from)
    .into();
    words.extend([
        "--ro-bind".into(),
        executable.display().to_string(),
        HELPER.into(),
    ]);
    words.extend(["--ro-bind-fd".into(), input_fd.to_string(), INPUT.into()]);
    if let Some(socket) = pulse {
        words.extend([
            "--ro-bind".into(),
            socket.display().to_string(),
            "/pulse-native".into(),
        ]);
        words.extend([
            "--setenv".into(),
            "PULSE_SERVER".into(),
            "unix:/pulse-native".into(),
        ]);
    }
    let options = Options {
        box_w: request.box_w,
        box_h: request.box_h,
        audio: pulse.is_some(),
        autoplay: request.autoplay,
    };
    words.extend(["--".into(), HELPER.into(), FLAG.into()]);
    words.extend(options.words());
    words
}

/// The sound server socket the helper may play into, when the session has one.
fn pulse_socket() -> Option<PathBuf> {
    let socket = Path::new(&std::env::var_os("XDG_RUNTIME_DIR")?).join("pulse/native");
    socket.exists().then_some(socket)
}

impl Backend for Sandbox {
    /// Returns at once: the open, the spawn and every write happen on the helper's own thread, never the UI's.
    fn start(&self, request: &Request) -> Result<(Box<dyn Playback>, super::engine::Feed), String> {
        let (sender, feed) = feed();
        let (commands, received) = mpsc::channel::<Command>();
        let pid = Arc::new(AtomicU32::new(0));
        let (request, executable, bwrap, shown) = (
            request.clone(),
            self.executable.clone(),
            self.bwrap.clone(),
            pid.clone(),
        );
        std::thread::Builder::new()
            .name("flea-media".into())
            .spawn(move || match spawn(&request, &executable, &bwrap) {
                Ok(mut child) => drive(&mut child, &shown, &received, sender),
                Err(failure) => {
                    eprintln!("flea-media: {}", failure.1);
                    sender.event(Event::Failed(failure.0));
                    sender.closed();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok((Box::new(Helper { commands, pid }), feed))
    }
}

fn spawn(request: &Request, executable: &Path, bwrap: &Path) -> Result<Child, (Failure, String)> {
    let unreadable = |error: io::Error| (Failure::Unreadable, error.to_string());
    let input = OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(&request.path)
        .map_err(unreadable)?;
    if !input.metadata().map_err(unreadable)?.is_file() {
        return Err((Failure::Unreadable, "not a regular file".into()));
    }
    let fd = input.as_raw_fd();
    let pulse = if request.audio { pulse_socket() } else { None };
    let mut command = std::process::Command::new(bwrap);
    command.args(arguments(request, executable, fd, pulse.as_deref()));
    // SAFETY: async-signal-safe calls only; bubblewrap passes on all it inherits, so 3 and up are marked close-on-exec, then the input is cleared.
    unsafe {
        command.pre_exec(move || {
            if close_range(3, u32::MAX, CLOSE_RANGE_CLOEXEC) != 0 {
                for each in 3..FALLBACK_FD_MAX {
                    fcntl(each, F_SETFD, FD_CLOEXEC);
                }
            }
            if fcntl(fd, F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let helper = |error: io::Error| (Failure::Helper, error.to_string());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(helper)
}

/// Feeds commands to a running helper until the player lets go, then ends it.
fn drive(
    child: &mut Child,
    pid: &AtomicU32,
    commands: &mpsc::Receiver<Command>,
    sender: FeedSender,
) {
    pid.store(child.id(), Ordering::Release);
    let (Some(stdout), Some(mut stdin)) = (child.stdout.take(), child.stdin.take()) else {
        sender.event(Event::Failed(Failure::Helper));
        sender.closed();
        return;
    };
    let reader = std::thread::Builder::new()
        .name("flea-media-read".into())
        .spawn(move || read(stdout, &sender));
    for command in commands {
        if writeln!(stdin, "{}", command.render()).is_err() {
            break;
        }
    }
    // The helper's pid namespace dies with bubblewrap, taking every decoder thread and child.
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(reader) = reader {
        let _ = reader.join();
    }
}

struct Helper {
    commands: mpsc::Sender<Command>,
    pid: Arc<AtomicU32>,
}

impl Playback for Helper {
    fn send(&mut self, command: Command) {
        let _ = self.commands.send(command);
    }

    fn pids(&self) -> Vec<u32> {
        match self.pid.load(Ordering::Acquire) {
            0 => Vec::new(),
            pid => vec![pid],
        }
    }
}

/// Reads records until the helper ends, refusing anything malformed before allocating for it.
pub fn read(mut input: impl Read, sender: &FeedSender) {
    let mut header = [0_u8; HEADER];
    loop {
        if input.read_exact(&mut header).is_err() {
            break;
        }
        let record = match parse_header(&header) {
            Ok(record) => record,
            Err(_) => {
                sender.event(Event::Failed(Failure::Helper));
                break;
            }
        };
        let mut body = vec![0_u8; record.length()];
        if input.read_exact(&mut body).is_err() {
            break;
        }
        let delivered = match record {
            Record::Frame { width, height } => match picture(width, height, body) {
                Some(image) => sender.frame(image),
                None => sender.event(Event::Failed(Failure::Helper)),
            },
            Record::Event { .. } => match std::str::from_utf8(&body).ok().and_then(Event::parse) {
                Some(event) => sender.event(event),
                None => {
                    sender.event(Event::Failed(Failure::Helper));
                    false
                }
            },
        };
        if !delivered {
            break;
        }
    }
    sender.closed();
}

fn picture(width: u32, height: u32, pixels: Vec<u8>) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}
