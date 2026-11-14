// The visual lane's shell contracts run without a compositor or a display server.
use std::process::Command;

#[test]
fn visual_lane_wait_and_navigation_contracts() {
    let output = Command::new("python3")
        .args([
            "-I",
            "-B",
            "-m",
            "unittest",
            "discover",
            "-s",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/visual"),
            "-p",
            "test_*.py",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
