use super::*;

#[test]
fn print_and_export_jobs_are_not_replaced_by_later_previews() {
    let d = TestDir::new("officepdf-required");
    let r = rig(&d, GATED, CONVERT_TIMEOUT);
    let gate = d.join("running.docx");
    mkfifo(&gate);
    let required = d.file("required.docx", "office\n");
    let preview = d.file("preview.docx", "preview\n");
    assert_eq!(r.ask(1, &gate), converting_line(1));
    let service = Arc::clone(&r.svc);
    let source = required.clone();
    let fetch = std::thread::spawn(move || service.fetch(&source, Arc::new(AtomicBool::new(false))));
    let end = Instant::now() + BOUND;
    while r.svc.locked().required.is_empty() && Instant::now() < end { std::thread::sleep(Duration::from_millis(5)); }
    assert_eq!(r.svc.locked().required.len(), 1);
    assert_eq!(r.ask(2, &preview), converting_line(2));
    within("the running office conversion is released", move || drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap()));
    let pdf = fetch.join().unwrap().unwrap();
    assert_eq!(pdf, r.svc.store.pdf_path(&Store::key(&required)));
    assert_eq!(std::fs::read(&pdf).unwrap(), b"%PDF-1.4 gated");
    assert!([r.end(), r.end()].iter().all(|(_, line)| line.contains("ready")));
    assert_eq!(r.jobs(), 3);
    assert_eq!(r.svc.fetch(&required, Arc::new(AtomicBool::new(false))).unwrap(), pdf);
    assert_eq!(r.jobs(), 3, "export reuses the preview cache");
}

#[test]
fn cancelling_a_required_office_job_does_not_cache_a_failure() {
    let d = TestDir::new("officepdf-required-cancel");
    let r = rig(&d, HANGS, CONVERT_TIMEOUT);
    let source = d.file("letter.docx", "office");
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancel);
    let service = Arc::clone(&r.svc);
    let fetch = std::thread::spawn(move || service.fetch(&source, stop));
    let end = Instant::now() + BOUND;
    while r.jobs() == 0 && Instant::now() < end { std::thread::sleep(Duration::from_millis(5)); }
    assert_eq!(r.jobs(), 1);
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(fetch.join().unwrap().unwrap_err(), "cancelled");
    assert_eq!(r.fail_markers(), 0);
}

#[test]
fn an_older_request_that_finishes_its_probe_last_is_replaced_and_the_newer_stays_queued() {
    let d = TestDir::new("officepdf-order");
    let (mut r, (open_tx, open_rx)) = (rig(&d, GATED, CONVERT_TIMEOUT), channel::<()>());
    let (gate_rx, entered) = (Mutex::new(open_rx), Arc::new(AtomicUsize::new(0)));
    let seen = Arc::clone(&entered);
    // The second probe to start is the older request's, and it waits here.
    Arc::get_mut(&mut r.svc).unwrap().resolve = Box::new(move || {
        if seen.fetch_add(1, Ordering::SeqCst) == 1 {
            let _ = gate_rx.lock().unwrap().recv_timeout(Duration::from_secs(10));
        }
        Some("/usr/bin/sh".to_string())
    });
    let gate = d.join("x.docx");
    mkfifo(&gate);
    let (older, newer) = (d.file("older.docx", "o"), d.file("newer.docx", "n"));
    assert_eq!(r.ask(1, &gate), converting_line(1));
    r.svc.start(2, 2, older, false, None, r.tx.clone());
    while entered.load(Ordering::SeqCst) < 2 {
        std::thread::sleep(Duration::from_millis(5));
    }
    r.svc.start(3, 3, newer, false, None, r.tx.clone());
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(3)));
    open_tx.send(()).unwrap();
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(2)));
    let (reg, line) = r.end();
    assert!(reg == 2 && line.contains(r#""state":"failed""#) && line.contains("replaced"), "the older request is the replaced one: {reg} {line}");
    within("the gate opens", move || drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap()));
    let mut ends = vec![r.end(), r.end()];
    ends.sort();
    assert_eq!(ends.iter().map(|e| e.0).collect::<Vec<_>>(), vec![1, 3]);
    assert!(ends.iter().all(|e| e.1.contains(r#""state":"ready""#)), "{ends:?}");
    assert_eq!(r.jobs(), 2, "the older request never ran");
}

#[test]
fn a_request_older_than_the_running_job_is_dropped_and_never_converts() {
    let d = TestDir::new("officepdf-stale");
    let r = rig(&d, GATED, CONVERT_TIMEOUT);
    let gate = d.join("running.docx");
    mkfifo(&gate);
    let (stale, fresh) = (d.file("stale.docx", "s"), d.file("fresh.docx", "f"));
    r.svc.request(1, 1, 2, &gate, false, None, &r.tx);
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(1)));
    while r.jobs() == 0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(r.svc.locked().queued.is_none(), "the newer job left the queue and runs");
    r.svc.request(2, 2, 1, &stale, false, None, &r.tx);
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(2)));
    let (reg, line) = r.end();
    assert!(reg == 2 && line.contains(r#""state":"failed""#) && line.contains("replaced"), "the older request is answered replaced: {reg} {line}");
    assert!(r.svc.locked().queued.is_none(), "an older request is never queued");
    r.svc.request(3, 3, 3, &fresh, false, None, &r.tx);
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(3)));
    within("the gate opens", move || drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap()));
    let mut ends = vec![r.end(), r.end()];
    ends.sort();
    assert_eq!(ends.iter().map(|e| e.0).collect::<Vec<_>>(), vec![1, 3]);
    assert!(ends.iter().all(|e| e.1.contains(r#""state":"ready""#)), "{ends:?}");
    assert_eq!(r.jobs(), 2, "the older request never ran, the newer one did");
    assert!(r.svc.locked().running.is_none(), "the running waiters are cleared when the work ends");
}

#[test]
fn an_older_request_whose_probe_lands_after_the_newer_job_finished_starts_no_conversion() {
    let d = TestDir::new("officepdf-late");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let (late, fresh) = (d.file("late.docx", "l"), d.file("fresh.docx", "f"));
    r.svc.request(1, 1, 2, &fresh, false, None, &r.tx);
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(1)));
    let (reg, line) = r.end();
    assert!(reg == 1 && line.contains(r#""state":"ready""#), "the newer job finished: {reg} {line}");
    while r.svc.locked().running.is_some() {
        std::thread::sleep(Duration::from_millis(5));
    }
    r.svc.request(2, 2, 1, &late, false, None, &r.tx);
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(2)));
    let (reg, line) = r.end();
    assert!(reg == 2 && line.contains(r#""state":"failed""#) && line.contains("replaced"), "the late older request is answered replaced: {reg} {line}");
    assert_eq!(r.jobs(), 1, "the late older request started no conversion");
}

#[test]
fn the_conversion_takes_the_canonical_input_the_probe_resolved() {
    let d = TestDir::new("officepdf-canonical");
    let mut r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let (real, link) = (d.file("real.docx", "doc"), d.join("link.docx"));
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let inputs = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&inputs);
    let build: Build = Box::new(move |_, _, input, out| {
        seen.lock().unwrap().push(input.to_path_buf());
        ["/usr/bin/sh", "-c", MAKE_PDF, "sh", &input.to_string_lossy(), &out.to_string_lossy()].iter().map(|s| s.to_string()).collect()
    });
    r.svc = Arc::new(Service::new(Store::new(&r.cache), Box::new(|| Some("/usr/bin/sh".to_string())), build, Box::new(unjailed), CONVERT_TIMEOUT));
    r.ask(1, &link);
    r.end();
    assert_eq!(*inputs.lock().unwrap(), vec![std::fs::canonicalize(&real).unwrap()]);
}

#[test]
fn the_conversion_makes_no_path_call_of_its_own() {
    let d = TestDir::new("officepdf-no-path-call");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let absent = d.join("resolved-by-the-probe.docx");
    let job = Job { key: "k".repeat(KEY_HEX_CHARS), program: "/usr/bin/sh".into(), input: absent, mtime: SystemTime::UNIX_EPOCH, seq: 0, waiters: Vec::new(), required: false, sheets: false, head: None, cancel: None };
    assert!(r.svc.convert(&job).is_ok(), "an input the job already carries needs no second resolve");
    assert_eq!(r.jobs(), 1);
}

// Written independently of production constants, so routing Office back to the generic cap fails.
const CHECK_OFFICE_LIMITS: &str = r#"
import os, resource, sys
assert resource.getrlimit(resource.RLIMIT_AS) == (8589934592, 8589934592), 'Office must have a finite 8 GiB address-space cap'
assert resource.getrlimit(resource.RLIMIT_CPU) == (resource.RLIM_INFINITY, resource.RLIM_INFINITY), 'Office has no CPU cap: the wall clock bounds it'
os.execv(sys.argv[1], sys.argv[1:])
"#;

// Port the retained office native probe's DOCX, real converter, cache hit and PDF assertions to the backend suite.
#[test]
#[ignore = "native: needs LibreOffice and a working sandbox, run by MINIPC.md"]
fn a_real_docx_converts_in_the_production_jail_and_is_cached() {
    let program = find_program(PROGRAM, &std::env::var("PATH").unwrap_or_default())
        .expect("the native office regression requires LibreOffice");
    assert!(
        !skipped(),
        "the native office regression requires the real sandbox"
    );
    let d = TestDir::new("officepdf-native");
    d.assert_contains(d.path());
    let generated = std::process::Command::new("python3")
        .args(["-B", "tests/fixtures/office/generate.py"])
        .arg(d.path())
        .output()
        .expect("the retained office fixture generator starts");
    assert!(
        generated.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let mut r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let jail: Runner = Box::new(run_boxed_office);
    r.svc = Arc::new(Service::new(
        Store::new(&r.cache),
        Box::new(move || Some(program.clone())),
        Box::new(|program, target, input, out| {
            // Check the kernel's actual inherited limits, then exec the unmodified converter argv.
            let mut checked = vec![
                "/usr/bin/python3".into(),
                "-c".into(),
                CHECK_OFFICE_LIMITS.into(),
            ];
            checked.extend(convert_argv(program, target, input, out));
            checked
        }),
        jail,
        CONVERT_TIMEOUT,
    ));
    let src = d.join("report.docx");
    let original = std::fs::read(&src).unwrap();
    let began = Instant::now();
    assert_eq!(r.ask(1, &src), converting_line(1));
    let pdf = r.svc.store.pdf_path(&Store::key(&src));
    let end =
        r.rx.recv_timeout(CONVERT_TIMEOUT + Duration::from_secs(20))
            .expect("native conversion answers after its deadline and bounded drain");
    let OpMsg::DetachedDone { id, line } = end else {
        panic!("native conversion must finish the detached operation");
    };
    assert_eq!(
        (id, line),
        (1, ready_line(1, &pdf)),
        "native conversion failed"
    );
    assert!(
        std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"),
        "the converter must produce a real PDF"
    );
    assert_eq!(
        std::fs::read(&src).unwrap(),
        original,
        "the read-only input is unchanged"
    );
    assert_eq!(
        r.ask(2, &src),
        ready_line(2, &pdf),
        "the second request is a cache hit"
    );
    assert_eq!(mtime_of(&pdf), mtime_of(&src));
    eprintln!(
        "office native DOCX conversion and cache: {} ms; kernel AS=8589934592 CPU unlimited",
        began.elapsed().as_millis()
    );
}

#[path = "officepdf_matrix_tests.rs"]
mod matrix;

#[path = "officepdf_cache_tests.rs"]
mod cache;
