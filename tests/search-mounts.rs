// Checks requested-row limits and incomplete-scope status through the shipping backend.
#[test]
fn search_mount_scope_removal_never_publishes_stale_complete_rows() {
    let output = std::process::Command::new("python3")
        .args(["-B", "-c", include_str!("search-mounts.py"), env!("CARGO_BIN_EXE_flea")])
        .output().expect("protocol test needs python3");
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "protocol exit {}", output.status);
}
