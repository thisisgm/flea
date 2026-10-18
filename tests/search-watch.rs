// Exercises external mutations against the production session without sending another query.
#[test]
fn search_recursive_watch_updates_the_standing_generation() {
    let output = std::process::Command::new("python3")
        .args(["-B", "-c", include_str!("search-watch.py"), env!("CARGO_BIN_EXE_flea")])
        .output().expect("protocol test needs python3");
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "protocol exit {}", output.status);
}
