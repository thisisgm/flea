// The six compatibility suites on demand (`-- --ignored`); flea-ci runs them through tests/run-all.sh.
use std::path::Path;
use std::process::Command;

fn suite(name: &str, ending: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("bash")
        .arg(root.join("tests").join(format!("{name}.sh")))
        .current_dir(root)
        .env("FLEA_BIN", env!("CARGO_BIN_EXE_flea"))
        .output()
        .expect("compatibility suite starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}");
    assert!(
        output.status.success(),
        "{name}: {}\n{stderr}",
        output.status
    );
    assert_eq!(
        stdout.lines().last(),
        Some(ending),
        "{name}: no final success line"
    );
}

#[test]
#[ignore = "compatibility suite, run with -- --ignored; flea-ci runs it through tests/run-all.sh"]
fn cli_contract() {
    suite("cli-contract", "cli-contract: all rows passed");
}

#[test]
#[ignore = "compatibility suite, run with -- --ignored; flea-ci runs it through tests/run-all.sh"]
fn cli_mutations() {
    suite(
        "cli-contract-mutations",
        "cli-contract-mutations: all 8 source mutations rejected",
    );
}

#[test]
#[ignore = "compatibility suite, run with -- --ignored; flea-ci runs it through tests/run-all.sh"]
fn legacy_upgrade() {
    suite("legacy-upgrade", "legacy-upgrade: all cases passed");
}

#[test]
#[ignore = "compatibility suite, run with -- --ignored; flea-ci runs it through tests/run-all.sh"]
fn legacy_helpers() {
    suite("legacy-helpers", "legacy-helpers: all cases passed");
}

#[test]
#[ignore = "compatibility suite, run with -- --ignored; flea-ci runs it through tests/run-all.sh"]
fn state_compat() {
    suite(
        "state-compat",
        "state-compat: all covered readers preserved source files",
    );
}

#[test]
#[ignore = "compatibility suite, run with -- --ignored; flea-ci runs it through tests/run-all.sh"]
fn picker_filters_allow_repeated_chooser_calls() {
    suite("chooser-replies", "chooser-replies: all cases passed");
}
