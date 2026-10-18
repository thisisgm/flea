use super::test_support::Fixture;
use super::*;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn missing_chafa_is_none_and_nonexecutables_do_not_count() {
    let fixture = Fixture::new();
    let path = fixture.path("bin");
    assert!(find_on(path.as_os_str()).is_none());
    let input = File::open(fixture.file("input", b"held")).unwrap();
    assert!(
        Job::start_on(input, 2, 2, ColorMode::Truecolor, path.as_os_str(),)
            .unwrap()
            .is_none()
    );
    let stub = fixture.file("bin/chafa", b"not executable");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(find_on(path.as_os_str()).is_none());
}

#[test]
fn chafa_lookup_takes_executable_files_and_links_to_them_only() {
    let fixture = Fixture::new();
    // One candidate per PATH directory, on both sides of the execute-bit criterion.
    let modes = [
        ("owner", 0o700),
        ("group", 0o610),
        ("other", 0o601),
        ("none", 0o644),
    ];
    for (name, mode) in modes {
        std::fs::create_dir(fixture.path(name)).unwrap();
        let candidate = fixture.file(&format!("{name}/chafa"), b"#!/bin/sh\n");
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    for name in ["directory", "link", "dangling"] {
        std::fs::create_dir(fixture.path(name)).unwrap();
    }
    std::fs::create_dir(fixture.path("directory/chafa")).unwrap();
    symlink(fixture.path("group/chafa"), fixture.path("link/chafa")).unwrap();
    symlink(fixture.path("missing"), fixture.path("dangling/chafa")).unwrap();
    // A link answers with the file it names, so the program run is the one that was checked.
    let found = |name: &str| find_on(fixture.path(name).as_os_str());
    assert_eq!(found("owner"), Some(fixture.path("owner/chafa")));
    assert_eq!(found("group"), Some(fixture.path("group/chafa")));
    assert_eq!(found("other"), Some(fixture.path("other/chafa")));
    assert_eq!(found("link"), Some(fixture.path("group/chafa")));
    for name in ["none", "directory", "dangling"] {
        assert_eq!(found(name), None, "{name}");
    }
    let rejected_first = std::env::join_paths(["none", "directory", "dangling", "other"].map(|name| fixture.path(name))).unwrap();
    assert_eq!(find_on(&rejected_first), Some(fixture.path("other/chafa")));
}

#[test]
fn relative_and_empty_path_entries_are_never_searched() {
    let fixture = Fixture::new();
    fixture.stub("printf '▀'");
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    let relative = fixture.path("bin").strip_prefix(&cwd).unwrap().to_owned();
    assert!(
        find_on(relative.as_os_str()).is_none(),
        "relative PATH must not search the working directory"
    );
    for entry in ["", ".", "bin", ":", ".:bin:"] {
        assert!(std::env::split_paths(OsStr::new(entry)).all(|p| !p.is_absolute()));
        assert!(find_on(OsStr::new(entry)).is_none());
    }
    assert!(find_on(fixture.path("bin").as_os_str()).is_some());
}

#[test]
fn drop_returns_before_a_held_worker_can_finish() {
    let cancel = Arc::new(AtomicBool::new(false));
    let (release, held) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        held.recv().unwrap();
        done.send(()).unwrap();
    });
    let (_tx, result) = mpsc::channel();
    let job = Job {
        result,
        cancel: cancel.clone(),
        worker: Some(worker),
    };
    let (dropped, returned) = mpsc::channel();
    let dropping = std::thread::spawn(move || {
        let started = Instant::now();
        drop(job);
        dropped.send(started.elapsed()).unwrap();
    });
    let elapsed = returned.recv_timeout(Duration::from_millis(250));
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(2)).unwrap();
    dropping.join().unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(
        elapsed.is_ok(),
        "Drop blocked on the held worker: {elapsed:?}"
    );
    println!("Drop duration: {} us", elapsed.unwrap().as_micros());
}

#[test]
fn empty_and_unset_path_never_use_an_executable_in_cwd() {
    if std::env::var_os("FLEA_CHAFA_PATH_PROBE").is_some() {
        assert!(find_on(OsStr::new("")).is_none());
        assert!(find_on(OsStr::new(".")).is_none());
        assert!(
            Job::start(File::open("input").unwrap(), 1, 1, ColorMode::Truecolor)
                .unwrap()
                .is_none()
        );
        return;
    }
    let fixture = Fixture::new();
    fixture.stub("printf '▀'");
    fixture.file("bin/input", b"held");
    for empty in [false, true] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "tui::chafa::job_tests::empty_and_unset_path_never_use_an_executable_in_cwd",
                "--nocapture",
            ])
            .current_dir(fixture.path("bin"))
            .env("FLEA_CHAFA_PATH_PROBE", "1")
            .env_remove("PATH");
        if empty {
            child.env("PATH", "");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "the exact PATH probe must execute: {output:?}"
        );
    }
}

#[test]
fn no_namespace_skips_jail_legs_and_runs_parser_cases() {
    if std::env::var_os("FLEA_CHAFA_NO_NAMESPACE_PROBE").is_some() {
        return;
    }
    let fixture = Fixture::new();
    let fake = fixture.path("nojail");
    std::fs::create_dir(&fake).unwrap();
    for tool in ["prlimit", "cat", "chmod"] {
        symlink(Path::new("/usr/bin").join(tool), fake.join(tool)).unwrap();
    }
    let mut writer = Command::new("/bin/sh")
        .args([
            "-c",
            "/usr/bin/cat > \"$1\" && /usr/bin/chmod 755 \"$1\"",
            "sh",
        ])
        .arg(fake.join("bwrap"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    writer.stdin.take().unwrap().write_all(b"#!/bin/sh\necho 'bwrap: No permissions to create new namespace, likely because the kernel does not allow non-privileged user namespaces.' >&2\nexit 1\n").unwrap();
    assert!(writer.wait().unwrap().success());
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["tui::chafa::", "--nocapture"])
        .env("PATH", &fake)
        .env("FLEA_CHAFA_NO_NAMESPACE_PROBE", "1")
        .output()
        .unwrap();
    // The child's SKIP lines stay in its captured output; echoing them would count as skips in CI.
    assert!(
        output.status.success(),
        "chafa module must survive namespace refusal: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("SKIP tui::chafa::"));
    assert!(stderr.contains(
        ": the bwrap sandbox cannot run here: bwrap: No permissions to create new namespace"
    ));
    let stdout = String::from_utf8_lossy(&output.stdout);
    for case in [
        "tests::valid_truecolor_and_indexed_output_becomes_cells_with_resets",
        "tests::each_control_escape_and_unsupported_sgr_is_refused_individually",
        "job_tests::missing_chafa_is_none_and_nonexecutables_do_not_count",
        "process_tests::output_cap_kills_decoder_before_it_can_hang",
        "process_tests::dropping_job_cancels_and_eventually_joins_pending_decoder",
    ] {
        assert!(
            stdout.contains(&format!("tui::chafa::{case} ... ok")),
            "namespace refusal did not execute {case}: {output:?}"
        );
    }
    for case in [
        "output_cap_kills_decoder_before_it_can_hang",
        "dropping_job_cancels_and_eventually_joins_pending_decoder",
    ] {
        assert!(
            stderr.contains(&format!("SKIP tui::chafa::process_tests::{case}:")),
            "namespace refusal did not skip jailed case {case}: {output:?}"
        );
    }
}
