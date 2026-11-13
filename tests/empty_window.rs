use std::io::Write;
use std::path::Path;
use std::time::Instant;
use std::io::{BufRead, BufReader};
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const MOUNTINFO_OPENS: usize = 0;
const MIME_OPENS: usize = MOUNTINFO_OPENS + 1;
const ICON_OPENS: usize = MIME_OPENS + 1;
const ALIAS_OPENS: usize = ICON_OPENS + 1;
const THUMBNAILER_OPENS: usize = ALIAS_OPENS + 1;
const METADATA_CALLS: usize = THUMBNAILER_OPENS + 1;
const THREAD_STARTS: usize = METADATA_CALLS + 1;
const COUNTER_SLOT_COUNT: usize = THREAD_STARTS + 1;
const COUNTER_FILE_BYTES: usize = COUNTER_SLOT_COUNT * std::mem::size_of::<u64>();
const COUNTER_NAMES: [&str; COUNTER_SLOT_COUNT] = ["mountinfo opens", "MIME opens", "icon opens", "alias opens", "thumbnailer opens", "metadata calls", "thread starts"];
type Counters = [u64; COUNTER_SLOT_COUNT];

fn reader_only_counts() -> Counters {
    let mut counts = [0; COUNTER_SLOT_COUNT];
    // 14261ccb3's work-cache is_dir at startup: its ENOENT statx, then std's statx(0, NULL) availability probe.
    counts[METADATA_CALLS] = 2;
    counts[THREAD_STARTS] = 1;
    counts
}

fn wire_path(path: &Path) -> String {
    let mut text = String::new();
    for ch in path.to_str().expect("UTF-8 fixture path").chars() {
        match ch {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            ch if ch < ' ' => {
                use std::fmt::Write as _;
                write!(text, "\\u{:04x}", ch as u32).unwrap();
            }
            ch => text.push(ch),
        }
    }
    text
}

struct Backend {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Backend {
    fn start(dir: &TestDir) -> Self {
        Self::observed(dir, None)
    }

    fn observed(dir: &TestDir, observer: Option<(&Path, &Path)>) -> Self {
        Self::spawn(dir, observer, false)
    }

    fn spawn(dir: &TestDir, observer: Option<(&Path, &Path)>, prefetch: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flea"));
        command.arg("--backend")
            .env("HOME", dir.path()).env("XDG_CACHE_HOME", dir.path())
            .env_remove("XDG_RUNTIME_DIR").env_remove("FLEA_UNDO_DIR")
            .env_remove("FLEA_PREFETCH").env_remove("LD_PRELOAD")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if prefetch {
            command.env("FLEA_PREFETCH", dir.join("prefetch"))
                .env("FLEA_PREFETCH_SHELL", std::process::id().to_string());
        }
        if let Some((library, counters)) = observer {
            let runtime = dir.join("runtime");
            std::fs::create_dir_all(&runtime).unwrap();
            command.env("XDG_RUNTIME_DIR", runtime);
            command.env("LD_PRELOAD", library).env("FLEA_TEST_COUNTS", counters);
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self { child, input, output }
    }

    fn ask(&mut self, request: &str) -> String {
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        self.read()
    }

    fn read(&mut self) -> String {
        #[repr(C)]
        struct PollFd { fd: i32, events: i16, revents: i16 }
        extern "C" { fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32; }
        if self.output.buffer().is_empty() {
            let mut fd = PollFd { fd: self.output.get_ref().as_raw_fd(), events: 1, revents: 0 };
            assert!(unsafe { poll(&mut fd, 1, 5000) } > 0, "backend reply timed out");
        }
        let mut line = String::new();
        assert!(self.output.read_line(&mut line).unwrap() > 0, "backend exited before its reply");
        line.trim_end().to_string()
    }

    fn memory(&self) -> (usize, usize) {
        let body = std::fs::read_to_string(format!("/proc/{}/smaps_rollup", self.child.id())).unwrap();
        let value = |key: &str| body.lines().find(|line| line.starts_with(key)).unwrap()
            .split_whitespace().nth(1).unwrap().parse::<usize>().unwrap();
        (value("Pss:"), value("Rss:"))
    }

    fn quit(&mut self) {
        writeln!(self.input, "{{\"c\":\"quit\"}}").unwrap();
        self.input.flush().unwrap();
        for _ in 0..500 {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("idle backend did not quit");
    }
}

fn counters(path: &Path) -> Counters {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len(), COUNTER_FILE_BYTES);
    let mut counts = [0; COUNTER_SLOT_COUNT];
    for (slot, bytes) in counts.iter_mut().zip(bytes.as_chunks::<{ std::mem::size_of::<u64>() }>().0) {
        *slot = u64::from_ne_bytes(*bytes);
    }
    counts
}


struct TestDir { path: std::path::PathBuf }

impl TestDir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap().canonicalize().unwrap();
        assert!(root.is_absolute() && !root.starts_with(home));
        let output = Command::new("mktemp").arg("-d").arg(root.join(format!("flea-test-{tag}.XXXXXXXX"))).output().unwrap();
        assert!(output.status.success());
        let path = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        std::fs::write(path.join(".flea-test-sandbox"), "empty window test\n").unwrap();
        Self { path }
    }
    fn path(&self) -> &Path { &self.path }
    fn join(&self, name: &str) -> std::path::PathBuf { self.path.join(name) }
    fn file(&self, name: &str, text: &str) -> std::path::PathBuf {
        let path = self.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn dir(&self, name: &str) -> std::path::PathBuf {
        let path = self.join(name);
        std::fs::create_dir(&path).unwrap();
        path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        assert!(self.path.is_absolute() && !self.path.as_os_str().is_empty());
        assert_eq!(self.path.parent().unwrap(), std::env::temp_dir().canonicalize().unwrap());
        assert!(self.path.file_name().unwrap().to_str().unwrap().starts_with("flea-test-"));
        assert!(self.path.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

fn observer(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let source = dir.file("observer.c", include_str!("../src/backend/run_empty_observer.c"));
    let library = dir.join("observer.so");
    assert!(Command::new("cc").args(["-shared", "-fPIC", "-O2", "-Wall", "-Werror"])
        .arg(&source).arg("-o").arg(&library).args(["-ldl", "-pthread"]).status().unwrap().success());
    let counts = dir.file("counts", "");
    std::fs::File::options().write(true).open(&counts).unwrap().set_len(COUNTER_FILE_BYTES as u64).unwrap();
    (library, counts)
}

#[test]
fn empty_window_protocol() {
    let d = TestDir::new("empty-window-wire");
    let (library, counts) = observer(&d);
    let mut backend = Backend::observed(&d, Some((&library, &counts)));
    let empty = r#"{"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.000,"listing":0}"#;
    let reply = backend.ask(r#"{"c":"window","start":999,"count":0}"#);
    let cold = counters(&counts);
    eprintln!("cold counters {COUNTER_NAMES:?}={cold:?}");
    assert_eq!(cold, reader_only_counts(), "only the stdin reader starts before any row is demanded");
    assert_eq!(reply, empty);
    for _ in 0..1000 {
        assert_eq!(backend.ask(r#"{"c":"window","start":999,"count":0}"#), empty);
    }
    assert_eq!(counters(&counts), cold);
    let folder = d.dir("tree");
    std::fs::write(folder.join("hello.txt"), "hello").unwrap();
    let listed = backend.ask(&format!(r#"{{"c":"list","path":"{}","first":0}}"#, wire_path(&folder)));
    assert!(listed.contains(r#""n":1,"#), "{listed}");
    assert!(backend.read().contains(r#""rows":[]"#));
    let no_rows = counters(&counts);
    assert_eq!([no_rows[MIME_OPENS], no_rows[ICON_OPENS], no_rows[ALIAS_OPENS], no_rows[THUMBNAILER_OPENS]], [1, 0, 0, 0], "names-only listing demands only the picker MIME table");
    let row = backend.ask(r#"{"c":"window","start":0,"count":1,"listing":1}"#);
    assert!(row.contains(r#""n":"hello.txt","d":false,"s":5"#), "{row}");
    assert!(row.contains(r#""i":"text-x-generic""#), "{row}");
    assert!(row.contains(r#""kinds":["#) && row.ends_with(r#""listing":1}"#), "{row}");
    let warm = counters(&counts);
    assert_eq!([warm[MIME_OPENS], warm[ICON_OPENS], warm[ALIAS_OPENS]], [1, 1, 1], "each shared row table loads once");
    assert!(warm[METADATA_CALLS] > no_rows[METADATA_CALLS], "requested metadata is observed by the preload control");
    assert!(warm[MOUNTINFO_OPENS] > no_rows[MOUNTINFO_OPENS], "a real nonempty window positively controls mountinfo observation");
    for _ in 0..10 {
        let reply = backend.ask(r#"{"c":"window","start":999,"count":0,"listing":1}"#);
        assert_eq!(reply, r#"{"t":"rows","start":1,"rows":[],"kinds":[],"ms":0.000,"listing":1}"#);
    }
    assert_eq!(counters(&counts), warm, "empty windows start no services after a real window either");
    let refused = backend.ask(r#"{"c":"paths","rows":[0],"listing":0}"#);
    assert!(refused.contains("stale"), "rowguard still refuses an old listing: {refused}");
    let made = backend.ask(&format!(r#"{{"c":"mkdir","path":"{}","name":"made"}}"#, wire_path(d.path())));
    assert!(made.contains(r#""t":"made","ok":true"#), "{made}");
    assert!(d.join("runtime/flea/undo-journal").is_file(), "the first journal use attaches shared history");
    let undone = backend.ask(r#"{"c":"undo"}"#);
    assert!(undone.contains(r#""t":"undone","op":"mkdir","ok":true"#), "{undone}");
    assert!(!d.join("made").exists());
    let started = backend.ask(r#"{"c":"redo"}"#);
    assert!(started.contains(r#""t":"redostarted""#), "{started}");
    let redone = backend.read();
    assert!(redone.contains(r#""t":"redone","op":"mkdir","ok":true"#), "{redone}");
    assert!(d.join("made").is_dir());
    backend.quit();
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

#[test]
#[ignore = "real executable latency and memory probe"]
fn empty_window_probe() {
    let d = TestDir::new("empty-window-probe");
    let mut backend = Backend::start(&d);
    let expected = r#"{"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.000,"listing":0}"#;
    let _ = backend.ask(r#"{"c":"window","start":999,"count":0}"#);
    let memory = backend.memory();
    let threads = std::fs::read_dir(format!("/proc/{}/task", backend.child.id())).unwrap().count();
    let mut times = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let at = Instant::now();
        let reply = backend.ask(r#"{"c":"window","start":999,"count":0}"#);
        times.push(at.elapsed().as_nanos());
        assert_eq!(reply.split(",\"ms\":").next(), expected.split(",\"ms\":").next());
    }
    times.sort_unstable();
    eprintln!("empty-window n=1000 p50_us={:.3} p95_us={:.3} range_us={:.3}..{:.3} pss_kib={} rss_kib={} threads={} bytes={}",
        times[499] as f64 / 1000.0, times[949] as f64 / 1000.0,
        times[0] as f64 / 1000.0, times[999] as f64 / 1000.0, memory.0, memory.1, threads, expected.len() + 1);
}

#[test]
#[ignore = "real executable probe after a complete 100k name index without metadata windows"]
fn empty_window_index_probe() {
    let d = TestDir::new("empty-window-index");
    let tree = d.dir("tree");
    for i in 0..100_000 {
        std::fs::File::create(tree.join(format!("entry-{i:06}.txt"))).unwrap();
    }
    let mut backend = Backend::start(&d);
    let opened = backend.ask(&format!(r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#, wire_path(&tree), wire_path(&tree)));
    assert!(opened.contains(r#""t":"searchopened""#), "{opened}");
    writeln!(backend.input, "{{\"c\":\"searchquery\",\"query\":\"entry\",\"gen\":1,\"count\":0}}").unwrap();
    backend.input.flush().unwrap();
    let mut stamp = 0;
    loop {
        let line = backend.read();
        if line.contains(r#""t":"rows""#) {
            stamp = line.split(r#""listing":"#).nth(1).unwrap()
                .split([',', '}']).next().unwrap().parse::<u64>().unwrap();
        }
        if line.contains(r#""t":"searched""#) && line.contains(r#""walking":false"#) {
            assert!(line.contains(r#""scanned":100000,"#), "{line}");
            assert!(line.contains(r#""n":100000,"#), "{line}");
            break;
        }
    }
    let memory = backend.memory();
    let mut times = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let at = Instant::now();
        let reply = backend.ask(r#"{"c":"window","start":0,"count":0}"#);
        times.push(at.elapsed().as_nanos());
        assert!(reply.contains(r#""start":0,"rows":[],"kinds":[]"#), "{reply}");
        assert!(reply.ends_with(&format!(r#""listing":{stamp}}}"#)), "{reply}");
    }
    times.sort_unstable();
    eprintln!("empty-window-index n=1000 entries=100000 p50_us={:.3} p95_us={:.3} range_us={:.3}..{:.3} pss_kib={} rss_kib={}",
        times[499] as f64 / 1000.0, times[949] as f64 / 1000.0,
        times[0] as f64 / 1000.0, times[999] as f64 / 1000.0, memory.0, memory.1);
}

#[test]
fn empty_window_prefetch_protocol() {
    let d = TestDir::new("empty-window-prefetch");
    let (library, counts) = observer(&d);
    let mut backend = Backend::spawn(&d, Some((&library, &counts)), true);
    for _ in 0..10 {
        let reply = backend.ask(r#"{"c":"window","start":0,"count":0}"#);
        assert_eq!(reply, r#"{"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.000,"listing":0}"#);
    }
    assert_eq!(counters(&counts), reader_only_counts(), "prefetch recorder waits for a listing's rows");
    backend.quit();
    assert_eq!(counters(&counts), reader_only_counts(), "idle drain starts no unused service");
    assert!(!d.join("runtime/flea").exists(), "an unused journal creates no session directory");
}

#[path = "empty_window/transport.rs"]
mod transport;

#[path = "empty_window/review.rs"]
mod review;

#[path = "empty_window/wakes.rs"]
mod wakes;

#[path = "empty_window/inbox_open.rs"]
mod inbox_open;
