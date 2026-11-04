//! Reuse the unit tests' independent namespace probe, outside the production jail builder.
mod sandbox {
    pub const BWRAP: &str = "bwrap";
}

#[path = "../../src/backend/sandboxprobe.rs"]
mod probe;

pub fn preview_supported() -> bool {
    let supported = !probe::skipped();
    if !supported {
        eprintln!(
            "SKIP torrent-preview enable and runtime exchange: namespace capability unavailable"
        );
    }
    supported
}

use super::Sandbox;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const RUNTIME_TEST: &str = "plugins_doc_examples_add_enable_and_answer_through_the_runtime";
const PROBE_ARGS: &str = "--unshare-user --unshare-pid --unshare-net --die-with-parent --ro-bind / / --proc /proc /usr/bin/true";

// Each subprocess has its own PATH and OnceLock, so the two controls cannot affect other tests.
fn controlled_runtime(name: &str, probe_exit: u8) -> (std::process::Output, Vec<String>) {
    let sandbox = Sandbox::new(name);
    let bin = sandbox.0.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let calls = sandbox.0.join("bwrap-calls");
    let bwrap = bin.join("bwrap");
    std::fs::write(
        &bwrap,
        format!(
            "#!/bin/sh\nif [ \"$*\" = '{PROBE_ARGS}' ]; then\n  echo probe >> '{calls}'\n  echo 'controlled namespace refusal' >&2\n  exit {probe_exit}\nfi\necho jail >> '{calls}'\nexit 97\n",
            calls = calls.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = vec![bin];
    path.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let output = Command::new("timeout")
        .args(["--signal=KILL", "120s"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", RUNTIME_TEST, "--nocapture"])
        .env("PATH", std::env::join_paths(path).unwrap())
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(calls)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    (output, calls)
}

fn transcript(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn opr_unsupported_preview_keeps_ordinary_runtime_assertions() {
    let (output, calls) = controlled_runtime("unsupported", 1);
    let text = transcript(&output);
    assert!(
        text.contains("OPR ordinary runtime assertions completed"),
        "{text}"
    );
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("running 1 test"), "{text}");
    assert!(text.contains("1 passed; 0 failed; 0 ignored"), "{text}");
    assert!(text.contains(&format!("SKIP {RUNTIME_TEST}:")), "{text}");
    assert!(
        text.contains("SKIP torrent-preview enable and runtime exchange:"),
        "{text}"
    );
    assert!(text.contains("controlled namespace refusal"), "{text}");
    assert_eq!(
        calls,
        ["probe"],
        "unsupported preview must not start a jail"
    );
    eprintln!(
        "OPR unsupported control: runtime assertions passed, preview jail calls=0, child exit=0"
    );
}

#[test]
fn opr_allowed_probe_preserves_a_real_preview_jail_failure() {
    let (output, calls) = controlled_runtime("allowed-failure", 0);
    let text = transcript(&output);
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(text.contains("running 1 test"), "{text}");
    assert!(text.contains("0 passed; 1 failed; 0 ignored"), "{text}");
    assert!(text.contains("torrent-preview: Obj"), "{text}");
    assert!(text.contains("plugin closed stdout"), "{text}");
    assert!(!text.contains("SKIP "), "{text}");
    assert_eq!(calls, ["probe", "jail", "jail"]);
    eprintln!("OPR allowed control: real preview jail attempted, product failure preserved, child exit=101");
}
