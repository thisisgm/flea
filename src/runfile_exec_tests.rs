// The repro runs in a child copy of this test binary, so its forking siblings share a process only with it.
use crate::backend::testdir::{with_forking_siblings, TestDir};
use std::process::Command;

// The outer test sets this on the child only; without it the inner test returns at once, so a normal run forks nothing.
const REPRO_FLAG: &str = "FLEA_ETXTBSY_REPRO";
// The inner test's full libtest path; --exact selects it, and the ok line check fails the outer test if the path drifts.
const INNER: &str = "runfile::tests::exec::executables_written_beside_forking_siblings_inner";
// Bounds the child, so a hang fails the outer test instead of stalling the suite.
const CHILD_LIMIT_SECS: &str = "60";
// Padding after the exit line, so each write takes milliseconds and a sibling fork lands inside it.
const PADDING_BYTES: usize = 16 << 20;
// Fresh executables written and run in one child, each removed after its exec.
const ITERATIONS: usize = 20;

#[test]
fn executables_written_beside_forking_siblings_exec() {
    let out = Command::new("timeout")
        .arg(CHILD_LIMIT_SECS)
        .arg(std::env::current_exe().expect("the test binary"))
        .args(["--exact", INNER, "--test-threads=1", "--nocapture"])
        .env(REPRO_FLAG, "1")
        .output()
        .expect("the repro child starts");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the repro child failed:\n{stdout}{stderr}");
    assert!(stdout.contains(&format!("test {INNER} ... ok")), "the inner repro ran and passed:\n{stdout}");
}

#[test]
fn executables_written_beside_forking_siblings_inner() {
    if std::env::var_os(REPRO_FLAG).is_none() {
        return;
    }
    let mut body = b"#!/bin/sh\nexit 0\n".to_vec();
    body.resize(PADDING_BYTES, b'#');
    let dir = TestDir::new("runfile-forkers");
    with_forking_siblings(|| {
        for n in 0..ITERATIONS {
            let tool = super::write(&dir, "tool.sh", &body, 0o755);
            let ran = Command::new(&tool).status().expect("the tool starts");
            std::fs::remove_file(&tool).expect("the tool is removed");
            assert!(ran.success(), "tool {n} ran");
        }
    });
}
