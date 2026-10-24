// Runs the shipping executable on a marked root and verifies worker, cache and generation behavior.
#[test]
fn search_protocol_workers_cache_generations_windows_and_close() {
    let output = std::process::Command::new("python3")
        .args(["-B", "-c", include_str!("search-fb5.py"), env!("CARGO_BIN_EXE_flea")])
        .output().expect("protocol test needs python3");
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "protocol exit {}", output.status);
}
