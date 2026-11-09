// A row whose name is not UTF-8 is acted on as that exact file over the wire, see docs/architecture/exact-names.md.
use std::process::Command;

#[test]
fn a_row_whose_name_is_not_utf8_acts_on_its_own_file_through_every_operation() {
    let output = Command::new("python3")
        .args([
            "-I",
            "-B",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/exact_names.py"),
            env!("CARGO_BIN_EXE_flea"),
        ])
        .output()
        .unwrap();
    print!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
