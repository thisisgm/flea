// The copy manifest under a failing filesystem: every file-size cap runs in a re-executed child, so no test ever sets a process-wide limit.
use crate::backend::copyfile::{copy_any, Progress};
use crate::backend::testdir::TestDir;
use crate::backend::undo::{copied_partial, Entry, ItemIdentity, Journal};
use crate::backend::copymanifest;
use std::sync::atomic::AtomicBool;

// RLIMIT_FSIZE is process-wide, so each capped test below re-executes itself in a child where the cap harms only it.
const FSIZE_CHILD: &str = "FLEA_FSIZE_CHILD";
fn child_or_spawn(test: &str) -> bool {
    if std::env::var_os(FSIZE_CHILD).is_some() {
        return true;
    }
    let exe = std::env::current_exe().expect("a test binary knows its own path");
    let out = std::process::Command::new(exe).args(["--exact", "--test-threads=1", "--nocapture", test]).env(FSIZE_CHILD, "1").output().expect("the test binary re-executes");
    let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "the fsize child failed: {report}");
    assert!(report.contains("1 passed"), "the fsize child ran no test: {report}");
    false
}

#[repr(C)]
struct Rlimit {
    cur: u64,
    max: u64,
}
#[allow(clashing_extern_declarations)]
extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
    fn setrlimit(resource: i32, rlim: *const Rlimit) -> i32;
    fn getrlimit(resource: i32, rlim: *mut Rlimit) -> i32;
}
const SIGXFSZ: i32 = 25;
const SIG_IGN: usize = 1;
const RLIMIT_FSIZE: i32 = 1;

struct FileSizeCap {
    old_handler: usize,
    old_limit: Rlimit,
}
impl FileSizeCap {
    fn cap(bytes: u64) -> Self {
        let mut old_limit = Rlimit { cur: 0, max: 0 };
        assert_eq!(unsafe { getrlimit(RLIMIT_FSIZE, &mut old_limit) }, 0);
        // Without this the first exceeding write kills the process instead of failing it.
        let old_handler = unsafe { signal(SIGXFSZ, SIG_IGN) };
        let cap = Rlimit { cur: bytes, max: old_limit.max };
        assert_eq!(unsafe { setrlimit(RLIMIT_FSIZE, &cap) }, 0);
        let mut took = Rlimit { cur: 0, max: 0 };
        assert_eq!(unsafe { getrlimit(RLIMIT_FSIZE, &mut took) }, 0);
        // The read proves the cap is really on: a silent no-op here would copy whole.
        assert_eq!(took.cur, bytes);
        Self { old_handler, old_limit }
    }
}
impl Drop for FileSizeCap {
    fn drop(&mut self) {
        unsafe { setrlimit(RLIMIT_FSIZE, &self.old_limit) };
        unsafe { signal(SIGXFSZ, self.old_handler) };
    }
}

#[test]
fn a_file_half_written_when_writes_give_out_is_removed_by_undo() {
    if !child_or_spawn("backend::manifestfsize_tests::a_file_half_written_when_writes_give_out_is_removed_by_undo") {
        return;
    }
    // In place, as a FUSE destination writes a tree; a staged folder goes whole instead (copyfolder_tests).
    let _seams = crate::backend::copystage_tests::in_place();
    let d = TestDir::new("undopartialfsize");
    let src = d.dir("source");
    for i in 0..3 {
        std::fs::write(src.join(format!("f{i}.bin")), "x".repeat(64)).unwrap();
    }
    // Past the 256 KiB copy chunk, so the failure lands mid-file, not at create.
    std::fs::write(src.join("big.bin"), "x".repeat(512 * 1024)).unwrap();
    let partial = d.join("clone");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = Progress {
        cancel: &flag,
        on_bytes: &mut sink,
        tree: None,
        partial: None,
        manifest: copymanifest::writer_for(&src, &partial),
        durability: None,
        for_move: false,
    };
    let cap = FileSizeCap::cap(300 * 1024);
    let outcome = copy_any(&src, &partial, &mut p);
    let finished = p.partial.take();
    let manifest = p.manifest.take().map(|writer| writer.finish().expect("no I/O")).unwrap_or(None);
    drop(p);
    drop(cap);
    outcome.expect_err("the size cap must fail the big file mid-write");
    assert_eq!(finished, Some(partial.clone()));
    let half = partial.join("big.bin").symlink_metadata().expect("the half-written file stays");
    assert!(half.len() < 512 * 1024 && half.len() >= 256 * 1024, "part of it went: {}", half.len());
    d.assert_contains(&partial);
    assert!(partial.is_absolute() && partial.starts_with(d.path()));
    let handle = manifest.expect("the failed copy is manifested");
    let step = copied_partial(&src, &partial, ItemIdentity::inspect(&src).unwrap(), Some(handle)).unwrap();
    let mut j = Journal::new();
    j.push(Entry { op: "copy".to_string(), steps: vec![step] });
    assert_eq!(j.undo().expect("everything the copy created must go"), "copy");
    assert!(!partial.exists(), "the half-written file went with its tree");
    assert_eq!(src.join("big.bin").metadata().unwrap().len(), 512 * 1024, "the source stands whole");
}

#[test]
fn a_manifest_append_failure_is_loud_rather_than_silent_overflow() {
    if !child_or_spawn("backend::manifestfsize_tests::a_manifest_append_failure_is_loud_rather_than_silent_overflow") {
        return;
    }
    let d = TestDir::new("undomanifestloud");
    let src = d.dir("srcl");
    let root = d.dir("rootl");
    let mut writer = copymanifest::writer_for(&src, &root).expect("manifest dir writable");
    std::fs::write(root.join("probe.txt"), "x").unwrap();
    let cap = FileSizeCap::cap(2 * 1024);
    for _ in 0..100 {
        writer.record(&root, &root.symlink_metadata().unwrap());
    }
    let result = writer.finish();
    drop(cap);
    assert!(result.is_err(), "append failure must Err, not silent None");
}

#[test]
fn a_mid_copy_flush_failure_stays_loud() {
    if !child_or_spawn("backend::manifestfsize_tests::a_mid_copy_flush_failure_stays_loud") {
        return;
    }
    let d = TestDir::new("manifestmidfail");
    let src = d.dir("src");
    let root = d.dir("root");
    let meta = d.file("probe.bin", "x").symlink_metadata().unwrap();
    let mut writer = copymanifest::writer_for(&src, &root).expect("manifest dir writable");
    // The 100 KiB cap fails a mid-copy batch; later records set overflow as well, yet finish must still Err on the latch.
    let cap = FileSizeCap::cap(100 * 1024);
    for i in 0..10000 {
        if writer.failed() {
            break;
        }
        writer.record(&root.join(format!("m{i:05}.bin")), &meta);
    }
    assert!(writer.failed(), "a batch failed mid-copy");
    drop(cap);
    for i in 0..100 {
        writer.record(&root.join(format!("n{i:05}.bin")), &meta);
    }
    let result = writer.finish();
    assert!(result.is_err(), "a latched mid-copy failure stays loud, never Ok(None)");
}
