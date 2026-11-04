use super::*;
use crate::json::{escape, field_str, field_usize};
use std::os::unix::ffi::OsStringExt;

struct Fixture {
    root: PathBuf,
    st: State,
    tb: Tables,
    pool: Pool,
    cache: Cache,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    out: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let parent = std::env::current_dir()
            .unwrap()
            .join(".superpowers/m9-g2rows-01");
        std::fs::create_dir_all(&parent).unwrap();
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("fixture.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
        assert!(root.is_absolute() && root.starts_with(&parent));
        std::fs::write(root.join(".flea-test-sandbox"), b"contents test\n").unwrap();
        let (tx, rx) = crate::backend::events::channel();
        Self {
            st: State::new(super::super::dirsizeworker::Worker::new(tx.clone())),
            tb: Tables::load(),
            pool: Pool::idle(),
            cache: Cache::at(root.join("cache")),
            root,
            tx,
            rx,
            out: Vec::new(),
        }
    }

    fn event(&mut self, event: Event) {
        let mut watch = Watch::start(self.tx.clone());
        self.event_watched(event, &mut watch);
    }

    fn event_watched(&mut self, event: Event, watch: &mut Watch) {
        let mut ops = Ops::new(channel().0);
        let mut flash = flashslot::Ctx::new(channel().0);
        let mut peeks = PeekWatch::start(self.tx.clone());
        let mut fsinfo = FsInfo::new(self.tx.clone());
        let poller = super::super::watchpoll::Poller::new(self.tx.clone());
        let mut gate = Gate::new(self.tx.clone());
        assert!(
            dispatch(
                event,
                &mut self.out,
                &mut self.st,
                &self.tb,
                &self.pool,
                &self.cache,
                &mut ops,
                &mut flash,
                watch,
                &mut peeks,
                &mut fsinfo,
                &poller,
                &self.tx,
                &mut gate
            ) == Control::Continue
        );
    }

    fn request(&mut self, line: &str) {
        self.event(Event::Request(line.to_owned()));
    }

    fn search(&mut self, hidden: bool, gen: usize, count: usize) {
        self.request(&format!(r#"{{"c":"searchcontents","path":"{}","query":"needle","hidden":{hidden},"gen":{gen},"count":{count}}}"#, escape(&self.root.to_string_lossy())));
    }

    fn text(&self) -> String {
        String::from_utf8(self.out.clone()).unwrap()
    }

    fn finish(&mut self) {
        self.until("searched");
    }

    fn until(&mut self, kind: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self
            .text()
            .lines()
            .any(|line| field_str(line, "t").as_deref() == Some(kind))
        {
            if self.text().contains(r#""t":"error""#) {
                panic!("contents request failed: {}", self.text());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let event = self
                .rx
                .recv_timeout(remaining)
                .expect("contents producer must finish");
            self.event(event);
        }
    }
}

#[path = "contents_fixlogic_tests.rs"]
mod fixlogic;
#[path = "contents_lifecycle_tests.rs"]
mod lifecycle;
#[path = "contents_review_tests.rs"]
mod review;

#[test]
fn m9_g2rows_01_rg_ignore_hidden_binary_first_line() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    std::fs::write(f.root.join(".gitignore"), "ignored.txt\n").unwrap();
    std::fs::write(f.root.join("ignored.txt"), "needle ignored\n").unwrap();
    std::fs::write(
        f.root.join("visible.txt"),
        "before\nneedle first\nneedle second\n",
    )
    .unwrap();
    std::fs::write(f.root.join(".hidden.txt"), "needle hidden\n").unwrap();
    std::fs::write(f.root.join("binary.bin"), b"\0needle binary\n").unwrap();
    f.search(false, 1, 1);
    f.finish();
    assert_eq!(f.st.listing.len(), 1);
    assert_eq!(f.st.listing.name(0), "visible.txt");
    assert!(f.text().contains("needle first"));
    assert!(!f.text().contains("needle second"));
    f.out.clear();
    f.search(true, 2, 10);
    f.finish();
    assert_eq!(f.st.listing.len(), 2);
    std::fs::write(f.root.join("-literal.txt"), "-needle.[x]\n").unwrap();
    std::fs::write(f.root.join("regex-decoy.txt"), "-needleXx\n").unwrap();
    f.out.clear();
    f.request(&format!(
        r#"{{"c":"searchcontents","path":"{}","query":"-needle.[x]","gen":3,"count":10}}"#,
        escape(&f.root.to_string_lossy())
    ));
    f.finish();
    assert_eq!(f.st.listing.len(), 1);
    assert_eq!(f.st.listing.name(0), "-literal.txt");
}

#[test]
fn m9_g2rows_01_cancel_reaps_and_stale_generation_is_ignored() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    if std::env::var_os("FLEA_CONTENTS_CI_ENV").is_some() {
        use std::io::IsTerminal;
        assert!(!std::io::stdin().is_terminal());
        assert!(std::env::vars().all(|(key, _)| [
            "PATH",
            "HOME",
            "TMPDIR",
            "FLEA_CONTENTS_CI_ENV"
        ]
        .contains(&key.as_str())));
    }
    let mut f = Fixture::new();
    std::fs::write(f.root.join("one.txt"), "needle\n").unwrap();
    f.search(false, 7, 1);
    f.finish();
    let listing = f.st.generation;
    f.out.clear();
    f.search(false, 6, 1);
    assert!(f.out.is_empty(), "old generation must be silent");
    assert_eq!(f.st.generation, listing);
    f.search(false, 8, 1);
    f.request(r#"{"c":"searchcancel"}"#);
    f.finish();
    let out = f.text();
    assert!(
        out.contains(r#""cancelled":true"#),
        "cancel must acknowledge its own generation: {out}"
    );
    assert!(out.contains(r#""gen":8"#));
    f.out.clear();
    let stale = super::super::contents::Snapshot {
        ask: super::super::contents::Ask {
            path: f.root.to_string_lossy().into(),
            query: "old".into(),
            hidden: false,
            gen: 7,
            count: 1,
        },
        matches: Vec::new(),
        scanned: 0,
        dev: 0,
        writable: false,
        ms: 0.,
        partial: false,
    };
    f.event(Event::Contents(super::super::contents::Done {
        ticket: 1,
        result: super::super::contents::Outcome::Ready(stale),
    }));
    assert!(
        f.out.is_empty(),
        "a delivered stale completion cannot publish"
    );
    assert_eq!(f.st.generation, listing);

    for before_spawn in [true, false] {
        let mut f = Fixture::new();
        let tool = f.root.join("held-rg");
        let mut writer = std::process::Command::new("/bin/sh")
            .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
            .arg(&tool)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        writer
            .stdin
            .take()
            .unwrap()
            .write_all(b"#!/usr/bin/python3\nimport time\ntime.sleep(60)\n")
            .unwrap();
        assert!(writer.wait().unwrap().success());
        f.st.contents.program = Some(tool);
        let probe = f.st.contents.probe.clone();
        probe.hold.store(true, std::sync::atomic::Ordering::Release);
        probe
            .before_spawn
            .store(before_spawn, std::sync::atomic::Ordering::Release);
        f.search(false, 1, 1);
        let until = Instant::now() + Duration::from_secs(5);
        while !probe.reached.load(std::sync::atomic::Ordering::Acquire) {
            assert!(
                Instant::now() < until,
                "worker must reach controlled startup"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let launcher = probe.launcher.load(std::sync::atomic::Ordering::Acquire);
        let mut sandbox = 0;
        if !before_spawn {
            while sandbox <= 0 {
                sandbox = probe
                    .sandbox
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map_or(0, |pid| pid.load(std::sync::atomic::Ordering::Acquire));
                assert!(
                    Instant::now() < until,
                    "real sandbox must report its init PID"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(launcher > 0);
        }
        f.search(false, 2, 1);
        if before_spawn {
            f.request(r#"{"c":"searchcancel"}"#);
            f.finish();
        } else {
            f.request(r#"{"c":"searchclose"}"#);
            f.until("searchclosed");
        }
        assert!(probe.reaped.load(std::sync::atomic::Ordering::Acquire));
        for pid in [launcher, sandbox].into_iter().filter(|pid| *pid > 0) {
            assert!(
                crate::procsignal::signal_process(pid, 0).is_err(),
                "cancel left real child {pid} alive"
            );
        }
        assert!(f.text().contains(r#""gen":2"#));
        assert!(!f.text().contains("contentsmatches"));
    }
}

#[test]
fn m9_g2rows_01_raw_paths_and_remainder_keep_listing_identity() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    for name in ["-dash.txt", "two words.txt", "café.txt", "last.txt"] {
        std::fs::write(f.root.join(name), "needle\n").unwrap();
    }
    let raw = std::ffi::OsString::from_vec(b"raw-\xff.txt".to_vec());
    std::fs::write(f.root.join(&raw), "needle raw\n").unwrap();
    std::fs::write(f.root.join("raw-�.txt"), "needle real UTF-8 neighbor\n").unwrap();
    f.search(false, 1, 1);
    f.finish();
    assert_eq!(
        f.st.listing.len(),
        6,
        "first-window count must never cap complete listing"
    );
    let generation = f.st.generation;
    f.out.clear();
    f.request(r#"{"c":"window","start":1,"count":5}"#);
    assert_eq!(f.st.generation, generation);
    let rows = f.text();
    assert_eq!(field_usize(&rows, "start"), Some(1));
    assert_eq!(field_usize(&rows, "listing"), Some(generation as usize));
    assert!(rows.contains("raw-"));
    assert!(rows.contains('�'), "oracle retains lossy display text");
    assert!(rows.contains(r#""operable":false"#));
    eprintln!("CONTENTS-RAW-WINDOW {rows}");
    let held = f.st.contents.held.as_ref().unwrap();
    assert!(held
        .matches
        .iter()
        .any(|hit| hit.path_bytes == b"raw-\xff.txt" && !hit.operable));
    assert!(held
        .matches
        .iter()
        .any(|hit| hit.path_bytes == "raw-�.txt".as_bytes() && !hit.operable));
    for row in 0..f.st.listing.len() {
        if f.st.listing.name(row) != "raw-�.txt" {
            continue;
        }
        f.out.clear();
        f.request(&format!(
            r#"{{"c":"paths","rows":[{row}],"listing":{generation}}}"#
        ));
        assert!(
            f.text().contains(r#""t":"error""#),
            "unsafe row cannot resolve to operation paths"
        );
    }
    let alias = escape(&f.root.join("raw-�.txt").to_string_lossy());
    for line in [
        format!(r#"{{"c":"rename","path":"{alias}","to":"changed.txt"}}"#),
        format!(r#"{{"c":"trash","paths":["{alias}"]}}"#),
        format!(r#"{{"c":"locate","path":"{alias}"}}"#),
        format!(r#"{{"c":"open","path":"{alias}"}}"#),
        format!(r#"{{"c":"reveal","path":"{alias}"}}"#),
        format!(r#"{{"c":"compare","left":"{alias}","right":"safe.txt"}}"#),
    ] {
        f.out.clear();
        f.request(&line);
        assert!(
            f.text().contains(r#""where":"contentsidentity""#),
            "explicit display aliases must be refused: {}",
            f.text()
        );
    }
    assert_eq!(
        std::fs::read_to_string(f.root.join("raw-�.txt")).unwrap(),
        "needle real UTF-8 neighbor\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.root.join(raw)).unwrap(),
        "needle raw\n"
    );
}

#[test]
fn m9_g2rows_01_missing_rg_and_failure_never_publish_success() {
    let mut f = Fixture::new();
    f.st.contents.program = Some(f.root.join("missing-rg"));
    f.search(false, 1, 1);
    f.until("error");
    assert!(!f.text().contains("searched"));
    assert!(!f.text().contains("listed"));
    assert!(f.text().contains(r#""gen":1"#));
    let tool = f.root.join("failing-rg");
    let mut writer = std::process::Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(&tool)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    writer.stdin.take().unwrap().write_all(b"#!/usr/bin/python3\nimport pathlib,sys\ntry:\n pathlib.Path('must-not-write').write_text('bad')\nexcept OSError as e:\n print(str(e),file=sys.stderr)\nsys.exit(42)\n").unwrap();
    assert!(writer.wait().unwrap().success());
    f.st.contents.program = Some(tool);
    f.out.clear();
    f.search(false, 2, 1);
    f.until("error");
    assert!(
        !f.root.join("must-not-write").exists(),
        "real jail must stay read-only"
    );
    assert!(!f.text().contains("searched"));
    assert!(!f.text().contains("listed"));
    assert!(f.text().contains(r#""gen":2"#));
}
