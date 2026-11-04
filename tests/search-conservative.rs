// Runs the production executable on a marked root; the Python driver checks ranked identities after paging.
#[test]
fn search_protocol_conservative_matching_and_scope_changes() {
    let output = std::process::Command::new("python3")
        .args(["-B", "-c", include_str!("search-conservative.py"), env!("CARGO_BIN_EXE_flea")])
        .output().expect("protocol test needs python3");
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "protocol exit {}", output.status);
}
