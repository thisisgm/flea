use flea::jsondoc::{self, Json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let temporary = PathBuf::from("/tmp").canonicalize().unwrap();
        let sandbox_home = PathBuf::from(std::env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap();
        assert!(temporary.is_absolute() && !temporary.starts_with(sandbox_home));
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
        fs::copy(env!("CARGO_BIN_EXE_flea"), root.join("flea")).unwrap();
        fs::create_dir(root.join("files")).unwrap();
        Self { root }
    }

    fn frontend(&self) {
        let path = self.root.join("flea-gui");
        fs::write(&path, r#"#!/usr/bin/python3
import json,os,sys
keys=('QSG_RHI_BACKEND','FLEA_RENDERER_AUTOMATIC','FLEA_VK_PIN','VK_DRIVER_FILES','VK_ICD_FILENAMES','FLEA_PATH','FLEA_SELECT')
print(json.dumps({'argv':sys.argv[1:],'env':{key:os.environ.get(key) for key in keys}}))
"#).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.root.join("flea"));
        command
            .arg("--gui")
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

fn reply(output: &Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    jsondoc::parse(std::str::from_utf8(&output.stdout).unwrap()).unwrap()
}

#[test]
fn gpui_skips_qt_renderer_switches_and_diagnostics() {
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
    let output = sandbox
        .command()
        .env("QSG_RHI_BACKEND", "opengl")
        .env("FLEA_VK_PIN", "1")
        .output()
        .unwrap();
    let document = reply(&output);
    let environment = document.get("env").unwrap();
    for key in ["FLEA_VK_PIN", "VK_DRIVER_FILES", "VK_ICD_FILENAMES"] {
        assert_eq!(environment.get(key), Some(&Json::Null));
    }
}

#[test]
fn gpui_path_and_selection_use_argv_despite_stale_parent_environment() {
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
    let home = std::env::var("HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/".into());
    assert_eq!(arguments, ["--", home.as_str()]);
    for key in ["FLEA_PATH", "FLEA_SELECT"] {
        assert_eq!(document.get("env").unwrap().get(key), Some(&Json::Null));
    }
}

#[test]
fn gpui_exec_errors_name_the_program_and_os_reason() {
    let denied = Sandbox::new();
    let frontend = denied.root.join("flea-gui");
    fs::write(&frontend, "not executable\n").unwrap();
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
