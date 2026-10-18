use super::*;
use crate::backend::{model3d::testdata, testdir::TestDir};
use crate::jsondoc::Json;
use std::io::{BufRead,BufReader,Write};
use std::process::{Child,ChildStdin,Command};
use std::sync::mpsc::{channel,Receiver};

struct Session { child: Child, input: ChildStdin, lines: Receiver<String> }

impl Session {
    fn start(root: &TestDir, path: Option<&Path>) -> Option<Self> {
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("flea");
        if !exe.is_file() {
            std::io::stderr().write_all(b"SKIP model3d protocol: build --locked --bin flea before this test to supply the actual backend executable\n").unwrap();
            return None;
        }
        let home = root.dir("home");
        let mut command = Command::new(exe);
        command.arg("--backend").env("HOME",&home).env("XDG_CACHE_HOME",root.join("cache"))
            .env("XDG_CONFIG_HOME",root.join("config")).env("XDG_DATA_HOME",root.join("data"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        if let Some(path) = path { command.env("PATH",path); }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx,lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() { break }
            }
        });
        Some(Self { child, input, lines })
    }

    fn send(&mut self, line: &str) { writeln!(self.input,"{line}").unwrap(); self.input.flush().unwrap(); }

    fn response(&self, kind: &str) -> Json {
        let until = Instant::now()+Duration::from_secs(25);
        loop {
            let line = self.lines.recv_timeout(until.saturating_duration_since(Instant::now())).expect("the backend did not answer within its deadline");
            let answer = crate::jsondoc::parse(&line).unwrap();
            if answer.get("t").and_then(Json::as_str) == Some(kind) { return answer }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = writeln!(self.input,"{{\"c\":\"quit\"}}");
        let end = Instant::now()+Duration::from_secs(3);
        while Instant::now() < end {
            if self.child.try_wait().unwrap().is_some() { return }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ask(path: &Path, token: usize) -> String {
    format!(r#"{{"c":"model3d","slot":"look","token":{token},"path":"{}"}}"#,escape(&path.to_string_lossy()))
}

#[test]
fn protocol_real_sandbox_model_facts_and_existing_thumbnail_cache() {
    if crate::backend::sandboxprobe::skipped() { return }
    let root = TestDir::new("model3d-protocol");
    let path = root.join("kit.3mf");
    root.assert_contains(&path);
    std::fs::write(&path,testdata::project()).unwrap();
    let Some(mut session) = Session::start(&root,None) else { return };
    session.send(&format!(r#"{{"c":"list","path":"{}","first":100}}"#,escape(&root.path().to_string_lossy())));
    let rows = session.response("rows");
    let rows = rows.get("rows").unwrap().as_array().unwrap();
    let row = rows.iter().position(|r| r.get("n").and_then(Json::as_str) == Some("kit.3mf")).unwrap();
    assert_eq!(rows[row].get("t").and_then(Json::as_bool),Some(true));
    session.send(&format!(r#"{{"c":"thumb","rows":[{row}],"cacheOnly":true}}"#));
    assert_eq!(session.response("thumbed").get("file").and_then(Json::as_str),Some(""));
    session.send(&ask(&path,7));
    let ready = session.response("model3d");
    assert_eq!(ready.get("state").and_then(Json::as_str),Some("ready"),"{ready:?}");
    assert_eq!(ready.get("token").and_then(Json::as_f64),Some(7.));
    let file = PathBuf::from(ready.get("file").unwrap().as_str().unwrap());
    root.assert_contains(&file);
    let model = model3d::wire::read(std::fs::File::open(&file).unwrap()).unwrap();
    assert_eq!(model.objects.len(),2);
    assert_eq!(model.plates.len(),2);
    assert_eq!(model.facts(None).open_edges,Some(0));
    let thumbnail = PathBuf::from(ready.get("thumbnail").unwrap().as_str().unwrap());
    root.assert_contains(&thumbnail);
    assert!(cache::png(&std::fs::read(&thumbnail).unwrap()));
    session.send(&format!(r#"{{"c":"thumb","rows":[{row}]}}"#));
    let thumbed = session.response("thumbed");
    let image = PathBuf::from(thumbed.get("file").unwrap().as_str().unwrap());
    root.assert_contains(&image);
    let png = std::fs::read(image).unwrap();
    assert!(cache::png(&png));
    assert_eq!(thumbcache::png_text(&png,"Thumb::URI"),Some(thumbcache::uri_for(&path)));
    session.send(&ask(&path,8));
    assert_eq!(session.response("model3d").get("file").and_then(Json::as_str),Some(file.to_str().unwrap()));
    root.assert_contains(&path);
    std::fs::write(&path,b"broken").unwrap();
    session.send(&ask(&path,9));
    let failed = session.response("model3d");
    assert_eq!(failed.get("state").and_then(Json::as_str),Some("failed"));
    assert_eq!(failed.get("message").and_then(Json::as_str),Some("This file cannot be previewed."));
    session.send(r#"{"c":"model3d","slot":"look","token":10,"cancel":true}"#);
    assert_eq!(session.response("model3d").get("state").and_then(Json::as_str),Some("cancelled"));
}

#[test]
fn protocol_missing_sandbox_refuses_before_parsing_or_publishing() {
    let root = TestDir::new("model3d-no-sandbox");
    let input = root.file("part.obj",std::str::from_utf8(testdata::OBJ).unwrap());
    let empty = root.dir("empty-path");
    let Some(mut session) = Session::start(&root,Some(&empty)) else { return };
    session.send(&ask(&input,1));
    let failed = session.response("model3d");
    assert_eq!(failed.get("state").and_then(Json::as_str),Some("failed"));
    assert!(failed.get("reason").and_then(Json::as_str).unwrap().contains("bwrap and prlimit"));
    assert!(!root.join("cache/thumbnails/flea-3d").read_dir().unwrap().any(|e| e.unwrap().path().extension().is_some_and(|e| e == "3d")));
}

#[test]
fn cache_rejects_broken_png_and_model_headers() {
    assert!(cache::png(&testdata::PNG));
    for n in 0..testdata::PNG.len() { assert!(!cache::png(&testdata::PNG[..n])); }
    let mut model = model3d::parse("obj",testdata::OBJ).unwrap();
    model.thumbnails.push(model3d::Thumbnail { plate: None, format: "png".into(), bytes: testdata::PNG.to_vec() });
    let mut bytes = Vec::new();
    model3d::wire::write(&model,&mut bytes).unwrap();
    for n in 0..bytes.len() {
        assert!(std::panic::catch_unwind(|| model3d::wire::read(&bytes[..n])).is_ok());
        assert!(model3d::wire::read(&bytes[..n]).is_err());
    }
}

#[test]
fn every_protocol_state_is_valid_json_including_error_quotes() {
    let ask = Ask::parse(r#"{"token":3,"slot":"look","path":"/a.obj"}"#);
    for state in ["failed","cancelled"] {
        let line = reply(&ask,state,None,Some("This file cannot be previewed."),"a \"quoted\" refusal");
        let json = crate::jsondoc::parse(&line).unwrap();
        assert_eq!(json.get("reason").and_then(Json::as_str),Some("a \"quoted\" refusal"));
    }
    let ready = Ready { file: "/cache/a.3d".into(), thumbnail: None, summary: "{}".into() };
    crate::jsondoc::parse(&reply(&ask,"ready",Some(&ready),None,"")).unwrap();
}

#[test]
fn waiting_for_another_parser_is_part_of_the_same_deadline() {
    let root = TestDir::new("model3d-busy");
    let input = root.file("model.obj",std::str::from_utf8(testdata::OBJ).unwrap());
    let ticket = codereq::begin("model3d-test-busy");
    let _held = PARSING.lock().unwrap();
    let started = Instant::now();
    let result = ensure_until(&input,root.path(),None,&ticket,started+Duration::from_millis(30));
    assert!(result.err().unwrap().contains("timed out"));
    assert!(started.elapsed() >= Duration::from_millis(30));
    assert!(started.elapsed() < Duration::from_secs(1));
    ticket.discard();
}

#[test]
fn cache_sweep_removes_only_dead_parser_work_and_preserves_live_jobs() {
    let root = TestDir::new("model3d-work-sweep");
    let old = std::time::SystemTime::now()-Duration::from_secs(120);
    let dead = root.dir(".flea-work-model3d-4294967295-0");
    let live = root.dir(&format!(".flea-work-model3d-{}-1",std::process::id()));
    let unrelated = root.dir("another-preview-cache");
    for dir in [&dead,&live,&unrelated] {
        root.assert_contains(dir);
        std::fs::File::open(dir).unwrap().set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
    }
    root.assert_contains(root.path());
    cache::evict(root.path());
    assert!(!dead.exists());
    assert!(live.exists());
    assert!(unrelated.exists());
}

#[test]
fn sandbox_argv_keeps_file_after_double_dash_and_binds_only_input_and_executable() {
    let inner = vec!["/app/flea".into(),"--backend".into(),"--3d-worker".into(),"obj".into(),"--".into(),"/in/-part.obj".into()];
    let argv = sandbox::wrap_readonly_extra(&inner,&[Path::new("/in/-part.obj"),Path::new("/app/flea")]);
    assert_eq!(&argv[argv.len()-6..],inner.as_slice());
    assert!(argv.iter().any(|v| v == "--cpu=30"));
    assert!(argv.iter().any(|v| v == "--as=2147483648"));
    assert!(argv.iter().any(|v| v == "--unshare-all"));
    assert!(!argv.iter().any(|v| v == "--bind"));
}
