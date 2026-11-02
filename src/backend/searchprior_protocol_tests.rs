use super::*;
use crate::backend::testdir::TestDir;
use crate::json::{escape, field_bool, field_str, field_usize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{Receiver, channel};

struct Backend {
    child: Child,
    input: ChildStdin,
    output: Receiver<String>,
    gen: usize,
    open_request: String,
}

impl Backend {
    fn new(root: &TestDir, data: &Path, folder: &str) -> Self {
        let binary = PathBuf::from(env!("CARGO_BIN_EXE_flea"));
        assert!(binary.is_file(), "Cargo must build the production backend for this integration target: {}", binary.display());
        let mut child = Command::new(binary).arg("--backend")
            .env("HOME", root.path()).env("XDG_DATA_HOME", data).env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state")).env("XDG_CACHE_HOME", root.join("cache"))
            .env("_ZO_DATA_DIR", root.join("zoxide-data")).env("PATH", format!("{}:/usr/bin:/bin", root.join("bin").display()))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = channel();
        std::thread::spawn(move || { for line in BufReader::new(stdout).lines() { if tx.send(line.unwrap()).is_err() { break; } } });
        let open_request = format!(r#"{{"c":"searchopen","path":"{}","folder":"{}","hidden":false}}"#, escape(&root.join("scope").to_string_lossy()), escape(&root.join(folder).to_string_lossy()));
        let mut backend = Self { child, input, output, gen: 0, open_request };
        backend.reopen();
        backend
    }

    fn reopen(&mut self) {
        let request = self.open_request.clone();
        self.send(&request);
        loop { if field_str(&self.line(), "t").as_deref() == Some("searchopened") { break; } }
    }

    fn send(&mut self, request: &str) { writeln!(self.input, "{request}").unwrap(); self.input.flush().unwrap(); }

    fn line(&self) -> String { self.output.recv_timeout(Duration::from_secs(8)).expect("bounded backend reply") }

    fn query(&mut self, gen: usize, query: &str, count: usize) -> Vec<String> {
        let gen = gen.max(self.gen + 1);
        self.gen = gen;
        self.send(&format!(r#"{{"c":"searchquery","query":"{}","gen":{gen},"count":{count}}}"#, escape(query)));
        let mut rows = Vec::new();
        let mut listed = false;
        loop {
            let line = self.line();
            if field_usize(&line, "gen") != Some(gen) { continue; }
            match field_str(&line, "t").as_deref() {
                Some("listed") => { listed = true; rows.clear(); }
                Some("rows") => { assert!(listed, "rows must follow listed: {line}"); rows = names(&line); }
                Some("searched") if !field_bool(&line, "walking") => {
                    assert!(!field_bool(&line, "partial"), "{line}");
                    assert!(listed);
                    return rows;
                }
                _ => {}
            }
        }
    }

    fn eventually_after_reopen(&mut self, query: &str, count: usize, expected: &[&str]) -> Vec<String> {
        let started = std::time::Instant::now();
        loop {
            // A completed query in an old open cannot stand in for adoption by a new open.
            self.reopen();
            let rows = self.query(0, query, count);
            if rows == expected { return rows; }
            assert!(started.elapsed() < Duration::from_secs(4), "expected {expected:?}, got {rows:?}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Backend { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }

fn names(line: &str) -> Vec<String> {
    line.split("{\"n\":").skip(1).map(|part| field_str(&format!("{{\"n\":{part}"), "n").unwrap()).collect()
}

fn fixture() -> TestDir {
    let d = TestDir::new("searchprior-wire");
    d.dir("scope/a/project");
    d.dir("scope/z/project");
    d.dir("bin");
    d.dir("data");
    d.dir("home-data");
    for file in ["scope/a/report.rs", "scope/z/report.rs", "scope/render", "scope/z/render_more"] { d.file(file, ""); }
    d.script("bin/zoxide", "#!/bin/sh\nexit 127\n");
    d
}

#[test]
#[ignore]
fn searchprior_protocol_real_backend_protects_exact_and_ranks_files_folders_and_scrolls() {
    let d = fixture();
    let target = d.join("scope/z/report.rs");
    let xml = format!("<xbel><bookmark href='file://{}' visited='2026-10-18T00:00:00Z'/><bookmark href='file://{}'/></xbel>", target.display(), d.join("scope/z/render_more").display());
    let history = d.file("data/recently-used.xbel", &xml);
    let folders = d.script("bin/zoxide", &format!("#!/bin/sh\n[ \"$*\" = 'query --list --all --score' ] || exit 9\nprintf '90 %s\\n' '{}'\n", d.join("scope/z/project").display()));
    let provider_before = (std::fs::read(&history).unwrap(), std::fs::read(&folders).unwrap());
    let mut backend = Backend::new(&d, &d.join("data"), "scope/a");
    assert_eq!(backend.eventually_after_reopen("report.rs", 1, &["z/report.rs"]), ["z/report.rs"]);
    backend.send(r#"{"c":"window","start":0,"count":100}"#);
    loop {
        let line = backend.line();
        if field_str(&line, "t").as_deref() == Some("rows") {
            assert_eq!(names(&line), ["z/report.rs", "a/report.rs"]);
            break;
        }
    }
    assert_eq!(backend.query(2, "render", 100)[0], "render");
    assert_eq!(backend.query(3, "project", 100), ["z/project", "a/project"]);
    assert_eq!(provider_before, (std::fs::read(history).unwrap(), std::fs::read(folders).unwrap()));
    assert!(!d.join("state/flea/search-opens.v1").exists());
    assert!(!d.join("zoxide-data").exists(), "the provider double never creates or updates a database");
}

#[test]
#[ignore]
fn searchprior_protocol_missing_corrupt_unavailable_and_held_out_providers() {
    let d = fixture();
    let expected = ["a/report.rs", "z/report.rs"];
    {
        let mut backend = Backend::new(&d, &d.join("data"), "scope/z");
        assert_eq!(backend.query(1, "report.rs", 100), expected, "missing providers preserve fb2 ties even in a different pane folder");
    }
    d.file("data/recently-used.xbel", &format!("<xbel><bookmark href='file://{}'/>", d.join("scope/z/report.rs").display()));
    {
        let mut backend = Backend::new(&d, &d.join("data"), "scope/z");
        assert_eq!(backend.query(1, "report.rs", 100), expected, "a truncated document must not retain its valid prefix");
    }
    d.file("data/recently-used.xbel", &format!("<xbel><bookmark href='file://{}'/></xbel>", d.join("scope/z/unrelated-open.txt").display()));
    {
        let mut backend = Backend::new(&d, &d.join("data"), "scope/z");
        assert_eq!(backend.eventually_after_reopen("report.rs", 100, &["z/report.rs", "a/report.rs"]), ["z/report.rs", "a/report.rs"], "an unrelated open supplies folder context with the target held out");
    }
    d.file("data/recently-used.xbel", "<xbel/>");
    d.script("bin/zoxide", &format!("#!/bin/sh\nprintf '90 %s\\n' '{}'\nexit 1\n", d.join("scope/z/project").display()));
    let mut backend = Backend::new(&d, &d.join("data"), "scope/z");
    assert_eq!(backend.query(1, "report.rs", 100), expected, "a failed command's stdout must not become a prior");
    drop(backend);
    d.file("data/recently-used.xbel", "<broken>");
    d.script("bin/zoxide", &format!("#!/bin/sh\nprintf '90 %s\\n' '{}'\n", d.join("scope/z/project").display()));
    let mut backend = Backend::new(&d, &d.join("data"), "scope/a");
    assert_eq!(backend.eventually_after_reopen("project", 100, &["z/project", "a/project"]), ["z/project", "a/project"], "a corrupt file provider must not disable the folder provider");
}

#[test]
#[ignore]
fn searchprior_fix1_protocol_invalid_xml_keeps_fresh_ranking_after_providers_ready() {
    for name in ["1bad", "-bad", ".bad"] {
        let d = fixture();
        let xml = format!("<xbel><bookmark href='file://{}'/><{name}/></xbel>", d.join("scope/z/report.rs").display());
        d.file("data/recently-used.xbel", &xml);
        d.script("bin/zoxide", &format!("#!/bin/sh\nprintf '90 %s\\n' '{}'\n", d.join("scope/z/project").display()));
        let mut backend = Backend::new(&d, &d.join("data"), "scope/a");
        backend.eventually_after_reopen("project", 100, &["z/project", "a/project"]);
        let rows = backend.query(0, "report.rs", 100);
        println!("FB3_FIX1 malformed_name={name} rows={rows:?}");
        assert_eq!(rows, ["a/report.rs", "z/report.rs"], "the ready folder prior must not hide acceptance of malformed XML");
    }
}

#[test]
#[ignore]
fn searchprior_fix1_protocol_searchclose_during_provider_load() {
    let mut samples = Vec::new();
    for _ in 0..3 {
        let d = fixture();
        let ready = d.join("provider-ready");
        d.assert_contains(&ready);
        d.script("bin/zoxide", &format!("#!/bin/sh\nprintf ready > '{}'\nexec sleep 3\n", ready.display()));
        let mut backend = Backend::new(&d, &d.join("data"), "scope/a");
        backend.send(r#"{"c":"searchquery","query":"report.rs","gen":1,"count":0}"#);
        let started = std::time::Instant::now();
        while !ready.exists() {
            assert!(started.elapsed() < Duration::from_secs(4), "provider never started");
            std::thread::sleep(Duration::from_millis(1));
        }
        let closing = std::time::Instant::now();
        backend.send(r#"{"c":"searchclose"}"#);
        loop {
            if field_str(&backend.line(), "t").as_deref() == Some("searchclosed") { break; }
        }
        let ms = closing.elapsed().as_secs_f64() * 1000.0;
        println!("FB3_FIX1 searchclose_wait_ms={ms:.3}");
        samples.push(ms);
    }
    assert!(samples.iter().all(|&ms| ms < 500.0), "an optional provider blocked dispatch: {samples:?}");
}

#[test]
#[ignore]
fn searchprior_fix1_protocol_adopts_ready_priors_on_a_new_open() {
    let d = fixture();
    let ready = d.join("provider-ready");
    let release = d.join("provider-release");
    d.assert_contains(&ready);
    d.assert_contains(&release);
    d.file("data/recently-used.xbel", &format!("<xbel><bookmark href='file://{}'/></xbel>", d.join("scope/z/report.rs").display()));
    d.script("bin/zoxide", &format!("#!/bin/sh\nprintf ready > '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done\nprintf '90 %s\\n' '{}'\n", ready.display(), release.display(), d.join("scope/z/project").display()));
    let mut backend = Backend::new(&d, &d.join("data"), "scope/a");
    assert_eq!(backend.query(1, "report.rs", 100), ["a/report.rs", "z/report.rs"], "an unfinished snapshot must leave the current answer fresh");
    let started = std::time::Instant::now();
    while !ready.exists() {
        assert!(started.elapsed() < Duration::from_secs(4));
        std::thread::sleep(Duration::from_millis(1));
    }
    d.file("provider-release", "ready\n");
    backend.eventually_after_reopen("report.rs", 100, &["z/report.rs", "a/report.rs"]);
    assert_eq!(backend.query(0, "project", 100), ["z/project", "a/project"]);
}

fn corrupt_xml_after_readiness(form: &str, fragment: &str) {
    let d = fixture();
    let xml = format!("<xbel><bookmark href='file://{}'/>{fragment}</xbel>", d.join("scope/z/report.rs").display());
    let history = d.file("data/recently-used.xbel", &xml);
    d.script("bin/zoxide", &format!("#!/bin/sh\nprintf '90 %s\\n' '{}'\n", d.join("scope/z/project").display()));
    let mut backend = Backend::new(&d, &d.join("data"), "scope/a");
    backend.eventually_after_reopen("project", 100, &["z/project", "a/project"]);
    let rows = backend.query(0, "report.rs", 100);
    println!("FB3_FIX2 malformed={form} snapshot=ready rows={rows:?}");
    assert_eq!(std::fs::read(history).unwrap(), xml.as_bytes());
    assert_eq!(rows, ["a/report.rs", "z/report.rs"], "a corrupt file provider must contribute no priors after readiness");
}

#[test]
#[ignore]
fn searchprior_fix2_protocol_processing_instruction() {
    corrupt_xml_after_readiness("processing-instruction", "<??>");
}

#[test]
#[ignore]
fn searchprior_fix2_protocol_cdata() {
    corrupt_xml_after_readiness("cdata", "<![CDATA[\u{0}]]>");
}

#[test]
#[ignore]
fn searchprior_fix2_protocol_text_terminator() {
    corrupt_xml_after_readiness("text-terminator", "]]>");
}
