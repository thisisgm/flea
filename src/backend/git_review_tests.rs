use super::*;
use crate::backend::gitfixture::Fixture;
use std::time::Instant;

const CHILD_ADDRESS_SPACE_BYTES: usize = 256 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;

fn assert_deep_snapshot(snapshot: &Snapshot, paths: &[(std::path::PathBuf, char)]) {
    assert_eq!(snapshot.changes.len(), paths.len());
    for (path, status) in paths {
        let change = snapshot
            .changes
            .iter()
            .find(|change| change.path.as_os_str() == path.as_os_str())
            .expect("the byte-exact indexed path must be present");
        assert_eq!(change.status, *status);
        assert_eq!(snapshot.mark(path), (Some(*status), false));
    }
}

// Sample input: "REVIEW_MEMORY VmHWM:\t123 kB\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured\n".
fn assert_snapshot_child(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "the bounded Git output must not abort the backend parser"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("REVIEW_MEMORY VmHWM:")),
        "{stdout}"
    );
    assert!(
        stdout.contains("test result: ok. 1 passed; 0 failed"),
        "{stdout}"
    );
}

#[test]
fn git_deep_snapshot_assertions_reject_wrong_counts_paths_and_marks() {
    let paths = [(std::path::PathBuf::from("a/a/leaf"), 'D')];
    for bytes in [
        b"# branch.head main\0".as_slice(),
        b"# branch.head main\x001 .D N... 1 1 1 a b wrong\0",
        b"# branch.head main\0? a/a/leaf\0",
    ] {
        let snapshot = super::super::gitstatus::parse(bytes).unwrap();
        assert!(std::panic::catch_unwind(|| assert_deep_snapshot(&snapshot, &paths)).is_err());
    }
}

#[test]
fn git_snapshot_child_assertions_reject_missing_execution_witnesses() {
    for filter in [
        "backend::gitreq::review_tests::missing_after_rename",
        "backend::gitreq::review_tests::review_git_status_large_snapshot_child",
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", filter, "--nocapture"])
            .env_remove("FLEA_REVIEW_STATUS_INPUT")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(std::panic::catch_unwind(|| assert_snapshot_child(&output)).is_err());
    }
}

#[test]
fn review_git_status_with_deep_index_path() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("empty", b"");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let mut paths = Vec::new();
    for (depth, expected) in [(1024, 'D'), (4096, 'A'), (8192, 'A')] {
        let path = format!("{}leaf", "a/".repeat(depth));
        d.git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "100644",
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
            &path,
        ]);
        paths.push((std::path::PathBuf::from(path), expected));
        let start = Instant::now();
        let bytes = gitprobe::capture(&gitprobe::argv(&repo), gitprobe::DEADLINE).unwrap();
        let capture_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let snapshot = super::super::gitstatus::parse(&bytes).unwrap();
        assert_deep_snapshot(&snapshot, &paths);
        println!("REVIEW_DEEP depth={depth} bytes={} changes={} capture_ms={capture_ms:.3} parse_ms={:.3} {}", bytes.len(), snapshot.changes.len(), start.elapsed().as_secs_f64() * 1000.0, std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|line| line.starts_with("VmHWM:")).unwrap());
    }
}

#[test]
fn review_git_status_large_snapshot_child() {
    let Some(path) = std::env::var_os("FLEA_REVIEW_STATUS_INPUT") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let snapshot = super::super::gitstatus::parse(&bytes).unwrap();
    println!("{}", snapshot.label());
    println!(
        "REVIEW_MEMORY {}",
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|line| line.starts_with("VmHWM:"))
            .unwrap()
    );
}

#[test]
fn review_git_status_deep_index_exhausts_parent_memory() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("empty", b"");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let path = format!("{}leaf", "a/".repeat(16384));
    d.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "100644",
        "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
        &path,
    ]);
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let bytes = gitprobe::capture(&gitprobe::argv(&repo), gitprobe::DEADLINE).unwrap();
    println!("REVIEW_OOM valid_git_stdout_bytes={}", bytes.len());
    d.write("status-output", &bytes);
    let output = std::process::Command::new("/usr/bin/prlimit")
        .arg(format!("--as={CHILD_ADDRESS_SPACE_BYTES}"))
        .arg("--core=0")
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "backend::gitreq::review_tests::review_git_status_large_snapshot_child",
            "--nocapture",
        ])
        .env("FLEA_REVIEW_STATUS_INPUT", d.path("status-output"))
        .output()
        .unwrap();
    println!(
        "REVIEW_OOM status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_snapshot_child(&output);
}

#[test]
fn review_git_status_many_normal_length_paths_exhaust_parent_memory() {
    use std::process::{Command, Stdio};
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("empty", b"");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let mut input = String::new();
    for index in 0..512 {
        let path = format!("root-{index:04}/{}leaf", "a/".repeat(1024));
        assert!(path.len() < MAX_PATH_BYTES);
        input.push_str(&format!(
            "100644 e69de29bb2d1d6434b8b29ae775ad8c2e48c5391\t{path}\0"
        ));
    }
    d.write(".git/review-index-input", input.as_bytes());
    let output = Command::new("/usr/bin/git")
        .current_dir(&d.0)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "update-index",
            "-z",
            "--index-info",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::from(
            std::fs::File::open(d.path(".git/review-index-input")).unwrap(),
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let start = Instant::now();
    let bytes = gitprobe::capture(&gitprobe::argv(&repo), gitprobe::DEADLINE).unwrap();
    println!(
        "REVIEW_MANY valid_git_stdout_bytes={} capture_ms={:.3} maximum_path_bytes=2062 paths=512",
        bytes.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    let snapshot = gitprobe::status(&repo).unwrap();
    assert_eq!(snapshot.changes.len(), 512);
    for index in 0..512 {
        assert_eq!(
            snapshot.mark(std::path::Path::new(&format!("root-{index:04}"))),
            (None, true)
        );
    }
    for (name, bytes) in [
        (
            "small-output",
            b"# branch.head main\0? dir/file\0".as_slice(),
        ),
        ("large-output", bytes.as_slice()),
    ] {
        d.write(name, bytes);
        let output = Command::new("/usr/bin/prlimit")
            .arg(format!("--as={CHILD_ADDRESS_SPACE_BYTES}"))
            .arg("--core=0")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backend::gitreq::review_tests::review_git_status_large_snapshot_child",
                "--nocapture",
            ])
            .env("FLEA_REVIEW_STATUS_INPUT", d.path(name))
            .output()
            .unwrap();
        println!(
            "REVIEW_MANY {name} status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_snapshot_child(&output);
    }
}

#[test]
fn review_git_status_staged_removal_with_retained_worktree_file() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("retained.txt", b"keep this file locally\n");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let clean_repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let clean = gitprobe::status(&clean_repo).unwrap();
    assert_eq!(clean.changes.len(), 0);
    println!("REVIEW_RETAINED clean_control={}", clean.label());
    d.git(&["rm", "--cached", "--", "retained.txt"]);
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let bytes = gitprobe::capture(&gitprobe::argv(&repo), gitprobe::DEADLINE).unwrap();
    println!(
        "REVIEW_RETAINED porcelain={:?}",
        String::from_utf8_lossy(&bytes)
    );
    let reply = Session::default().answer(&format!(
        "{{\"c\":\"gitstatus\",\"path\":{},\"enabled\":true,\"id\":21}}",
        quote(&d.0.to_string_lossy())
    ));
    println!("REVIEW_RETAINED reply={reply}");
    assert!(
        crate::json::field_bool(&reply, "ok"),
        "valid Git state must produce a usable snapshot: {reply}"
    );
    assert_eq!(crate::json::field_usize(&reply, "changed"), Some(1));
    assert_eq!(
        crate::json::field_str(&reply, "label").as_deref(),
        Some("main · 1 changed")
    );
    let snapshot = gitprobe::status(&repo).unwrap();
    assert_eq!(
        snapshot.mark(std::path::Path::new("retained.txt")),
        (Some('?'), false)
    );
}
