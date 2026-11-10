use flea::jsondoc::{self, Json};
use std::fs;
use std::io::Write;

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// A concurrent child launch can retain a copied executable's writable fd until exec.
static EXECUTABLE_FIXTURES: Mutex<()> = Mutex::new(());

// A sibling fork inherits any descriptor held open for writing, and exec then fails with ETXTBSY.
fn write_in_child(path: &Path, contents: &[u8]) {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("cat > \"$1\"")
        .arg("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(contents).unwrap();
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "writing {} failed: {status}",
        path.display()
    );
}

struct Sandbox {
    root: PathBuf,
    temporary: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g1-01");
        fs::create_dir_all(&parent).unwrap();
        let temporary = parent.canonicalize().unwrap();
        let sandbox_home = PathBuf::from(std::env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap();
        assert!(temporary.is_absolute() && temporary != sandbox_home);
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(temporary.join("flea-gpui-launch.XXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute() && !root.as_os_str().is_empty());
        assert_eq!(root.parent(), Some(temporary.as_path()));
        assert_eq!(root.canonicalize().unwrap(), root);
        assert!(root.is_dir());
        fs::write(root.join(".flea-test-sandbox"), "gpui launcher test\n").unwrap();
        write_in_child(
            &root.join("flea"),
            &fs::read(env!("CARGO_BIN_EXE_flea")).unwrap(),
        );
        flea::executable_file::write(
            &root.join("flea"),
            &std::fs::read(root.join("flea")).unwrap(),
            0o755,
        )
        .unwrap();
        fs::create_dir(root.join("files")).unwrap();
        Self { root, temporary }
    }

    fn removable(&self) -> bool {
        self.root.is_absolute()
            && !self.root.as_os_str().is_empty()
            && self.root.parent() == Some(self.temporary.as_path())
            && self
                .root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("flea-gpui-launch."))
            && self.root.canonicalize().is_ok_and(|root| root == self.root)
            && fs::symlink_metadata(&self.root).is_ok_and(|metadata| metadata.is_dir())
            && fs::symlink_metadata(self.root.join(".flea-test-sandbox"))
                .is_ok_and(|metadata| metadata.is_file())
    }

    fn frontend(&self) {
        let path = self.root.join("flea-gui");
        write_in_child(&path, r#"#!/usr/bin/python3
import json,os,sys
keys=('QSG_RHI_BACKEND','FLEA_RENDERER_AUTOMATIC','FLEA_VK_PIN','VK_DRIVER_FILES','VK_ICD_FILENAMES','FLEA_PATH','FLEA_SELECT','FLEA_UNLOCK')
print(json.dumps({'argv':sys.argv[1:],'env':{key:os.environ.get(key) for key in keys}}))
"#.as_bytes());
        flea::executable_file::write(&path, &std::fs::read(&path).unwrap(), 0o755).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = self.launch_command();
        command.arg("--gui");
        command
    }

    fn launch_command(&self) -> Command {
        let mut command = Command::new(self.root.join("flea"));
        command
            .env("FLEA_FRONTEND", "gpui")
            .env("WAYLAND_DISPLAY", "flea-test")
            .env("QT_QPA_PLATFORMTHEME", "")
            .env("FLEA_UI", self.root.join("no-ui"))
            .env("FLEA_BIN", self.root.join("flea"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("VK_DRIVER_FILES", "/operator-driver.json")
            .env("VK_ICD_FILENAMES", "/operator-icd.json")
            .env("VK_LOADER_DRIVERS_DISABLE", "operator-filter");
        for key in [
            "QSG_RHI_BACKEND",
            "FLEA_RENDERER_AUTOMATIC",
            "FLEA_VK_PIN",
            "FLEA_PATH",
            "FLEA_SELECT",
        ] {
            command.env_remove(key);
        }
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if !self.removable() {
            eprintln!("flea test sandbox cleanup refused {}", self.root.display());
            return;
        }
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!(
                "flea test sandbox cleanup failed for {}: {error}",
                self.root.display()
            );
        }
    }
}

fn reply(output: &Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    jsondoc::parse(std::str::from_utf8(&output.stdout).unwrap()).unwrap()
}

#[test]
fn unlock_launch_uses_gpui_without_qml_and_forgets_the_parent_request() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    let output = sandbox
        .launch_command()
        .args(["--unlock", "/dev/mapper/vault"])
        .env("FLEA_UNLOCK", "/dev/stale-parent")
        .output()
        .unwrap();
    let document = reply(&output);
    assert_eq!(document.get("argv"), Some(&Json::Arr(Vec::new())));
    assert_eq!(
        document
            .get("env")
            .unwrap()
            .get("FLEA_UNLOCK")
            .and_then(Json::as_str),
        Some("/dev/mapper/vault")
    );
    let output = sandbox
        .command()
        .env("FLEA_UNLOCK", "/dev/stale-parent")
        .output()
        .unwrap();
    assert_eq!(
        reply(&output).get("env").unwrap().get("FLEA_UNLOCK"),
        Some(&Json::Null)
    );
}

#[test]
fn unlock_launch_rejects_invalid_or_headless_requests() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    for device in ["/etc/passwd", "/dev/../etc/passwd"] {
        let output = sandbox
            .launch_command()
            .args(["--unlock", device])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let output = sandbox
        .launch_command()
        .args(["--unlock", "/dev/sdb1"])
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no graphical session"));
    assert!(output.stdout.is_empty());
}

#[test]
fn gpui_skips_qt_renderer_switches_and_diagnostics() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    for renderer in [None, Some("opengl"), Some("vulkan")] {
        let mut command = sandbox.command();
        if let Some(renderer) = renderer {
            command.env("QSG_RHI_BACKEND", renderer);
        }
        let output = command
            .env("FLEA_RENDERER_AUTOMATIC", "1")
            .output()
            .unwrap();
        let document = reply(&output);
        let environment = document.get("env").unwrap();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for key in ["QSG_RHI_BACKEND", "FLEA_RENDERER_AUTOMATIC", "FLEA_VK_PIN"] {
            assert_eq!(
                environment.get(key),
                Some(&Json::Null),
                "{key}: {renderer:?}"
            );
        }
        assert_eq!(
            environment.get("VK_DRIVER_FILES").and_then(Json::as_str),
            Some("/operator-driver.json")
        );
        assert_eq!(
            environment.get("VK_ICD_FILENAMES").and_then(Json::as_str),
            Some("/operator-icd.json")
        );
    }
    let software = sandbox.root.join("config/vulkan/icd.d/software.json");
    fs::create_dir_all(software.parent().unwrap()).unwrap();
    fs::write(&software, r#"{"ICD":{"library_path":"libvulkan_lvp.so"}}"#).unwrap();
    let output = sandbox
        .command()
        .env("QSG_RHI_BACKEND", "opengl")
        .env("FLEA_RENDERER_AUTOMATIC", "1")
        .env("VK_LOADER_DRIVERS_DISABLE", "*")
        .env("FLEA_VK_PIN", "1")
        .output()
        .unwrap();
    let document = reply(&output);
    let environment = document.get("env").unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for key in ["QSG_RHI_BACKEND", "FLEA_RENDERER_AUTOMATIC"] {
        assert_eq!(environment.get(key), Some(&Json::Null), "{key}");
    }
    // GPUI picks its own adapter: an inherited launcher pin is dropped and no software ICD is added.
    for key in ["FLEA_VK_PIN", "VK_DRIVER_FILES", "VK_ICD_FILENAMES"] {
        assert_eq!(environment.get(key), Some(&Json::Null), "{key}");
    }
}

#[test]
fn gpui_path_and_selection_use_argv_despite_stale_parent_environment() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    let selected = sandbox.root.join("files/a b");
    let output = sandbox
        .command()
        .env("QSG_RHI_BACKEND", "opengl")
        .env("FLEA_PATH", "/stale-parent/path")
        .env("FLEA_SELECT", "/stale-parent/selection")
        .arg("--select")
        .arg(&selected)
        .arg(sandbox.root.join("ignored"))
        .output()
        .unwrap();
    let document = reply(&output);
    let arguments: Vec<_> = document
        .get("argv")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(
        arguments,
        [
            "--select",
            selected.to_str().unwrap(),
            "--",
            sandbox.root.join("files").to_str().unwrap()
        ]
    );
    for key in ["FLEA_PATH", "FLEA_SELECT"] {
        assert_eq!(document.get("env").unwrap().get(key), Some(&Json::Null));
    }
    let output = sandbox
        .command()
        .env("QSG_RHI_BACKEND", "opengl")
        .env("FLEA_PATH", "/stale-parent/path")
        .env("FLEA_SELECT", "/stale-parent/selection")
        .output()
        .unwrap();
    let document = reply(&output);
    let arguments: Vec<_> = document
        .get("argv")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(
        arguments.is_empty(),
        "an unnamed launch must let saved startup settings choose the path"
    );
    for key in ["FLEA_PATH", "FLEA_SELECT"] {
        assert_eq!(document.get("env").unwrap().get(key), Some(&Json::Null));
    }
    let start = sandbox.root.join("files");
    let output = sandbox.command().arg(&start).output().unwrap();
    let document = reply(&output);
    assert_eq!(
        document.get("argv"),
        Some(&Json::Arr(vec![
            Json::Str("--".into()),
            Json::Str(start.to_string_lossy().into_owned())
        ]))
    );
}

#[test]
fn gpui_exec_errors_name_the_program_and_os_reason() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let denied = Sandbox::new();
    let frontend = denied.root.join("flea-gui");
    write_in_child(&frontend, b"not executable\n");
    let output = denied
        .command()
        .env("QSG_RHI_BACKEND", "opengl")
        .output()
        .unwrap();
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert!(error.contains(frontend.to_str().unwrap()), "{error}");
    assert!(
        error.contains(&std::io::Error::from_raw_os_error(13).to_string()),
        "{error}"
    );
    let missing = Sandbox::new();
    let output = missing
        .command()
        .env("QSG_RHI_BACKEND", "opengl")
        .env("PATH", missing.root.join("empty-path"))
        .output()
        .unwrap();
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert!(error.contains("flea-gui"), "{error}");
    assert!(
        error.contains(&std::io::Error::from_raw_os_error(2).to_string()),
        "{error}"
    );
}

#[path = "gpui_launcher/sandbox.rs"]
mod sandbox;

#[path = "gpui_launcher/cutover.rs"]
mod cutover;

#[path = "gpui_launcher/prefetch.rs"]
mod prefetch;
