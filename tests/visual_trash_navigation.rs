// The visual lane must acknowledge cursor movement before activating its semantic Trash row.
#[test]
fn visual_trash_navigation_handles_asynchronous_rail_readers() {
    let output = std::process::Command::new("python3")
        .args(["-I", "tests/visual/test_trash_navigation.py"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("visual Trash navigation test needs python3");
    assert!(
        output.status.success(),
        "visual Trash navigation failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
