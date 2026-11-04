use super::graphics::Protocol;
use crate::{backend::regfile, oflags::O_NOFOLLOW};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

// One mpv for one preview: the child, the private directory that holds its IPC socket, and the file it reads.
pub struct Process {
    pub child: Child,
    pub directory: PathBuf,
    _input: File,
    // True while the TUI's poster still stands in for mpv's first frame.
    pub poster: Arc<AtomicBool>,
    forwarder: Option<std::thread::JoinHandle<()>>,
}
impl Process {
    pub fn spawn(
        path: &Path,
        protocol: Protocol,
        geometry: (usize, usize, usize, usize),
        terminal_size: (usize, usize),
        pixels: (usize, usize),
        shm: bool,
        poster: bool,
    ) -> io::Result<Self> {
        let before = fs::symlink_metadata(path)?;
        let input = regfile::open_if_regular(path, O_NOFOLLOW)?;
        let after = input.metadata()?;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(super::preview::words::issue("tui.preview.changed", "Selected item changed"));
        }
        let directory = temporary()?;
        let endpoint = directory.join("ipc");
        let mut command = crate::backend::locale::command("mpv");
        command
            .args(arguments(protocol, shm, &endpoint, geometry, terminal_size, pixels))
            .arg("--")
            .arg(format!(
                "/proc/{}/fd/{}",
                std::process::id(),
                input.as_raw_fd()
            ));
        let poster = Arc::new(AtomicBool::new(poster));
        // One start path in every mode, so none can choose another stdio or skip the relay.
        match spawn_relayed(command, Arc::clone(&poster), |bytes| {
            let mut stdout = io::stdout().lock();
            stdout.write_all(bytes).and_then(|()| stdout.flush()).is_ok()
        }) {
            Ok((child, forwarder)) => {
                Ok(Self { child, directory, _input: input, poster, forwarder })
            }
            Err(error) => {
                cleanup(&directory);
                Err(error)
            }
        }
    }
}
// The one start path in every mode: the child pipes its stdout into the escape filter.
fn spawn_relayed(
    mut command: Command,
    poster: Arc<AtomicBool>,
    sink: impl FnMut(&[u8]) -> bool + Send + 'static,
) -> io::Result<(Child, Option<std::thread::JoinHandle<()>>)> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn()?;
    let forwarder = child.stdout.take().map(|output| {
        let handoff = Arc::clone(&poster);
        std::thread::spawn(move || relay(output, &handoff, Path::new(SHM_ROOT), sink))
    });
    Ok((child, forwarder))
}
#[cfg(test)]
impl Process {
    // A reaped, harmless child lets the player's normal poll path observe a completed process.
    pub fn finished() -> Self {
        let mut child = Command::new("/usr/bin/true").env_clear().stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("true starts");
        child.wait().expect("true is reaped");
        let input = File::open("/dev/null").expect("/dev/null opens");
        Self { child, directory: PathBuf::from("/nonexistent"), _input: input, poster: Arc::new(AtomicBool::new(false)), forwarder: None }
    }
    // A stand-in child for a player whose mpv has started but not yet connected.
    pub fn idle() -> Self {
        let child = Command::new("sleep").arg("30").stdout(Stdio::null()).spawn().expect("sleep starts");
        let input = File::open("/dev/null").expect("/dev/null opens");
        Self { child, directory: PathBuf::from("/nonexistent"), _input: input, poster: Arc::new(AtomicBool::new(false)), forwarder: None }
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // mpv's last bytes reach the terminal before the TUI draws past this player.
        if let Some(forwarder) = self.forwarder.take() {
            let _ = forwarder.join();
        }
        cleanup(&self.directory);
    }
}
const READ_CHUNK: usize = 65536;
const SHM_ROOT: &str = "/dev/shm";
// Reads mpv's output to its end, hands each run of whole escapes to sink until it refuses, then removes the last frame.
pub fn relay(mut output: impl Read, poster: &AtomicBool, shm_root: &Path, mut sink: impl FnMut(&[u8]) -> bool) {
    let mut buffer = [0u8; READ_CHUNK];
    let mut pending = Vec::new();
    let mut handoff = Handoff { poster: poster.load(Ordering::Relaxed), drawing: false, shm: None };
    // A signal the TUI handles can interrupt a read; only EOF or a real error ends the relay.
    loop {
        let read = match output.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&buffer[..read]);
        let (out, used) = pass(&pending, &mut handoff);
        pending.drain(..used);
        if !out.is_empty() && !sink(&out) {
            break;
        }
        poster.store(handoff.poster, Ordering::Relaxed);
    }
    // kitty unlinks each frame it reads, so the one mpv wrote last before it stopped would stay in shared memory.
    if let Some(leftover) = handoff.shm.as_deref().and_then(|name| shm_leftover(shm_root, name)) {
        let _ = fs::remove_file(leftover);
    }
}
// Only a plain name under the shared memory root, the shape mpv gives its frames.
pub fn shm_leftover(root: &Path, name: &str) -> Option<PathBuf> {
    let plain = name.strip_prefix('/')?;
    let plain_ok = !plain.is_empty() && plain.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    plain_ok.then(|| root.join(plain))
}
// kitty 0.49 opens a shared memory frame only by a name that starts with a slash, and mpv 0.41 sends its names without one.
// Sample input: b"\x1b_Ga=T,t=s,f=24,s=640,v=360,C=1,q=2,m=1;bXB2LWtpdHR5LTE=\x1b\\", the payload being base64 of "mpv-kitty-1".
fn shm_absolute(unit: &[u8], last: &mut Option<String>) -> Option<Vec<u8>> {
    let body = unit.strip_suffix(b"\x1b\\")?;
    let split = body.iter().position(|&b| b == b';')?;
    let mut name = super::graphics::unbase64(&body[split + 1..])?;
    if name.first() != Some(&b'/') {
        name.insert(0, b'/');
    }
    let name = String::from_utf8(name).ok()?;
    let mut absolute = body[..=split].to_vec();
    absolute.extend_from_slice(super::graphics::base64(name.as_bytes()).as_bytes());
    absolute.extend_from_slice(b"\x1b\\");
    *last = Some(name);
    Some(absolute)
}
// Terminal modes stay the TUI's; while the poster is up mpv's delete-all waits, and the poster goes after its first frame.
pub struct Handoff {
    pub poster: bool,
    pub drawing: bool,
    // The shared memory name of mpv's frames, once one has passed.
    pub shm: Option<String>,
}
const POSTER_DELETE: &[u8] = b"\x1b_Ga=d,d=I,i=42,q=2\x1b\\";
// Sample input: b"\x1b[?25l\x1b_Ga=d;\x1b\\\x1b[3;81f\x1b_Ga=T,f=24,s=800,v=450,q=2,m=0;AAAA\x1b\\", returning the bytes to write and the bytes used.
pub fn pass(bytes: &[u8], handoff: &mut Handoff) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        let length = if rest[0] != 0x1b {
            rest.iter().position(|&b| b == 0x1b).unwrap_or(rest.len())
        } else if rest.len() < 2 {
            break;
        } else if matches!(rest[1], b'_' | b'P' | b'^' | b'X' | b']') {
            // APC, DCS (sixel), PM, SOS and OSC run to the string terminator; an OSC may end at BEL instead.
            let terminator = rest[2..].windows(2).position(|w| w == b"\x1b\\").map(|end| end + 4);
            let bell = (rest[1] == b']').then(|| rest[2..].iter().position(|&b| b == 0x07).map(|end| end + 3)).flatten();
            match terminator.into_iter().chain(bell).min() {
                Some(length) => length,
                None => break,
            }
        } else if rest[1] == b'[' {
            match rest[2..].iter().position(|b| (0x40..=0x7e).contains(b)) {
                Some(end) => end + 3,
                None => break,
            }
        } else {
            2
        };
        let unit = &rest[..length];
        at += length;
        if unit.starts_with(b"\x1b[?") && matches!(unit.last(), Some(b'h' | b'l')) {
            continue;
        }
        if let Some(control) = unit.strip_prefix(b"\x1b_G") {
            let keys = control.split(|&b| b == b';' || b == 0x1b).next().unwrap_or_default();
            let key = |name: &[u8]| keys.split(|&b| b == b',').find_map(|kv| kv.strip_prefix(name)?.strip_prefix(b"="));
            let action = key(b"a");
            if handoff.poster && action == Some(b"d") && matches!(key(b"d"), None | Some(b"a" | b"A")) {
                continue;
            }
            let absolute = if key(b"t") == Some(b"s") { shm_absolute(unit, &mut handoff.shm) } else { None };
            out.extend_from_slice(absolute.as_deref().unwrap_or(unit));
            if matches!(action, Some(b"T")) {
                handoff.drawing = true;
            }
            // A frame sent through shared memory or a file is whole in one escape; mpv 0.41 still marks it m=1.
            if handoff.drawing && (key(b"m") != Some(b"1") || key(b"t").is_some_and(|medium| medium != b"d")) {
                handoff.drawing = false;
                if handoff.poster {
                    out.extend_from_slice(POSTER_DELETE);
                    handoff.poster = false;
                }
            }
            continue;
        }
        out.extend_from_slice(unit);
    }
    (out, at)
}
// mpv's options before the input: paused on the first frame, and drawing into the preview pane without clearing the screen around it.
fn arguments(
    protocol: Protocol,
    shm: bool,
    endpoint: &Path,
    geometry: (usize, usize, usize, usize),
    terminal_size: (usize, usize),
    pixels: (usize, usize),
) -> Vec<String> {
    let driver = match protocol {
        Protocol::Kitty => "kitty",
        Protocol::Sixel => "sixel",
        Protocol::None => "null",
    };
    let mut args: Vec<String> = [
        "--no-config",
        "--load-scripts=no",
        "--ytdl=no",
        "--pause=yes",
        "--keep-open=yes",
        "--terminal=no",
        "--input-default-bindings=no",
        "--input-vo-keyboard=no",
        "--force-window=no",
        "--osc=no",
        "--osd-level=0",
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect();
    args.push(format!("--vo={}", driver));
    args.push(format!("--input-ipc-server={}", endpoint.display()));
    if driver != "null" {
        for (option, value) in [
            ("cols", terminal_size.0),
            ("rows", terminal_size.1),
            ("width", pixels.0),
            ("height", pixels.1),
            ("left", geometry.2),
            ("top", geometry.3),
        ] {
            args.push(format!("--vo-{}-{}={}", driver, option, value));
        }
        args.push(format!("--vo-{}-alt-screen=no", driver));
        // The driver clears the whole terminal at each reconfigure by default, a blank frame under the listing.
        args.push(format!("--vo-{}-config-clear=no", driver));
        // Frames through shared memory instead of megabytes of base64 escapes, when kitty said it can read them.
        if shm && driver == "kitty" {
            args.push("--vo-kitty-use-shm=yes".into());
        }
    }
    args
}
fn temporary() -> io::Result<PathBuf> {
    let output = crate::backend::locale::command("mktemp")
        .args(["-d", "/tmp/flea-tui-XXXXXX"])
        .output()?;
    if !output.status.success() {
        return Err(super::preview::words::issue("tui.preview.socket_create", "Could not create preview socket directory"));
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if !path.is_absolute()
        || path.parent() != Some(Path::new("/tmp"))
        || !path
            .file_name()
            .is_some_and(|p| p.to_string_lossy().starts_with("flea-tui-"))
    {
        return Err(super::preview::words::issue("tui.preview.socket_sandbox", "Preview socket directory is outside its sandbox"));
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join(".flea-tui-owned"))?
        .write_all(b"flea")?;
    Ok(path)
}
fn cleanup(root: &Path) {
    if !root.is_absolute()
        || root.parent() != Some(Path::new("/tmp"))
        || !root
            .file_name()
            .is_some_and(|p| p.to_string_lossy().starts_with("flea-tui-"))
        || !fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_dir())
        || fs::read(root.join(".flea-tui-owned")).ok().as_deref() != Some(b"flea")
    {
        return;
    }
    for name in ["ipc", ".flea-tui-owned"] {
        let path = root.join(name);
        if path.is_absolute() && path.starts_with(root) && path != root {
            let _ = fs::remove_file(path);
        }
    }
    let _ = fs::remove_dir(root);
}
#[cfg(test)]
#[path = "mpv_tests.rs"]
mod tests;
