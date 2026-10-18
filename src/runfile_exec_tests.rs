// A sibling fork that inherits a written fd keeps it until its exec, so the next exec of that file fails with ETXTBSY.
use crate::backend::testdir::{with_forking_siblings, TestDir};

#[test]
fn executables_written_beside_forking_siblings_exec() {
    // Padding after the exit widens the write to milliseconds, so a sibling fork lands inside the window.
    let mut body = b"#!/bin/sh\nexit 0\n".to_vec();
    body.resize(16 << 20, b'#');
    let dir = TestDir::new("runfile-forkers");
    with_forking_siblings(|| {
        for n in 0..20 {
            let tool = super::write(&dir, "tool.sh", &body, 0o755);
            let ran = std::process::Command::new(&tool).status().expect("the tool starts");
            std::fs::remove_file(&tool).expect("the tool is removed");
            assert!(ran.success(), "tool {n} ran");
        }
    });
}
