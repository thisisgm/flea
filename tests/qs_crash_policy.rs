//! Launch the real Flea binary against a signal/relaunch shim, without a display or a GPU failure.
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SIGABRT: i32 = 6;

struct Sandbox(PathBuf);

// A sibling fork inherits any descriptor held open for writing, and exec then fails with ETXTBSY.
fn executable(path: &Path, contents: &[u8]) {
    let mut writer = Command::new("/bin/sh")
        .args(["-c", "/bin/cat > \"$1\"", "write-fixture"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    writer.stdin.take().unwrap().write_all(contents).unwrap();
    assert!(writer.wait().unwrap().success());
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Sandbox {
    fn new() -> Self {
        let output = Command::new("mktemp").arg("-d").output().unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        fs::write(root.join(".flea-test-sandbox"), "qs-crash-policy\n").unwrap();
        for dir in ["bin", "ui/boot", "user", "runtime"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::set_permissions(root.join("runtime"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("ui/boot/shell.qml"), "// launch fixture\n").unwrap();
        fs::write(root.join("ui/boot/picker.qml"), "// picker fixture\n").unwrap();
        fs::write(root.join("notes.txt"), "plain text\n").unwrap();
        executable(
            &root.join("bin/flea"),
            &fs::read(env!("CARGO_BIN_EXE_flea")).unwrap(),
        );
        let sandbox = Self(root);
        sandbox.script(
            "flea-gui",
            r#"ulimit -c 0
case "$FLEA_TEST_QS_MODE" in
  crash) printf 'frontend-pid=%s\n' "$$"; kill -ABRT "$$";;
  open) exec "$FLEA_BIN" --open "$FLEA_TEST_TARGET";;
  *) printf '%s|%s|%s\n' "${QS_DISABLE_CRASH_HANDLER+x}" "${QS_DISABLE_CRASH_HANDLER-}" "${FLEA_QS_CRASH_POLICY-}";;
esac
"#,
        );
        sandbox.script(
            "gio",
            "printf '%s|%s|%s\\n' \"${QS_DISABLE_CRASH_HANDLER+x}\" \"${QS_DISABLE_CRASH_HANDLER-}\" \"${FLEA_QS_CRASH_POLICY-}\" > \"$FLEA_TEST_OPEN_LOG\"\n",
        );
        sandbox
    }

    fn script(&self, name: &str, body: &str) {
        let path = self.0.join("bin").join(name);
        executable(&path, format!("#!/bin/sh\n{body}").as_bytes());
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.0.join("bin/flea"));
        command
            .env_clear()
            .env("PATH", self.0.join("bin"))
            .env("HOME", self.0.join("user"))
            .env("XDG_RUNTIME_DIR", self.0.join("runtime"))
            .env("XDG_CACHE_HOME", self.0.join("cache"))
            .env("XDG_CONFIG_HOME", self.0.join("config"))
            .env("XDG_STATE_HOME", self.0.join("state"))
            .env("WAYLAND_DISPLAY", "fixture")
            .env("FLEA_UI", self.0.join("ui"))
            .env("FLEA_BIN", self.0.join("bin/flea"))
            .env("FLEA_TEST_TARGET", self.0.join("notes.txt"))
            .env("FLEA_TEST_OPEN_LOG", self.0.join("opened.env"))
            .env("FLEA_TEST_QS_MODE", "inspect")
            .env("QSG_RHI_BACKEND", "opengl")
            .env("VK_LOADER_DRIVERS_DISABLE", "*");
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.0.is_absolute()
            && fs::symlink_metadata(&self.0).is_ok_and(|m| m.is_dir())
            && fs::read(self.0.join(".flea-test-sandbox")).ok().as_deref()
                == Some(b"qs-crash-policy\n")
        {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn fatal_signal(picker: bool) {
    let sandbox = Sandbox::new();
    let mut command = sandbox.command();
    command.env("FLEA_TEST_QS_MODE", "crash");
    if picker {
        command
            .args(["--pick", sandbox.0.join("reply.json").to_str().unwrap()])
            .env("FLEA_PICKER", "{}");
    } else {
        command.arg("--gui");
    }
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        output.status.signal(),
        Some(SIGABRT),
        "picker={picker}: {stdout}"
    );
    assert_eq!(stdout, format!("frontend-pid={pid}\n"));
}

#[test]
fn a_fatal_signal_never_relaunches_a_main_window() {
    fatal_signal(false);
}

#[test]
fn a_fatal_signal_never_relaunches_a_picker() {
    fatal_signal(true);
}

#[test]
fn a_gpui_launch_does_not_install_a_quickshell_crash_policy() {
    let sandbox = Sandbox::new();
    let output = sandbox.command().arg("--gui").output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "||\n");
    // A GPUI launch clears a crash policy inherited from the old frontend.
    let output = sandbox
        .command()
        .arg("--gui")
        .env("QS_DISABLE_CRASH_HANDLER", "1")
        .env("FLEA_QS_CRASH_POLICY", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "||\n");
    for chosen in ["", "1", "0", "operator"] {
        let output = sandbox
            .command()
            .arg("--gui")
            .env("QS_DISABLE_CRASH_HANDLER", chosen)
            .env("FLEA_QS_CRASH_POLICY", "stale")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("x|{chosen}|\n")
        );
    }
}

#[test]
fn opening_a_program_or_gpui_drops_only_fleas_crash_policy() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .command()
        .arg("--open")
        .arg(sandbox.0.join("notes.txt"))
        .env("QS_DISABLE_CRASH_HANDLER", "1")
        .env("FLEA_QS_CRASH_POLICY", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(sandbox.0.join("opened.env")).unwrap(),
        "||\n"
    );
    for (disabled, marker, expected) in [
        ("1", "1", "||\n"),
        ("1", "", "x|1|\n"),
        ("", "stale", "x||\n"),
        ("operator", "stale", "x|operator|\n"),
    ] {
        let output = sandbox
            .command()
            .arg("--gui")
            .env("FLEA_FRONTEND", "gpui")
            .env("QS_DISABLE_CRASH_HANDLER", disabled)
            .env("FLEA_QS_CRASH_POLICY", marker)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }
}
