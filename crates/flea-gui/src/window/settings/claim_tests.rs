use super::*;
use crate::g2::settings_pages::claim::DESKTOP;
use std::{
    fs,
    path::Path,
    process::Stdio,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CHILD: &str = "window::settings::claim::tests::isolated_completed";

fn script(root: &Path, name: &str, body: &str) {
    let path = root.join("bin").join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    flea::executable_file::write(&path, &std::fs::read(&path).unwrap(), 0o700).unwrap();
}

fn run(case: &str) {
    run_child(case, CHILD);
}

fn run_child(case: &str, child: &str) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = workspace.join(format!(
        ".superpowers/gh123/claim-{case}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir(root.join("run")).unwrap();
    fs::write(root.join(".flea-test-sandbox"), "gh123 claim test\n").unwrap();
    script(
        &root,
        "flea",
        r#"printf '%s\n' "$@" > "$GH123_ROOT/flea.args"
case "$GH123_CASE" in
    partial) printf '%s\n' 'flea: no file chooser service is installed, so Flea could not become the file chooser' >&2 ;;
    uninstalled) exit 3 ;;
    child_error) printf '%s\n' 'noise' 'flea: routing refused' >&2; exit 7 ;;
    child_status) exit 7 ;;
esac"#,
    );
    script(
        &root,
        "xdg-mime",
        r#"printf '%s\n' "$@" > "$GH123_ROOT/mime.args"
echo ASKED >> "$GH123_ROOT/events.log"
# The held case's first read answers only once the test opens the gate.
if [ "$GH123_CASE" = held ] && [ ! -e "$GH123_ROOT/asked-once" ]; then
    : > "$GH123_ROOT/asked-once"
    while [ ! -e "$GH123_ROOT/gate" ]; do /bin/sleep 0.05; done
fi
echo ANSWERED >> "$GH123_ROOT/events.log"
if [ "$GH123_CASE" = release ]; then
    printf '%s\n' org.example.Files.desktop
else
    printf '%s\n' com.thisisgm.flea.desktop
fi"#,
    );
    script(
        &root,
        "systemctl",
        r#"printf '%s\n' "$@" > "$GH123_ROOT/systemctl.args"
[ "$GH123_CASE" != refused ]"#,
    );
    script(
        &root,
        "busctl",
        r#"printf '%s\n' "$@" > "$GH123_ROOT/busctl.args"
case "$GH123_CASE" in
    ready|release) printf 'u 4\n' ;;
    malformed) printf 's 4\n' ;;
    zero) printf 'u 0\n' ;;
    stalled) printf '%s\n' "$$" > "$GH123_ROOT/probe.pid"; exec /bin/sleep 30 ;;
    *) exit 1 ;;
esac"#,
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", child, "--nocapture"])
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", &root)
        .env("XDG_RUNTIME_DIR", root.join("run"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/no-bus", root.display()),
        )
        .env("GH123_ROOT", &root)
        .env("GH123_CASE", case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(12) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "{case}: completed() exceeded 12 seconds; evidence: {}",
                root.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{case}: {}\n{}\nevidence: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        root.display()
    );
}

#[test]
fn isolated_completed() {
    let Ok(case) = std::env::var("GH123_CASE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("GH123_ROOT").unwrap());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let mut claim = Claim {
        handler: if case == "release" {
            DESKTOP
        } else {
            "before.desktop"
        }
        .into(),
        packaged: true,
        probed: true,
        ..Claim::default()
    };
    assert_eq!(claim.start(), Some(case != "release"));
    let binary = root.join("bin").join(if case == "unstarted" {
        "missing"
    } else {
        "flea"
    });
    let start = Instant::now();
    let claim = completed(claim, binary);
    assert!(!claim.running);
    assert_eq!(claim.revision, 1);
    assert!(claim.probed);
    assert_eq!(claim.claiming, case != "release");
    assert_eq!(claim.packaged, case != "uninstalled");
    let expected = match case.as_str() {
        "ready" | "release" => None,
        "partial" => Some((Text::Key("settings.default.partly"), Role::Foreground)),
        "uninstalled" => Some((Text::Key("settings.default.uninstalled"), Role::Foreground)),
        "child_error" => Some((Text::data("routing refused"), Role::Error)),
        "child_status" => Some((
            Text::args("settings.default.exited", vec![Text::data("7")]),
            Role::Error,
        )),
        "unstarted" => Some((
            Text::args("settings.default.unstarted", vec![Text::data("missing")]),
            Role::Error,
        )),
        _ => Some((Text::Key("settings.default.portal"), Role::Foreground)),
    };
    assert_eq!(claim.note, expected, "{case}: Settings readiness result");
    if case == "unstarted" {
        assert_eq!(claim.handler, "before.desktop");
        for helper in ["flea", "mime", "systemctl", "busctl"] {
            assert!(!root.join(format!("{helper}.args")).exists());
        }
        return;
    }
    assert_eq!(
        fs::read_to_string(root.join("flea.args")).unwrap(),
        if case == "release" {
            "--default\noff\n"
        } else {
            "--default\n"
        }
    );
    assert_eq!(
        fs::read_to_string(root.join("mime.args")).unwrap(),
        "query\ndefault\ninode/directory\n"
    );
    assert_eq!(
        claim.handler,
        if case == "release" {
            "org.example.Files.desktop"
        } else {
            DESKTOP
        }
    );
    let child_failed = matches!(
        case.as_str(),
        "uninstalled" | "child_error" | "child_status"
    );
    if child_failed {
        assert!(!root.join("systemctl.args").exists());
    } else {
        assert_eq!(
            fs::read_to_string(root.join("systemctl.args")).unwrap(),
            "--user\ntry-restart\nxdg-desktop-portal.service\n"
        );
    }
    if child_failed || case == "refused" {
        assert!(!root.join("busctl.args").exists());
    } else {
        assert_eq!(fs::read_to_string(root.join("busctl.args")).unwrap(),
            "--user\n--auto-start=no\n--timeout=5s\nget-property\norg.freedesktop.portal.Desktop\n/org/freedesktop/portal/desktop\norg.freedesktop.portal.FileChooser\nversion\n");
    }
    if case == "stalled" {
        assert!(start.elapsed() >= Duration::from_secs(6));
        assert!(start.elapsed() < Duration::from_secs(10));
        let pid = fs::read_to_string(root.join("probe.pid")).unwrap();
        assert!(
            !Path::new("/proc").join(pid.trim()).exists(),
            "probe was not reaped"
        );
    }
}

const HELD_CHILD: &str = "window::settings::claim::tests::isolated_held_read";

// EY-451: DefaultClaim.qml readBusy keeps a pre-claim About read ahead of the claim's fresh read.
#[test]
fn isolated_held_read() {
    let Ok(case) = std::env::var("GH123_CASE") else {
        return;
    };
    assert_eq!(case, "held");
    let root = PathBuf::from(std::env::var_os("GH123_ROOT").unwrap());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let events = || fs::read_to_string(root.join("events.log")).unwrap_or_default();
    let about = std::thread::spawn(handler);
    while !root.join("asked-once").exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut claim = Claim {
        handler: "before.desktop".into(),
        packaged: true,
        probed: true,
        ..Claim::default()
    };
    assert_eq!(claim.start(), Some(true));
    let binary = root.join("bin").join("flea");
    let run = std::thread::spawn(move || completed(claim, binary));
    while !root.join("flea.args").exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        events(),
        "ASKED\n",
        "the claim's re-read started beside the held read"
    );
    fs::write(root.join("gate"), "").unwrap();
    assert_eq!(about.join().unwrap(), DESKTOP);
    let claim = run.join().unwrap();
    assert_eq!(events(), "ASKED\nANSWERED\nASKED\nANSWERED\n");
    assert_eq!(claim.handler, DESKTOP);
    assert!(!claim.running);
}

#[test]
fn a_claim_re_read_waits_for_the_read_already_out() {
    run_child("held", HELD_CHILD);
}

macro_rules! cases {
    ($($name:ident => $case:literal),+ $(,)?) => { $(
        #[test]
        fn $name() { run($case); }
    )+ };
}

cases! {
    successful_restart_without_filechooser_retains_portal_note => "absent",
    ready_unsigned_version_completes_claim => "ready",
    release_uses_the_same_readiness_path => "release",
    stopped_frontend_is_never_auto_started => "stopped",
    refused_restart_retains_portal_note => "refused",
    malformed_version_retains_portal_note => "malformed",
    zero_version_retains_portal_note => "zero",
    stalled_probe_is_bounded_and_reaped => "stalled",
    partial_claim_note_has_precedence => "partial",
    uninstalled_child_has_precedence => "uninstalled",
    child_diagnostic_has_precedence => "child_error",
    child_exit_status_has_precedence => "child_status",
    unstarted_child_has_precedence => "unstarted",
}
