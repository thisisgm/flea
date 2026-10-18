use super::{gitfixture::Fixture, gitprobe, sandbox, sandboxprobe};

#[test]
fn capture_bounds_stdout_deadline_and_preserves_stderr_without_a_sandbox_tool() {
    let d = Fixture::new();
    d.write("probe", b"#!/bin/sh\nprintf 'stalled' >&2\nsleep 30\n");
    use std::os::unix::fs::PermissionsExt;
    let script = d.path("probe");
    d.check(&script);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let argv = vec![script.to_str().unwrap().to_string()];
    let start = std::time::Instant::now();
    let error = gitprobe::capture(&argv, std::time::Duration::from_millis(30)).unwrap_err();
    assert!(
        error.contains("timed out") && error.contains("stalled"),
        "{error}"
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(4));
    let start = std::time::Instant::now();
    let error = gitprobe::capture_with(&argv, std::time::Duration::from_secs(3), &|| {
        start.elapsed() > std::time::Duration::from_millis(30)
    })
    .unwrap_err();
    assert!(
        error.contains("cancelled") && error.contains("stalled"),
        "{error}"
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(4));
    assert!(gitprobe::capture_with(
        &[d.path("missing").to_string_lossy().into_owned()],
        std::time::Duration::from_secs(3),
        &|| true
    )
    .unwrap_err()
    .contains("cancelled"));
    d.write(
        "probe",
        b"#!/bin/sh\nprintf 'specific failure' >&2\nexit 23\n",
    );
    let error = gitprobe::capture(&argv, std::time::Duration::from_secs(3)).unwrap_err();
    assert!(
        error.contains("23") && error.contains("specific failure"),
        "{error}"
    );
    d.write("probe", b"#!/bin/sh\nhead -c 33554433 /dev/zero\n");
    let error = gitprobe::capture(&argv, std::time::Duration::from_secs(3)).unwrap_err();
    assert!(error.contains("output is too large"), "{error}");
}

#[test]
fn status_argv_is_readonly_disables_integrations_and_delimits_paths() {
    if !Fixture::available() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let argv = gitprobe::argv(&repo);
    let index = argv
        .iter()
        .position(|argument| argument == "/usr/bin/git")
        .unwrap();
    assert_eq!(argv.last().unwrap(), "--");
    assert!(argv[..index]
        .windows(3)
        .any(|args| args == ["--setenv", "GIT_OPTIONAL_LOCKS", "0"]));
    assert!(argv[index..]
        .windows(2)
        .any(|args| args == ["-c", "core.fsmonitor=false"]));
    assert!(argv[index..]
        .windows(2)
        .any(|args| args == ["-c", "core.hooksPath=/dev/null"]));
    assert!(argv.iter().any(|argument| argument == "--unshare-all"));
    assert!(!argv.iter().any(|argument| argument == "--bind"));
    assert!(!argv[index..]
        .iter()
        .any(|argument| ["sh", "bash", "-C"].contains(&argument.as_str())));
}

#[test]
fn git_deadline_kills_sandbox_and_keeps_stderr_and_exit_failures() {
    if sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.write("probe", b"#!/bin/sh\nprintf 'stalled' >&2\nsleep 30\n");
    use std::os::unix::fs::PermissionsExt;
    let script = d.path("probe");
    d.check(&script);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let inner = vec![script.to_str().unwrap().to_string()];
    let mut argv = sandbox::wrap_readonly(&inner, &d.0);
    sandbox::add_status(&mut argv, inner.len());
    let started = std::time::Instant::now();
    let error = gitprobe::capture(&argv, std::time::Duration::from_millis(200)).unwrap_err();
    assert!(
        error.contains("timed out") && error.contains("stalled"),
        "{error}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    d.write(
        "probe",
        b"#!/bin/sh\nprintf 'specific failure' >&2\nexit 23\n",
    );
    let error = gitprobe::capture(&argv, std::time::Duration::from_secs(3)).unwrap_err();
    assert!(
        error.contains("23") && error.contains("specific failure"),
        "{error}"
    );
}
