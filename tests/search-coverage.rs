// Runs the shipping backend on a marked generated tree and checks its complete identity inventory.
#[test]
fn search_protocol_covers_three_hundred_thousand_files_and_reports_refusals() {
    let output = std::process::Command::new("python3")
        .args(["-B", "-c", include_str!("search-coverage.py"), env!("CARGO_BIN_EXE_flea")])
        .output().expect("protocol test needs python3");
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "protocol exit {}", output.status);
}
