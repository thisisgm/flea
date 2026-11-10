// The repro runs in a child copy of this test binary, so its forking siblings share a process only with it.
use super::copy_tree;
use crate::backend::testdir::{with_forking_siblings, TestDir};
use std::process::Command;

// The outer test sets this on the child only; without it the inner test returns at once.
const REPRO_FLAG: &str = "FLEA_PLUGIN_COPY_ETXTBSY_REPRO";
// The inner test's full libtest path; the ok line check fails the outer test if the path drifts.
const INNER: &str = "backend::pluginfs::tests::plugin_copies_beside_forking_siblings_inner";
// Bounds the child, so a hang fails the outer test instead of stalling the suite.
const CHILD_LIMIT_SECS: &str = "60";
// Padding after the exit line, so each copy takes milliseconds and a sibling fork lands inside it.
const PADDING_BYTES: usize = 16 << 20;
// Plugin folders copied and run in one child, each removed after its exec.
const ITERATIONS: usize = 20;

#[test]
fn plugin_copies_beside_forking_siblings_exec() {
    let out = Command::new("timeout")
        .arg(CHILD_LIMIT_SECS)
        .arg(std::env::current_exe().expect("the test binary"))
        .args(["--exact", INNER, "--test-threads=1", "--nocapture"])
        .env(REPRO_FLAG, "1")
        .output()
        .expect("the repro child starts");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the repro child failed:\n{stdout}{stderr}"
    );
    assert!(
        stdout.contains(&format!("test {INNER} ... ok")),
        "the inner repro ran and passed:\n{stdout}"
    );
}

#[test]
fn plugin_copies_beside_forking_siblings_inner() {
    if std::env::var_os(REPRO_FLAG).is_none() {
        return;
    }
    let mut body = b"#!/bin/sh\nexit 0\n".to_vec();
    body.resize(PADDING_BYTES, b'#');
    let dir = TestDir::new("pluginfs-forkers");
    let source = dir.dir("source");
    dir.executable("source/run", &body, 0o755);
    with_forking_siblings(|| {
        for n in 0..ITERATIONS {
            let copy = dir.join(&format!("copy-{n}"));
            copy_tree(&source, &copy).expect("the plugin folder copies");
            let ran = Command::new(copy.join("run"))
                .status()
                .expect("the copied run starts");
            assert!(ran.success(), "copy {n} ran");
            std::fs::remove_file(copy.join("run")).expect("the copied run is removed");
            std::fs::remove_dir(&copy).expect("the copy is removed");
        }
    });
}
