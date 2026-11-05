use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache::{png_text, Hit, TABLE_KEY};
use std::sync::mpsc::{channel, Receiver};
use std::thread::JoinHandle;

#[path = "thumbs_cache_guard_tests.rs"]
mod cache_guard;

const MISSING: &str = "/definitely/not/here.jpg";
const FIXTURE_MTIME: i64 = 1787790423;

struct TestPool {
    sandbox: TestDir,
    pool: Pool,
    receiver: Option<Receiver<Done>>,
    worker: Option<JoinHandle<()>>,
}

impl TestPool {
    fn new(tag: &str, aliases: Arc<Aliases>, specs: Arc<Thumbnailers>) -> Self {
        let sandbox = TestDir::new(tag);
        sandbox.assert_contains(sandbox.path());
        let pool = Pool::idle();
        let inner = Arc::clone(&pool.inner);
        let tables = Arc::new(Tables {
            aliases,
            specs,
            cache: Cache::at(sandbox.path().to_path_buf()),
            worker: None,
        });
        let (sender, receiver) = channel();
        let worker = std::thread::spawn(move || worker(inner, sender, tables));
        Self {
            sandbox,
            pool,
            receiver: Some(receiver),
            worker: Some(worker),
        }
    }
}

impl Drop for TestPool {
    fn drop(&mut self) {
        self.pool.cancel_all();
        drop(self.receiver.take());
        // A closed receiver ends the real worker; an unsupported MIME wakes an idle one without filesystem work.
        self.pool.submit(Job {
            mime: String::new(),
            ..job(MISSING)
        });
        if self.worker.take().unwrap().join().is_err() && !std::thread::panicking() {
            panic!("thumbnail test worker panicked");
        }
    }
}

/// Build an untracked thumbnail job for a test path.
fn job(path: &str) -> Job {
    Job {
        replies: None,
        path: PathBuf::from(path),
        mtime: 0,
        source_size: None,
        trash_identity: None,
        mime: "image/jpeg".to_string(),
        trace: None,
    }
}

// Returns a pool whose one worker is already inside a ten minute child, so the queue can only change by the caller's own hand.
fn pinned(tag: &str, base: u32) -> TestPool {
    // A duration nothing else on the box shares, so the gate below cannot be satisfied by another test's child or another suite's.
    let seconds = format!("{}.{}", base, std::process::id());
    let body = format!(
        "[Thumbnailer Entry]\nTryExec=/usr/bin/sleep\nExec=/usr/bin/sleep {}\nMimeType=image/jpeg;\n",
        seconds
    );
    let entries = [("pin.thumbnailer".to_string(), body)];
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::from_entries(&entries, &aliases));
    let fixture = TestPool::new(tag, aliases, specs);
    fixture.pool.submit(job("/usr/bin/sleep"));
    for _ in 0..400 {
        if pin_running(&seconds) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(pin_running(&seconds), "the pin child never started");
    assert_eq!(fixture.pool.pending(), 0);
    fixture
}

// Sample input, /proc/<pid>/cmdline for the pin: the program and its one argument, NUL terminated, which is an exact match and not a prefix of bwrap's own argv.
fn pin_running(seconds: &str) -> bool {
    let want = format!("/usr/bin/sleep\0{}\0", seconds);
    let procs = match std::fs::read_dir("/proc") {
        Ok(p) => p,
        Err(_) => return false,
    };
    procs.flatten().any(|p| {
        std::fs::read(p.path().join("cmdline"))
            .map(|c| c == want.as_bytes())
            .unwrap_or(false)
    })
}

#[test]
fn foreign_and_newer_thumbnail_documents_survive_lookup_and_first_write() {
    let d = crate::backend::testdir::TestDir::new("thumb-preserve");
    let cache = Cache::at(d.path().to_path_buf());
    let uri = "file:///source";
    for target in [cache.large_path(uri), cache.fail_path(uri)] {
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        for original in [
            "{\"foreign\":true}",
            "{\"schemaVersion\":999}",
            "unparsable",
        ] {
            std::fs::write(&target, original).unwrap();
            let _ = cache.lookup(Path::new("/source"), 1, "table");
            assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
            let temp = d.join("new.png");
            crate::backend::thumbwrite::write_marker(&temp, uri, 1, Some("table")).unwrap();
            let _ = publish(&temp, uri, 1, target.clone());
            record_failure(&cache, uri, 1, "table");
            assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
        }
    }
}

#[test]
fn an_unwinding_fixture_joins_its_worker_before_removing_the_sandbox() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let pin_seconds = 604;
    let fixture = pinned("thumbs-unwind", pin_seconds);
    let root = fixture.sandbox.path().to_path_buf();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _fixture = fixture;
        panic!("exercise thumbnail fixture unwinding");
    }));
    assert!(result.is_err());
    assert!(!root.exists(), "the joined fixture left its sandbox behind");
    assert!(
        !pin_running(&format!("{}.{}", pin_seconds, std::process::id())),
        "the pin child outlived its sandbox"
    );
}

#[test]
fn a_cancelled_job_never_runs() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let fixture = pinned("thumbs-cancel", 601);
    let pool = &fixture.pool;
    pool.submit(job(MISSING));
    assert_eq!(pool.pending(), 1);
    pool.cancel(&PathBuf::from(MISSING), None);
    assert_eq!(pool.pending(), 0, "cancel left the job in the queue");
    // A cancelled job produces no Done at all, so a short wait must time out.
    let seen = fixture
        .receiver
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_millis(500));
    assert!(seen.is_err(), "a job reported when none should have");
}

#[test]
fn cancel_all_empties_the_queue() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let fixture = pinned("thumbs-cancelall", 602);
    let pool = &fixture.pool;
    for i in 0..8 {
        pool.submit(job(&format!("/definitely/not/here-{}.jpg", i)));
    }
    assert_eq!(pool.pending(), 8);
    pool.cancel_all();
    let left = pool.pending();
    assert_eq!(left, 0);
}

#[test]
fn the_queue_is_bounded_and_drops_the_oldest_rather_than_growing() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let fixture = pinned("thumbs-bounded", 603);
    let pool = &fixture.pool;
    for i in 0..(MAX_QUEUE * 2) {
        pool.submit(job(&format!("/definitely/not/here-{}.jpg", i)));
    }
    assert_eq!(pool.pending(), MAX_QUEUE);
    // The oldest submissions are the dropped ones, so cancelling one of those changes nothing.
    pool.cancel(&PathBuf::from("/definitely/not/here-0.jpg"), None);
    assert_eq!(pool.pending(), MAX_QUEUE);
    pool.cancel(
        &PathBuf::from(format!("/definitely/not/here-{}.jpg", MAX_QUEUE * 2 - 1)),
        None,
    );
    let left = pool.pending();
    assert_eq!(left, MAX_QUEUE - 1);
}

#[test]
fn a_timeout_records_locally_but_never_on_network_or_usb() {
    assert!(
        records_marker(&Ran::Failed, ""),
        "a decoder verdict records"
    );
    assert!(
        records_marker(&Ran::Failed, "network"),
        "a decoder verdict records anywhere"
    );
    assert!(
        records_marker(&Ran::TimedOut, ""),
        "a local timeout still records"
    );
    assert!(
        !records_marker(&Ran::TimedOut, "network"),
        "a network timeout never stays broken"
    );
    assert!(
        !records_marker(&Ran::TimedOut, "usb"),
        "a USB timeout never stays broken"
    );
    assert!(
        !records_marker(&Ran::NotStarted, ""),
        "a machine fault never records"
    );
}

#[test]
fn a_missing_input_reports_failed_rather_than_hanging() {
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::load(&aliases));
    let fixture = TestPool::new("thumbs-missing", aliases, specs);
    fixture.pool.submit(job(MISSING));
    let done = fixture
        .receiver
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .expect("no result");
    assert_eq!(done.path, PathBuf::from(MISSING));
    assert!(matches!(done.result, Outcome::Failed));
    // A vanished input is not a broken file; a box with no host thumbnailer never creates large/.
    let recorded = fixture.sandbox.join("fail").exists();
    let left = match std::fs::read_dir(fixture.sandbox.join("large")) {
        Ok(entries) => entries.count(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => panic!("large/ could not be read: {}", e),
    };
    assert!(!recorded, "a vanished input was recorded in fail/");
    assert_eq!(left, 0, "a temp file survived a failed job");
}

/// Exercise a real decoder and check the cache stamp on its output.
#[test]
fn a_real_file_round_trips_to_a_stamped_cache_entry() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::load(&aliases));
    // This one test needs a real host thumbnailer; a builder without one is told, not failed.
    if specs.for_mime("image/png", &aliases).is_none() {
        std::io::Write::write_all(&mut std::io::stderr(), b"SKIP backend::thumbs::tests::a_real_file_round_trips_to_a_stamped_cache_entry: no thumbnailer declares image/png here\n").ok();
        return;
    }
    let fixture = TestPool::new("thumbs-roundtrip", aliases, specs);
    let src = fixture.sandbox.join("in.png");
    // A real thumbnailer needs a real image, and the fail marker writer already makes the smallest valid one.
    write_marker(&src, "file:///input", 0, None).unwrap();
    fixture.pool.submit(Job {
        replies: None,
        path: src.clone(),
        mtime: FIXTURE_MTIME,
        source_size: None,
        trash_identity: None,
        mime: "image/png".to_string(),
        trace: None,
    });
    let done = fixture
        .receiver
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .expect("no result");
    let out = match done.result {
        Outcome::Ready(p) => p,
        Outcome::Failed | Outcome::Cancelled => panic!("a real thumbnailer failed on a valid png"),
    };
    let bytes = std::fs::read(&out).unwrap();
    let published = std::fs::read_dir(fixture.sandbox.join("large"))
        .unwrap()
        .count();
    let want = Cache::at(fixture.sandbox.path().to_path_buf()).large_path(&uri_for(&src));
    assert_eq!(out, want);
    assert_eq!(png_text(&bytes, "Thumb::URI"), Some(uri_for(&src)));
    assert_eq!(
        png_text(&bytes, "Thumb::MTime"),
        Some(FIXTURE_MTIME.to_string())
    );
    assert_eq!(published, 1, "a temp file survived the publish");
}

#[test]
fn a_failure_after_a_table_miss_rewrites_the_marker_under_the_current_table() {
    let d = TestDir::new("thumbs-rewrite");
    let cache = Cache::at(d.path().to_path_buf());
    let path = Path::new(MISSING);
    let uri = uri_for(path);
    record_failure(&cache, &uri, FIXTURE_MTIME, "table-a");
    assert!(matches!(
        cache.lookup(path, FIXTURE_MTIME, "table-a"),
        Hit::Failed
    ));
    assert!(matches!(
        cache.lookup(path, FIXTURE_MTIME, "table-b"),
        Hit::Miss
    ));
    record_failure(&cache, &uri, FIXTURE_MTIME, "table-b");
    assert!(matches!(
        cache.lookup(path, FIXTURE_MTIME, "table-b"),
        Hit::Failed
    ));
    assert_eq!(
        png_text(&std::fs::read(cache.fail_path(&uri)).unwrap(), TABLE_KEY),
        Some("table-b".to_string())
    );
}

/// Verify that a decoder failure is stamped with the active provider table.
#[test]
fn a_decoder_failure_records_its_marker_under_the_loaded_table() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let body = "[Thumbnailer Entry]\nTryExec=/usr/bin/false\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n";
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::from_entries(
        &[("fail.thumbnailer".to_string(), body.to_string())],
        &aliases,
    ));
    let fixture = TestPool::new("thumbs-failtable", aliases, Arc::clone(&specs));
    let src = fixture.sandbox.file("in.jpg", "bytes the decoder refuses");
    fixture.pool.submit(Job {
        replies: None,
        path: src.clone(),
        mtime: FIXTURE_MTIME,
        source_size: None,
        trash_identity: None,
        mime: "image/jpeg".to_string(),
        trace: None,
    });
    let done = fixture
        .receiver
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .expect("no result");
    assert!(matches!(done.result, Outcome::Failed));
    let cache = Cache::at(fixture.sandbox.path().to_path_buf());
    assert!(
        matches!(
            cache.lookup(&src, FIXTURE_MTIME, specs.fingerprint()),
            Hit::Failed
        ),
        "the marker names the loaded table"
    );
    assert!(matches!(
        cache.lookup(&src, FIXTURE_MTIME, "another table"),
        Hit::Miss
    ));
}
