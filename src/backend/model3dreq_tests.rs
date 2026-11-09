use super::*;
use crate::jsondoc::Json;
use crate::backend::{model3d::testdata, testdir::TestDir};

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
    let held_ticket = codereq::begin("model3d-test-held");
    let _held = PARSING.enter(&held_ticket,false,Instant::now()+Duration::from_secs(1)).unwrap();
    let started = Instant::now();
    let result = ensure_until(&input,root.path(),None,&ticket,started+Duration::from_millis(30),false);
    assert!(result.err().unwrap().contains("timed out"));
    assert!(started.elapsed() >= Duration::from_millis(30));
    assert!(started.elapsed() < Duration::from_secs(1));
    ticket.discard();
    drop(_held);
    held_ticket.discard();
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

#[test]
fn a_raw_named_model_reaches_the_worker_as_its_own_file() {
    use std::os::unix::ffi::OsStrExt;
    let root = TestDir::new("model3d-raw-name");
    let input = root
        .path()
        .join(std::ffi::OsStr::from_bytes(b"part-\xff.obj"));
    std::fs::write(&input, testdata::OBJ).unwrap();
    std::fs::write(root.path().join("part-\u{fffd}.obj"), b"twin").unwrap();
    let inner: Vec<String> = vec![
        "/app/flea".into(),
        "--backend".into(),
        "--3d-worker".into(),
        "obj".into(),
        "--".into(),
        input.exact_text().into_owned(),
    ];
    let args = worker_args(flea::bytename::args(&inner).map(|arg| arg.into_owned()));
    let mut bytes = Vec::new();
    regfile::open_if_regular(&flea::bytename::view(&args[5]), 0)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, testdata::OBJ);
}
