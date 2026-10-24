use super::{read_rounding, rounding};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let parent = std::env::current_dir()
            .unwrap()
            .join(".superpowers/m9-corner");
        std::fs::create_dir_all(&parent).unwrap();
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("tui.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(parent));
        std::fs::write(path.join(".flea-test-sandbox"), "TUI rounding controls\n").unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let parent = std::env::current_dir()
            .unwrap()
            .join(".superpowers/m9-corner");
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(parent)
                && self.0.join(".flea-test-sandbox").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn m9_corner_tui_missing_hyprctl_is_square() {
    let root = Fixture::new();
    assert!(!read_rounding(&mut Command::new(
        root.0.join("missing-hyprctl")
    )));
}

#[test]
fn m9_corner_tui_nonzero_exit_is_square() {
    let root = Fixture::new();
    let program = root.0.join("hyprctl");
    std::fs::write(&program, "#!/bin/sh\nprintf '{\"int\":8}'\nexit 1\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!read_rounding(&mut Command::new(program)));
}

#[test]
fn m9_corner_tui_garbage_json_is_square() {
    assert!(!rounding("not json"));
}

#[test]
fn m9_corner_tui_negative_is_square() {
    assert!(!rounding(r#"{"int":-8}"#));
}

#[test]
fn m9_corner_tui_nan_and_infinite_are_square() {
    for answer in [r#"{"int":NaN}"#, r#"{"int":1e400}"#, r#"{"int":"NaN"}"#] {
        assert!(!rounding(answer));
    }
}

#[test]
fn m9_corner_tui_unset_is_square() {
    for answer in ["{}", r#"{"int":8,"set":false}"#] {
        assert!(!rounding(answer));
    }
}

#[test]
fn m9_corner_tui_zero_is_square() {
    assert!(!rounding(r#"{"int":0,"set":true}"#));
}

#[test]
fn m9_corner_tui_eight_is_rounded() {
    assert!(rounding(r#"{"int":8,"set":true}"#));
}

#[test]
fn m9_corner_tui_successful_command_is_rounded() {
    let root = Fixture::new();
    let program = root.0.join("hyprctl");
    std::fs::write(&program, "#!/bin/sh\nprintf '{\"int\":8}'\nexit 0\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(read_rounding(&mut Command::new(program)));
}
