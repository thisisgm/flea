use std::process::Command;

fn run(case: &str) {
    let status = Command::new("python3")
        .args(["-B", "tests/model3d_protocol.py", env!("CARGO_BIN_EXE_flea"), case, env!("CARGO_TARGET_TMPDIR")])
        .status().expect("python3 is required for the 3D protocol integration tests");
    assert!(status.success(), "3D protocol integration case {case} failed: {status}");
}

#[test]
fn model3d_protocol_real_sandbox_model_facts_and_existing_thumbnail_cache() { run("sandbox"); }

#[test]
fn model3d_protocol_missing_sandbox_refuses_before_parsing_or_publishing() { run("missing-sandbox"); }
