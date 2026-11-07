use super::*;
use crate::backend::archivework::{failure_message, spawn_stderr_reader};
use crate::backend::fifotest::{mkfifo, within};
use crate::backend::opsreq::op_err;
use crate::backend::sandboxprobe::skipped;
use crate::backend::testdir::TestDir;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{channel, Receiver};

// The stand-ins for soffice: $1 is the input, $2 the writable output folder.
const MAKE_PDF: &str = r#"printf '%s' '%PDF-1.4 stub' > "$2/out.pdf""#;
// A builtin read, so a kill leaves no child holding the fifo.
const GATED: &str = r#"read -r _ < "$1"; printf '%s' '%PDF-1.4 gated' > "$2/out.pdf""#;
const BROKEN: &str = "echo boom >&2; exit 3";
const GATED_BROKEN: &str = r#"read -r _ < "$1"; echo boom >&2; exit 3"#;
const HANGS: &str = "exec sleep 30";
const BOUND: Duration = Duration::from_secs(20);

struct Rig {
    svc: Arc<Service>,
    jobs: Arc<AtomicUsize>,
    claims: AtomicUsize,
    tx: Sender<OpMsg>,
    rx: Receiver<OpMsg>,
    cache: PathBuf,
}

// The runner of the production shape without the jail: spawn, poll the stop flag, kill on it, say the last stderr line on a failure.
fn unjailed(argv: Vec<String>, _input: &Path, _work: &mut Work, stop: &AtomicBool) -> Result<(), FleaError> {
    let mut child = std::process::Command::new(&argv[0]).args(&argv[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped()).spawn().expect("the stub converter starts");
    let reader = spawn_stderr_reader(child.stderr.take());
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(op_err("office", "", "cancelled"));
        }
        if let Some(status) = child.try_wait().unwrap() {
            return if status.success() { Ok(()) } else { Err(op_err("office", "", &failure_message("office", &status, &reader.join().unwrap_or_default()))) };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn rig(d: &TestDir, script: &'static str, timeout: Duration) -> Rig {
    rig_in(d, script, timeout, Box::new(unjailed))
}

fn rig_in(d: &TestDir, script: &'static str, timeout: Duration, runner: Runner) -> Rig {
    let cache = d.dir("cache");
    let jobs = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&jobs);
    let build: Build = Box::new(move |_, _, input, out| {
        seen.fetch_add(1, Ordering::SeqCst);
        ["/usr/bin/sh", "-c", script, "sh", &input.to_string_lossy(), &out.to_string_lossy()].iter().map(|s| s.to_string()).collect()
    });
    let resolve: Resolve = Box::new(|| Some("/usr/bin/sh".to_string()));
    let (tx, rx) = channel();
    Rig { svc: Arc::new(Service::new(Store::new(&cache), resolve, build, runner, timeout)), jobs, claims: AtomicUsize::new(0), tx, rx, cache }
}

impl Rig {
    // The first line the request leaves: converting on a miss, otherwise the terminal line that frees its registry id.
    fn ask(&self, wire: usize, path: &Path) -> String {
        let reg = self.claims.fetch_add(1, Ordering::SeqCst) + 1;
        self.svc.request(wire, reg, self.svc.seq.fetch_add(1, Ordering::Relaxed), path, false, None, &self.tx);
        match self.rx.recv_timeout(BOUND).expect("a request always answers") {
            OpMsg::Meta { line } => line,
            OpMsg::DetachedDone { id, line } => {
                assert_eq!(id, reg, "an immediate answer frees its own registry id");
                line
            }
            _ => panic!("a request answers with Meta or DetachedDone"),
        }
    }

    // The end line a job leaves, with the registry id it frees.
    fn end(&self) -> (usize, String) {
        match self.rx.recv_timeout(BOUND).expect("a job always ends") {
            OpMsg::DetachedDone { id, line } => (id, line),
            _ => panic!("a job ends with DetachedDone"),
        }
    }

    fn jobs(&self) -> usize {
        self.jobs.load(Ordering::SeqCst)
    }

    fn fail_markers(&self) -> usize {
        std::fs::read_dir(self.cache.join(CACHE_LEAF).join(FAIL_LEAF)).map(|r| r.count()).unwrap_or(0)
    }
}

fn mtime_of(p: &Path) -> SystemTime {
    std::fs::metadata(p).unwrap().modified().unwrap()
}

#[test]
fn a_program_counts_only_when_it_resolves_under_usr() {
    let d = TestDir::new("officepdf-find");
    let local = d.file("soffice", "#!/bin/sh\n");
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(find_program("soffice", &d.path().to_string_lossy()), None, "a user directory program is refused");
    std::os::unix::fs::symlink("/usr/bin/sh", d.join("viausr")).unwrap();
    assert_eq!(find_program("viausr", &d.path().to_string_lossy()), Some(d.join("viausr").to_string_lossy().into_owned()));
    std::os::unix::fs::symlink(&local, d.join("outward")).unwrap();
    assert_eq!(find_program("outward", &d.path().to_string_lossy()), None, "a link that leaves /usr is refused");
    assert_eq!(find_program("sh", &format!("{}::/usr/bin", d.path().display())).as_deref(), Some("/usr/bin/sh"));
}

#[test]
fn the_argv_is_the_headless_convert_and_nothing_more() {
    let got = argv("/usr/bin/soffice", Path::new("/in/a.docx"), Path::new("/out"));
    let want = "/usr/bin/soffice --headless --norestore --nolockcheck -env:UserInstallation=file:///tmp/flea-office-profile --convert-to pdf --outdir /out /in/a.docx";
    assert_eq!(got.join(" "), want);
}

#[test]
fn the_reply_lines_name_the_state() {
    assert_eq!(ready_line(4, Path::new("/c/x.pdf")), r#"{"t":"officepdf","id":4,"state":"ready","path":"/c/x.pdf"}"#);
    assert_eq!(converting_line(4), r#"{"t":"officepdf","id":4,"state":"converting"}"#);
    assert_eq!(failed_line(4, "boom", false), r#"{"t":"officepdf","id":4,"state":"failed","err":"boom"}"#);
    assert_eq!(failed_line(4, "no", true), r#"{"t":"officepdf","id":4,"state":"failed","err":"no","unavailable":true}"#);
}

#[test]
fn a_hit_answers_ready_with_no_job() {
    let d = TestDir::new("officepdf-hit");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let pdf = r.svc.store.pdf_path(&Store::key(&src));
    Store::publish(&pdf, mtime_of(&src), &mut "cached".as_bytes()).unwrap();
    let line = r.ask(1, &src);
    assert_eq!(line, ready_line(1, &pdf));
    assert_eq!((r.jobs(), r.rx.try_recv().is_err()), (0, true), "a hit starts no job and leaves one line");
}

#[test]
fn a_miss_converts_once_and_the_second_request_runs_nothing() {
    let d = TestDir::new("officepdf-miss");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    assert_eq!(r.ask(1, &src), converting_line(1));
    let (reg, line) = r.end();
    let pdf = r.svc.store.pdf_path(&Store::key(&src));
    assert_eq!((reg, line), (1, ready_line(1, &pdf)));
    assert_eq!(std::fs::read(&pdf).unwrap(), b"%PDF-1.4 stub");
    assert_eq!(mtime_of(&pdf), mtime_of(&src), "the PDF carries the source's mtime");
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&pdf), mode(pdf.parent().unwrap())), (0o600, 0o700));
    assert_eq!(r.ask(2, &src), ready_line(2, &pdf));
    assert_eq!(r.jobs(), 1, "the second request ran nothing");
    let leftovers = std::fs::read_dir(pdf.parent().unwrap()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(".flea-work-")).count();
    assert_eq!(leftovers, 0, "the work folder is gone");
}

#[test]
fn a_changed_mtime_converts_again() {
    let d = TestDir::new("officepdf-mtime");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    r.ask(1, &src);
    r.end();
    let newer = mtime_of(&src) + Duration::from_secs(60);
    std::fs::OpenOptions::new().write(true).open(&src).unwrap().set_modified(newer).unwrap();
    assert_eq!(r.ask(2, &src), converting_line(2), "a changed file is a miss");
    r.end();
    assert_eq!(r.jobs(), 2);
    assert_eq!(mtime_of(&r.svc.store.pdf_path(&Store::key(&src))), newer);
}

#[test]
fn a_timeout_records_no_failure_and_the_next_request_converts_again() {
    let d = TestDir::new("officepdf-timeout");
    let r = rig(&d, HANGS, Duration::from_secs(1));
    let src = d.file("a.docx", "doc");
    r.ask(1, &src);
    let line = r.end().1;
    assert!(line.contains(r#""state":"failed""#) && line.contains("longer than 1 seconds"), "{line}");
    assert_eq!(r.fail_markers(), 0, "a timeout may succeed later, so it leaves no marker");
    assert_eq!(r.ask(2, &src), converting_line(2));
    assert_eq!((r.end().0, r.jobs()), (2, 2), "the next request converts again");
}

#[test]
fn a_non_zero_exit_records_one_failure_with_the_tools_last_line() {
    let d = TestDir::new("officepdf-exit");
    let r = rig(&d, BROKEN, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    r.ask(1, &src);
    assert_eq!(r.end().1, failed_line(1, "boom", false));
    assert_eq!(r.ask(2, &src), failed_line(2, "boom", false));
    assert_eq!((r.jobs(), r.fail_markers()), (1, 1));
}

#[test]
fn a_changed_file_is_tried_again_after_a_failure() {
    let d = TestDir::new("officepdf-retry");
    let r = rig(&d, BROKEN, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    r.ask(1, &src);
    r.end();
    std::fs::OpenOptions::new().write(true).open(&src).unwrap().set_modified(mtime_of(&src) + Duration::from_secs(60)).unwrap();
    assert_eq!(r.ask(2, &src), converting_line(2));
    r.end();
    assert_eq!(r.jobs(), 2);
}

#[test]
fn a_queued_request_is_replaced_while_the_running_one_completes() {
    let d = TestDir::new("officepdf-queue");
    let r = rig(&d, GATED, CONVERT_TIMEOUT);
    let gate = d.join("a.docx");
    mkfifo(&gate);
    let (b, c) = (d.file("b.docx", "b"), d.file("c.docx", "c"));
    assert_eq!((r.ask(1, &gate), r.ask(2, &b), r.ask(3, &c)), (converting_line(1), converting_line(2), converting_line(3)));
    let (reg, line) = r.end();
    assert_eq!(reg, 2, "the replaced request ends first");
    assert!(line.contains(r#""state":"failed""#) && line.contains("replaced"), "{line}");
    // The cursor moved on twice and the running conversion still finishes into the cache.
    within("the gate opens once the converter reads it", move || drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap()));
    let mut ends = vec![r.end(), r.end()];
    ends.sort();
    assert_eq!(ends.iter().map(|e| e.0).collect::<Vec<_>>(), vec![1, 3]);
    assert!(ends.iter().all(|e| e.1.contains(r#""state":"ready""#)), "{ends:?}");
    assert_eq!(r.jobs(), 2, "the replaced request never ran");
}

#[test]
fn the_same_file_asked_twice_converts_once() {
    let d = TestDir::new("officepdf-share");
    let r = rig(&d, GATED, CONVERT_TIMEOUT);
    let gate = d.join("a.docx");
    mkfifo(&gate);
    r.ask(1, &gate);
    assert_eq!(r.ask(2, &gate), converting_line(2));
    within("the gate opens", move || drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap()));
    let mut regs = vec![r.end().0, r.end().0];
    regs.sort();
    assert_eq!((regs, r.jobs()), (vec![1, 2], 1));
}

#[test]
fn closing_the_window_cancels_the_running_and_the_queued_job_without_a_failure_mark() {
    let d = TestDir::new("officepdf-quit");
    let r = rig(&d, GATED, CONVERT_TIMEOUT);
    let gate = d.join("a.docx");
    mkfifo(&gate);
    let b = d.file("b.docx", "b");
    r.ask(1, &gate);
    r.ask(2, &b);
    while r.jobs() == 0 {
        std::thread::sleep(Duration::from_millis(10));
    }
    r.svc.quit.store(true, Ordering::Relaxed);
    let ends = [r.end(), r.end()];
    assert!(ends.iter().all(|e| e.1.contains("cancelled")), "{ends:?}");
    assert_eq!((r.jobs(), r.fail_markers()), (1, 0), "the queued job never ran and a cancel is not recorded");
}

#[test]
fn a_convert_that_finds_a_hit_runs_nothing() {
    let d = TestDir::new("officepdf-late-hit");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let key = Store::key(&src);
    let pdf = r.svc.store.pdf_path(&key);
    Store::publish(&pdf, mtime_of(&src), &mut "other window".as_bytes()).unwrap();
    let job = Job { key, program: "/usr/bin/sh".into(), input: src.clone(), mtime: mtime_of(&src), seq: 0, waiters: Vec::new(), required: false, sheets: false, head: None, cancel: None };
    assert_eq!(r.svc.convert(&job), Ok(Done::Pdf(pdf)));
    assert_eq!(r.jobs(), 0);
}

#[test]
fn without_soffice_no_request_is_served() {
    let d = TestDir::new("officepdf-absent");
    let mut r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let none: Resolve = Box::new(|| None);
    let build: Build = Box::new(|_, _, _, _| panic!("no job may start"));
    r.svc = Arc::new(Service::new(Store::new(&r.cache), none, build, Box::new(unjailed), CONVERT_TIMEOUT));
    let src = d.file("a.docx", "doc");
    let line = r.ask(1, &src);
    assert_eq!(line, failed_line(1, "LibreOffice is not installed on this box.", true));
    assert!(r.svc.locked().running.is_none() && r.svc.locked().queued.is_none());
}

#[test]
fn a_request_on_a_stuck_mount_answers_the_refusal_and_converts_nothing() {
    let d = TestDir::new("officepdf-stuck");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let mount = iomount::mount_key(&src, &iomount::mount_body());
    let _stuck = iomount::test_hold_stuck(mount.clone());
    let line = r.ask(1, &src);
    assert_eq!(line, failed_line(1, &format!("{} is not responding.", mount.display()), false));
    assert_eq!((r.jobs(), r.fail_markers()), (0, 0), "a stuck mount starts no job and records nothing");
}

#[test]
fn start_returns_before_the_worker_has_looked_at_the_file() {
    let d = TestDir::new("officepdf-offloop");
    let (mut r, (open_tx, open_rx)) = (rig(&d, MAKE_PDF, CONVERT_TIMEOUT), channel::<()>());
    let gate = Mutex::new(open_rx);
    Arc::get_mut(&mut r.svc).unwrap().resolve = Box::new(move || {
        let _ = gate.lock().unwrap().recv_timeout(Duration::from_secs(3));
        Some("/usr/bin/sh".to_string())
    });
    let src = d.file("a.docx", "doc");
    r.svc.start(1, 1, src.clone(), false, None, r.tx.clone());
    assert!(r.rx.try_recv().is_err(), "start returned while the worker still waited, so the caller's thread did no path work");
    open_tx.send(()).unwrap();
    assert!(matches!(r.rx.recv_timeout(BOUND), Ok(OpMsg::Meta { line }) if line == converting_line(1)));
    assert_eq!(r.end(), (1, ready_line(1, &r.svc.store.pdf_path(&Store::key(&src)))));
}

#[test]
fn a_queued_job_behind_a_failed_one_answers_failed_without_running_the_converter_again() {
    let d = TestDir::new("officepdf-shared-failure");
    let r = rig(&d, GATED_BROKEN, CONVERT_TIMEOUT);
    let gate = d.join("a.docx");
    mkfifo(&gate);
    assert_eq!((r.ask(1, &gate), r.ask(2, &gate)), (converting_line(1), converting_line(2)));
    within("the gate opens", move || drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap()));
    let ends = [r.end(), r.end()];
    assert!(ends.iter().all(|e| e.1.contains(r#""state":"failed""#) && e.1.contains("boom")), "{ends:?}");
    assert_eq!((r.jobs(), r.fail_markers()), (1, 1), "the converter ran once for both requests");
}

#[test]
fn a_local_publish_error_records_no_failure_and_the_next_request_converts() {
    let d = TestDir::new("officepdf-local-error");
    let r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    let src = d.file("a.docx", "doc");
    let pdf = r.svc.store.pdf_path(&Store::key(&src));
    std::fs::create_dir_all(&pdf).unwrap();
    assert_eq!(r.ask(1, &src), converting_line(1));
    assert!(r.end().1.contains(r#""state":"failed""#));
    assert_eq!(r.fail_markers(), 0, "a local error is not remembered for the file");
    std::fs::remove_dir(&pdf).unwrap();
    assert_eq!(r.ask(2, &src), converting_line(2));
    assert_eq!(r.end().1, ready_line(2, &pdf));
    assert_eq!(r.jobs(), 1, "the refused path cost no conversion, and the next request converted");
}

#[test]
fn the_cache_holds_the_figure_stores_bound_and_drops_the_oldest_conversion() {
    let d = TestDir::new("officepdf-prune");
    let mut r = rig(&d, MAKE_PDF, CONVERT_TIMEOUT);
    Arc::get_mut(&mut r.svc).unwrap().store.entries = 2;
    let files: Vec<PathBuf> = ["a", "b", "c"].iter().map(|n| d.file(&format!("{n}.docx"), n)).collect();
    for (i, f) in files.iter().enumerate() {
        r.ask(i + 1, f);
        r.end();
        std::thread::sleep(Duration::from_millis(30));
    }
    let kept: Vec<bool> = files.iter().map(|f| r.svc.store.pdf_path(&Store::key(f)).exists()).collect();
    assert_eq!(kept, [false, true, true], "the first conversion went, the newest stays");
}
#[path = "officepdf_order_tests.rs"] mod order;
#[path = "officepdf_kind_tests.rs"] mod kinds;
#[path = "officepdf_cancel_tests.rs"] mod cancel;
#[path = "officepdf_jail_tests.rs"] mod jail_caps;
#[path = "officepdf_limits_tests.rs"] mod limits;
#[path = "officepdf_preserve_tests.rs"] mod preserve;
#[path = "officesheets_tests.rs"] mod sheets;
#[path = "officehead_tests.rs"] mod head_cut;
