//! The real backend process and its sandbox for m9_g1rows_02: marked roots, the fixture, the rustc builds, the inotify observer and /proc readers.
use super::probe::probe;
use std::ffi::{c_char, c_int, CString};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "../../src/g2/context_menus/makeexec_foldhash.rs"]
mod foldhash;

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int;
}

const IN_ACCESS: u32 = 0x1;
const IN_OPEN: u32 = 0x20;
const IN_Q_OVERFLOW: u32 = 0x4000;
const IN_NONBLOCK: c_int = 0o4000;
const IN_CLOEXEC: c_int = 0o2000000;
pub(crate) const DEADLINE: Duration = Duration::from_secs(120);
// Late work has this long to show up after the first rows; it only widens the observer, never passes a gate.
pub(crate) const SETTLE: Duration = Duration::from_millis(750);
// Every fixture file carries these bytes, so a content sniff or a decoder would have something to open.
pub(crate) const CONTENT: &[u8] = b"\x89PNG\r\n\x1a\nflea";

pub(crate) fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn scratch() -> PathBuf {
    let parent = workspace().join(".superpowers/m9-g1rows-02");
    fs::create_dir_all(&parent).unwrap();
    parent.canonicalize().unwrap()
}

// A new marked root under the unit's own scratch directory, created like mktemp -d; it is removed when dropped, pass or fail.
pub(crate) struct Root(PathBuf);

impl std::ops::Deref for Root {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let removed = remove_root(&self.0);
        assert!(
            removed || std::thread::panicking(),
            "the marked root {} was not removed",
            self.0.display()
        );
    }
}

pub(crate) fn marked_root(tag: &str) -> Root {
    let parent = scratch();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = parent.join(format!("{tag}.{}.{nanos}", std::process::id()));
    fs::create_dir(&root).unwrap();
    File::create(root.join(".flea-test-sandbox")).unwrap();
    File::create(root.join(".flea-owned")).unwrap();
    for dir in ["state", "config", "cache", "data", "home"] {
        fs::create_dir(root.join(dir)).unwrap();
    }
    Root(root)
}

// Removes only a marked absolute root inside the unit's scratch directory, and refuses anything else.
pub(crate) fn remove_root(root: &Path) -> bool {
    let parent = scratch();
    let marked = root.join(".flea-test-sandbox").is_file() && root.join(".flea-owned").is_file();
    if !(root.is_absolute() && root.starts_with(&parent) && root != parent && marked) {
        eprintln!("M9_G1ROWS_02 refused to remove {}", root.display());
        return false;
    }
    match fs::remove_dir_all(root) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("M9_G1ROWS_02 could not remove {}: {error}", root.display());
            false
        }
    }
}

// Mixed extensions so MIME, icon and thumbnail lookups all run on the drawn rows.
pub(crate) fn fixture(root: &Path, entries: usize) -> PathBuf {
    let dir = root.join(format!("fixture-{entries}"));
    fs::create_dir(&dir).unwrap();
    let kinds = ["txt", "png", "rs", "md", "pdf", "jpg", "mp4", ""];
    for i in 0..entries {
        let ext = kinds[i % kinds.len()];
        let name = if ext.is_empty() {
            format!("n{i:06}")
        } else {
            format!("n{i:06}.{ext}")
        };
        fs::write(dir.join(name), CONTENT).unwrap();
    }
    dir
}

// One reused build directory, so repeated runs overwrite the same probe source, probe library, core rlib and backend instead of piling up.
pub(crate) fn build_dir() -> PathBuf {
    let dir = scratch().join("build");
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn rustc(args: &[&std::ffi::OsStr]) {
    let executable = std::env::current_exe().unwrap();
    let dependencies = executable.parent().unwrap();
    let output = Command::new("rustc")
        .args(args)
        .args(foldhash::backend_extern_args(&workspace(), dependencies))
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()))
        .env("CARGO_MANIFEST_DIR", workspace())
        .env("CARGO_PKG_VERSION", env!("CARGO_PKG_VERSION"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rustc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// The real backend, built from this tree's src/lib.rs and src/main.rs with no features, so no stale cargo artefact can stand in for it.
pub(crate) fn binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let (lib, main) = (
            workspace().join("src/lib.rs"),
            workspace().join("src/main.rs"),
        );
        let (library, binary) = (
            build_dir().join("libflea.rlib"),
            build_dir().join("flea-backend"),
        );
        rustc(&[
            "--edition=2021".as_ref(),
            "--crate-type=rlib".as_ref(),
            "--crate-name=flea".as_ref(),
            lib.as_os_str(),
            "-o".as_ref(),
            library.as_os_str(),
        ]);
        let external = format!("flea={}", library.display());
        rustc(&[
            "--edition=2021".as_ref(),
            "--crate-name=flea".as_ref(),
            main.as_os_str(),
            "--extern".as_ref(),
            external.as_ref(),
            "-o".as_ref(),
            binary.as_os_str(),
        ]);
        binary
    })
}

pub(crate) fn sandboxed(command: &mut Command, root: &Path) {
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .current_dir(root);
}

// Counts every open or read of a named entry in the folder, the only way MIME sniffing or a thumbnail touches a file.
pub(crate) struct Observer {
    file: File,
}

impl Observer {
    pub(crate) fn watch(dir: &Path) -> Self {
        let fd = unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) };
        assert!(fd >= 0, "inotify_init1 failed");
        let path = CString::new(dir.as_os_str().as_bytes()).unwrap();
        assert!(
            unsafe { inotify_add_watch(fd, path.as_ptr(), IN_OPEN | IN_ACCESS) } >= 0,
            "inotify_add_watch failed"
        );
        Self {
            file: unsafe { File::from_raw_fd(fd) },
        }
    }

    // Sample input: struct inotify_event {i32 wd; u32 mask; u32 cookie; u32 len; name[len]}, native endian; the folder's own open has len 0.
    pub(crate) fn named(&mut self) -> usize {
        let mut named = 0;
        let mut buffer = [0u8; 65536];
        loop {
            match self.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let mut at = 0;
                    while at + 16 <= n {
                        let mask = u32::from_ne_bytes(buffer[at + 4..at + 8].try_into().unwrap());
                        assert_eq!(
                            mask & IN_Q_OVERFLOW,
                            0,
                            "the inotify queue overflowed, so the count is unknown"
                        );
                        let len = u32::from_ne_bytes(buffer[at + 12..at + 16].try_into().unwrap())
                            as usize;
                        if len > 0 {
                            named += 1;
                        }
                        at += 16 + len;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("inotify read failed: {error}"),
            }
        }
        named
    }
}

// Sample input: "4302 4305 " per task; a task that exits mid-read is skipped, but some task must expose the file (CONFIG_PROC_CHILDREN).
pub(crate) fn children(pid: u32) -> Vec<u32> {
    let mut readable = 0;
    let mut found = Vec::new();
    for task in fs::read_dir(format!("/proc/{pid}/task"))
        .unwrap()
        .filter_map(Result::ok)
    {
        match fs::read_to_string(task.path().join("children")) {
            Ok(line) => {
                readable += 1;
                found.extend(
                    line.split_whitespace()
                        .map(|child| child.parse::<u32>().unwrap()),
                );
            }
            Err(_) if !task.path().exists() => {}
            Err(error) => panic!("/proc/{pid}/task children file: {error}"),
        }
    }
    assert!(readable > 0, "no task of {pid} exposes /proc children");
    found
}

// Sample input: "zoxide\0query\0--list\0--all\0--score\0" in /proc/<pid>/cmdline; a zombie or reaped child reads empty.
pub(crate) fn child_argv(child: u32) -> String {
    let cmdline = fs::read(format!("/proc/{child}/cmdline")).unwrap_or_default();
    let args: Vec<String> = cmdline
        .split(|&byte| byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| format!("{:?}", String::from_utf8_lossy(arg)))
        .collect();
    args.join(" ")
}

pub(crate) fn threads(pid: u32) -> usize {
    fs::read_dir(format!("/proc/{pid}/task")).unwrap().count()
}

// Sample input: "4295 (flea-backend) S 4290 ... 9 6 ..."; after ')' utime and stime are fields 12 and 13, recorded for attribution only.
pub(crate) fn cpu_ticks(pid: u32) -> u64 {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields: Vec<&str> = stat[stat.rfind(')').unwrap() + 2..]
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

pub(crate) struct Process {
    pub(crate) child: Child,
    lines: mpsc::Receiver<String>,
}

impl Process {
    pub(crate) fn start(root: &Path, dir: &Path, log: &Path) -> Self {
        Self::start_with(root, dir, log, |_| {})
    }

    pub(crate) fn start_with(
        root: &Path,
        dir: &Path,
        log: &Path,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        fs::write(log.with_extension("paths"), b"").unwrap();
        fs::write(log.with_extension("spawns"), b"").unwrap();
        fs::write(log.with_extension("events"), b"").unwrap();
        let mut command = Command::new(binary());
        command.arg("--backend");
        sandboxed(&mut command, root);
        command
            .env("LD_PRELOAD", probe())
            .env("FLEA_PROBE_DIR", dir)
            .env("FLEA_PROBE_LOG", log)
            .env("FLEA_PROBE_SPAWNS", log.with_extension("spawns"))
            .env("FLEA_PROBE_EVENTS", log.with_extension("events"))
            .env("FLEA_PROBE_PATHS", log.with_extension("paths"));
        configure(&mut command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    pub(crate) fn send(&mut self, line: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{line}").unwrap();
    }

    pub(crate) fn next(&self) -> String {
        self.lines
            .recv_timeout(DEADLINE)
            .expect("the backend answers")
    }

    pub(crate) fn late_for(&self, settle: Duration) -> Vec<String> {
        let until = Instant::now() + settle;
        let mut late = Vec::new();
        while let Ok(line) = self
            .lines
            .recv_timeout(until.saturating_duration_since(Instant::now()))
        {
            late.push(line);
        }
        late
    }

    pub(crate) fn close(mut self) {
        drop(self.child.stdin.take());
        assert!(
            self.child.wait().unwrap().success(),
            "the backend leaves cleanly on EOF"
        );
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
