use super::{
    fixture::{Fixture, SHELL_GUARD},
    source::Sources,
    Service,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const RACE: &str = "presentation::regression_tests::u01_fix2_same_content_save_during_probe_publishes_stable_snapshot";
const GUARD: &str = "presentation::regression_tests::u01_fix2_fixtures_preserve_unmarked_sentinels_and_refuse_unowned_paths";

fn child(test: &str, fixture: &Fixture) {
    let output = Command::new("/usr/bin/setsid")
        .arg("-w")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env_clear()
        .env("HOME", fixture.path())
        .env("TMPDIR", fixture.path())
        .env("U01_FIX2_ROOT", fixture.path())
        .env("U01_FIXTURE_PARENT", Fixture::parent())
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", fixture.path().join("bin").display()),
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}{stderr}");
    assert!(
        stdout.contains("running 1 test"),
        "child did not run its regression"
    );
    assert!(output.status.success(), "isolated regression failed");
}

fn child_fixture() -> Option<Fixture> {
    let path = std::env::var_os("U01_FIX2_ROOT")?;
    assert!(std::fs::File::open("/dev/tty").is_err());
    assert!(std::env::vars_os().all(|(key, _)| {
        [
            "HOME",
            "TMPDIR",
            "U01_FIX2_ROOT",
            "U01_FIXTURE_PARENT",
            "PATH",
        ]
        .iter()
        .any(|allowed| key == *allowed)
    }));
    println!("U01_FIX2_CI tty=none env=explicit pidns=broker");
    Some(Fixture::open(path.into()).unwrap())
}

#[test]
fn u01_fix2_same_content_save_during_probe_publishes_stable_snapshot() {
    if let Some(fixture) = child_fixture() {
        let root = fixture.path();
        let (service, initial) = Service::start();
        let updates = service.subscribe();
        assert_eq!(updates.recv_blocking().unwrap().fonts.base, 12);
        fixture.write(
            &root.join(".config/omarchy/shell.toml"),
            "[font]\nbase-size=16\n",
        );
        let font = root.join(".config/fontconfig/fonts.conf");
        fixture.write(&font, "font-b");
        let deadline = Instant::now() + Duration::from_secs(8);
        while !root.join("entered").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            root.join("entered").exists(),
            "second probe never reached its barrier"
        );
        let before = Sources::current().stamps();
        fixture.write(&font, "font-b");
        assert_ne!(
            Sources::current().stamps(),
            before,
            "same-byte save did not change metadata"
        );
        fixture.write(&root.join("release"), "go");
        let until = Instant::now() + Duration::from_secs(3);
        let mut latest = initial;
        while Instant::now() < until {
            while let Ok(next) = updates.try_recv() {
                latest = next;
            }
            if latest.fonts.base == 16 {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        println!(
            "U01_FIX2_RACE expected_base=16 observed_base={} family={}",
            latest.fonts.base, latest.fonts.family
        );
        assert_eq!(
            latest.fonts.base, 16,
            "stable input was consumed but never published"
        );
        assert_eq!(latest.fonts.family, "Second Mono");
        drop(service);
        return;
    }
    let fixture = Fixture::new("same-save");
    let root = fixture.path();
    fixture.write(
        &root.join(".config/omarchy/shell.toml"),
        "[font]\nbase-size=12\n",
    );
    fixture.write(&root.join(".config/fontconfig/fonts.conf"), "font-a");
    let helper = root.join("bin/fc-match");
    fixture.write(
        &helper,
        format!(
            r#"#!/bin/sh
set -eu
{SHELL_GUARD}
if [ -f "$HOME/first" ]; then
    guard "$HOME" "$HOME/entered" "$U01_FIXTURE_PARENT"
    printf entered > "$HOME/entered"
    while [ ! -f "$HOME/release" ]; do sleep 0.001; done
    printf 'Second Mono'
else
    guard "$HOME" "$HOME/first" "$U01_FIXTURE_PARENT"
    printf first > "$HOME/first"
    printf 'First Mono'
fi
"#
        ),
    );
    fixture.executable(&helper);
    child(RACE, &fixture);
    fixture.cleanup();
}

#[test]
fn u01_fix2_fixtures_preserve_unmarked_sentinels_and_refuse_unowned_paths() {
    if let Some(enclosing) = child_fixture() {
        for name in ["launch", "idle", "cache", "stamp"] {
            let target = enclosing
                .path()
                .join(format!("flea-u01-{name}-{}", std::process::id()));
            let sentinel = target.join("preexisting-unowned-file");
            enclosing.write(&sentinel, "must survive fixture allocation");
            assert!(!target.join(".flea-test-sandbox").exists());
            let allocated = Fixture::new(name);
            assert_ne!(allocated.path(), target);
            assert!(Fixture::open(target).is_err(), "unmarked root was accepted");
            assert_eq!(
                std::fs::read_to_string(&sentinel).unwrap(),
                "must survive fixture allocation"
            );
            for refused in [
                Path::new(""),
                Path::new("relative"),
                enclosing.path().parent().unwrap(),
            ] {
                assert!(allocated.check(refused).is_err());
            }
            assert!(allocated
                .check(&allocated.path().join("../escape"))
                .is_err());
            allocated.cleanup();
            println!("U01_FIX2_GUARD {name} unmarked=refused sentinel=preserved");
        }
        return;
    }
    let fixture = Fixture::new("sentinel");
    child(GUARD, &fixture);
    fixture.cleanup();
}
