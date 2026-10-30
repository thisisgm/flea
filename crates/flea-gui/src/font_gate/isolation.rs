use crate::presentation::fixture::Fixture;
use std::process::{Command, Stdio};

pub(super) fn enter(test: &str, script: &str) -> bool {
    if let Some(root) = std::env::var_os("FONT_GATE_ROOT") {
        let fixture = Fixture::open(root.into()).unwrap();
        assert_eq!(std::env::var_os("HOME").unwrap(), fixture.path());
        for name in [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
        ] {
            assert!(std::path::PathBuf::from(std::env::var_os(name).unwrap())
                .starts_with(fixture.path()));
        }
        assert!(fixture
            .check(std::path::Path::new("/tmp/font-gate-unowned"))
            .is_err());
        return false;
    }
    let fixture = Fixture::new("font-gate");
    let root = fixture.path();
    let helper = root.join("bin/fc-match");
    fixture.write(&helper, script);
    fixture.executable(&helper);
    let output = Command::new("/usr/bin/setsid")
        .arg("-w")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env_clear()
        .env("FONT_GATE_ROOT", root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    println!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fixture.cleanup();
    assert!(
        output.status.success(),
        "isolated system-font surface gate failed"
    );
    // libtest exits 0 when --exact matches no test, so the child must report the one it ran.
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"),
        "isolated system-font child ran no {test}"
    );
    true
}

#[test]
#[should_panic(expected = "isolated system-font child ran no font_gate::no_such_test")]
fn font_gate_isolated_child_that_matches_no_test_fails() {
    enter("font_gate::no_such_test", "#!/bin/sh\nexit 1\n");
}
