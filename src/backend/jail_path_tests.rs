use super::*;
use crate::backend::testdir::TestDir;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn sandbox_availability_rejects_relative_launcher_entries() {
    if std::env::var_os("FLEA_SANDBOX_AVAILABILITY_PROBE").is_some() {
        let available = crate::backend::sandbox::available();
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
            std::os::unix::fs::symlink(
                Path::new("/usr/bin").join(tool),
                root.join(directory).join(tool),
            )
            .unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backend::jail::path_tests::sandbox_availability_rejects_relative_launcher_entries",
                "--nocapture",
            ])
            .current_dir(root.path())
            .env("FLEA_SANDBOX_AVAILABILITY_PROBE", "1")
            .env("PATH", format!("rel:{}", root.join("abs").display()))
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(
            output.status.success(),
            "{relative_tool} probe child failed"
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
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
