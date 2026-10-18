use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn sandbox_availability_rejects_relative_launcher_entries() {
    if let Ok(expected) = std::env::var("FLEA_SANDBOX_AVAILABILITY_PROBE") {
        let available = crate::backend::sandbox::available();
        if expected == "available" {
            assert!(
                available,
                "an absolute PATH to executable launchers must be available"
            );
            return;
        }
        let full = crate::backend::sandbox::wrap_readonly(
            &["/usr/bin/true".into()],
            Path::new("/usr/bin/true"),
        );
        let launched = spawn_jailed(&full, |cmd| {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        });
        let message = launched.err().map(|error| crate::error::io_message(&error));
        println!(
            "PATH={:?} available={available} launch error={message:?}",
            std::env::var_os("PATH")
        );
        assert!(
            !available,
            "available() accepted a PATH the launcher refuses"
        );
        assert_eq!(message.as_deref(), Some("Sandbox launcher is unavailable"));
        return;
    }
    for relative_tool in ["bwrap", "prlimit"] {
        let root = TestDir::new("sandbox-relative-launcher");
        root.dir("abs");
        root.dir("rel");
        for tool in ["bwrap", "prlimit"] {
            let directory = if tool == relative_tool { "rel" } else { "abs" };
            let target = Path::new("/usr/bin").join(tool);
            // A missing target leaves both PATHs unavailable, so the refusal below would prove nothing.
            assert!(
                executable_file(&target),
                "launcher fixture target {} must be executable",
                target.display()
            );
            std::os::unix::fs::symlink(&target, root.join(directory).join(tool)).unwrap();
        }
        // The same fixture reached only through absolute entries is the positive control.
        let absolute = format!(
            "{}:{}",
            root.join("rel").display(),
            root.join("abs").display()
        );
        let relative = format!("rel:{}", root.join("abs").display());
        for (path, expected) in [(absolute, "available"), (relative, "unavailable")] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "backend::jail::path_tests::sandbox_availability_rejects_relative_launcher_entries",
                    "--nocapture",
                ])
                .current_dir(root.path())
                .env("FLEA_SANDBOX_AVAILABILITY_PROBE", expected)
                .env("PATH", path)
                .output()
                .unwrap();
            // The child's output stays captured; echoing a SKIP line from it would count as a skip in CI.
            assert!(
                output.status.success(),
                "{relative_tool} {expected} probe child failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
    }
}

#[test]
fn launchers_ignore_relative_empty_and_unset_path_entries() {
    if let Some(case) = std::env::var_os("FLEA_JAIL_PATH_PROBE") {
        let case = case.to_str().unwrap();
        let args = if case == "bwrap" {
            vec![
                "prlimit".into(),
                "--".into(),
                "bwrap".into(),
                "--version".into(),
            ]
        } else {
            vec!["prlimit".into(), "--".into(), "/usr/bin/true".into()]
        };
        let launched = spawn_jailed(&args, |cmd| {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        });
        if ["relative-only", "empty", "unset"].contains(&case) {
            assert!(
                matches!(launched, Err(error) if error.kind() == std::io::ErrorKind::NotFound),
                "a bare launcher without an absolute PATH entry must not run"
            );
        } else {
            assert!(
                launched.unwrap().child.wait().unwrap().success(),
                "resolved {case} from cwd"
            );
        }
        return;
    }
    let mut failures = Vec::new();
    for case in ["prlimit", "bwrap", "relative-only", "empty", "unset"] {
        let root = TestDir::new("jail-absolute-path");
        root.script("prlimit", "#!/bin/sh\nexit 73\n");
        root.script("bwrap", "#!/bin/sh\nexit 74\n");
        let mut probe = Command::new(std::env::current_exe().unwrap());
        probe
            .args([
                "--exact",
                "backend::jail::path_tests::launchers_ignore_relative_empty_and_unset_path_entries",
                "--nocapture",
            ])
            .current_dir(root.path())
            .env("FLEA_JAIL_PATH_PROBE", case);
        match case {
            "relative-only" => {
                probe.env("PATH", ".:relative");
            }
            "empty" => {
                probe.env("PATH", "");
            }
            "unset" => {
                probe.env_remove("PATH");
            }
            _ => {
                probe.env("PATH", ".:relative::/usr/bin:/bin");
            }
        }
        let output = probe.output().unwrap();
        println!("launcher {case}: {:?}", output.status.code());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "launcher {case} must execute its exact probe: {output:?}"
        );
        if !output.status.success() {
            failures.push((case, String::from_utf8_lossy(&output.stderr).into_owned()));
        }
    }
    assert!(failures.is_empty(), "launcher PATH failures: {failures:?}");
}

#[test]
fn sandbox_availability_requires_executable_launchers() {
    const FLAG: &str = "FLEA_SANDBOX_EXECUTABILITY_PROBE";
    // A regular file with no execute bit, which the launcher lookup refuses.
    const NON_EXECUTABLE_MODE: u32 = 0o644;
    // The same file with execute bits, the positive control.
    const EXECUTABLE_MODE: u32 = 0o755;
    if let Ok(expected) = std::env::var(FLAG) {
        let available = crate::backend::sandbox::available();
        if expected == "available" {
            assert!(available, "executable launchers must be available");
            return;
        }
        let full = crate::backend::sandbox::wrap_readonly(
            &["/usr/bin/true".into()],
            Path::new("/usr/bin/true"),
        );
        let launched = spawn_jailed(&full, |_| {});
        let message = launched.err().map(|error| crate::error::io_message(&error));
        assert_eq!(message.as_deref(), Some("Sandbox launcher is unavailable"));
        assert!(
            !available,
            "non-executable launchers must be unavailable before spawning a worker"
        );
        return;
    }
    for blocked in ["bwrap", "prlimit"] {
        let root = TestDir::new("sandbox-nonexecutable-launcher");
        let path = root.file(blocked, "not executable\n");
        root.assert_contains(&path);
        let other = if blocked == "bwrap" {
            "prlimit"
        } else {
            "bwrap"
        };
        let target = Path::new("/usr/bin/true");
        // A missing target leaves the PATH unavailable whatever the blocked file's mode, so the refusal would prove nothing.
        assert!(
            executable_file(target),
            "launcher fixture target {} must be executable",
            target.display()
        );
        std::os::unix::fs::symlink(target, root.join(other)).unwrap();
        for (mode, expected) in [
            (EXECUTABLE_MODE, "available"),
            (NON_EXECUTABLE_MODE, "unavailable"),
        ] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "backend::jail::path_tests::sandbox_availability_requires_executable_launchers",
                    "--nocapture",
                ])
                .env(FLAG, expected)
                .env("PATH", root.path())
                .output()
                .unwrap();
            // The child's output stays captured; echoing a SKIP line from it would count as a skip in CI.
            assert!(
                output.status.success(),
                "{blocked} {expected} probe child failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
    }
}
