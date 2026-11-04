use super::{gitfixture::Fixture, gitprobe, sandbox, sandboxprobe};
use std::cell::Cell;
use std::time::{Duration, Instant};

const STARTUP_DEADLINE: Duration = Duration::from_millis(30);
const CANCEL_AFTER_READY: Duration = Duration::from_millis(30);
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const PROCESS_EXIT_BOUND: Duration = Duration::from_secs(4);
const DELAYED_START_SECONDS: u64 = 1;
const STALL_SECONDS: u64 = 30;
const EXPECTED_OUTPUT_LIMIT: usize = 32 * 1024 * 1024;

#[test]
fn capture_bounds_stdout_deadline_and_preserves_stderr_without_a_sandbox_tool() {
    let d = Fixture::new();
    let argv = stalled_script(&d, false);
    assert_stalled_capture(&d, &argv);
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
    d.write(
        "probe",
        format!(
            "#!/bin/sh\nhead -c {} /dev/zero\n",
            EXPECTED_OUTPUT_LIMIT + 1
        )
        .as_bytes(),
    );
    let error = gitprobe::capture(&argv, std::time::Duration::from_secs(3)).unwrap_err();
    assert!(error.contains("output is too large"), "{error}");
}

fn stalled_script(d: &Fixture, delayed: bool) -> Vec<String> {
    d.write("probe", format!("#!/bin/sh\nif [ \"$2\" = delayed ]; then sleep {DELAYED_START_SECONDS}; fi\nprintf 'stalled' >&2\nprintf ready > \"$1\"\nsleep {STALL_SECONDS}\n").as_bytes());
    use std::os::unix::fs::PermissionsExt;
    let script = d.path("probe");
    d.check(&script);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    vec![
        script.to_str().unwrap().to_string(),
        d.path("ready").to_str().unwrap().to_string(),
        if delayed { "delayed" } else { "normal" }.to_string(),
    ]
}

fn assert_stalled_capture(d: &Fixture, argv: &[String]) {
    let ready = d.path("ready");
    let start = Instant::now();
    let error = gitprobe::capture(argv, STARTUP_DEADLINE).unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    if ready.is_file() {
        assert!(error.contains("stalled"), "{error}");
        d.check(&ready);
        std::fs::remove_file(&ready).unwrap();
    }
    assert!(start.elapsed() < PROCESS_EXIT_BOUND);
    let start = Instant::now();
    let ready_at = Cell::new(None);
    let error = gitprobe::capture_with(argv, CAPTURE_TIMEOUT, &|| {
        if ready_at.get().is_none() && ready.is_file() {
            ready_at.set(Some(Instant::now()));
        }
        ready_at
            .get()
            .is_some_and(|at| at.elapsed() > CANCEL_AFTER_READY)
    })
    .unwrap_err();
    assert!(
        error.contains("cancelled") && error.contains("stalled"),
        "{error}"
    );
    assert!(
        ready_at.get().is_some(),
        "the child must print stderr before cancellation"
    );
    assert!(start.elapsed() < PROCESS_EXIT_BOUND);
}

#[test]
fn git_capture_deadline_and_cancellation_allow_delayed_first_output() {
    let d = Fixture::new();
    let argv = stalled_script(&d, true);
    assert_stalled_capture(&d, &argv);
}

#[test]
fn git_capture_accepts_exactly_the_documented_output_limit() {
    let bytes = gitprobe::capture(
        &[
            "/usr/bin/head".into(),
            "-c".into(),
            EXPECTED_OUTPUT_LIMIT.to_string(),
            "/dev/zero".into(),
        ],
        std::time::Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(bytes.len(), EXPECTED_OUTPUT_LIMIT);
}

#[test]
fn status_argv_is_readonly_disables_integrations_and_delimits_paths() {
    let d = Fixture::new();
    d.write(".git/HEAD", b"ref: refs/heads/main\n");
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
