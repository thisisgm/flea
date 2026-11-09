//! Ports of tests/js/errors.js, status.js, statusfix.js and motion.js, tests/emptybeats.qml and the message half of tests/statusbar-hint.qml.
mod empty;
mod errors;
mod lane;
mod locale;
mod messages;
mod motion;
mod status;

use crate::i18n::{I18n, Paths};
use std::path::{Path, PathBuf};
use std::process::Command;

/// English from the source tree alone, whatever language the build host has chosen.
pub(super) fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: vec![crate::i18n::test_catalogs::english_only()],
    })
}

fn scratch() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g2-fb1-tests")
}

/// A marked scratch root made by mktemp under .superpowers, as g1_04.rs Root is; removed only when its test passes.
pub(super) struct Root(PathBuf);

impl std::ops::Deref for Root {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Root {
    /// Writes `body` to `name` inside the root.
    pub(super) fn file(&self, name: &str, body: &str) {
        std::fs::write(self.0.join(name), body).unwrap();
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let parent = scratch().canonicalize().unwrap();
        let ours = self.0.is_absolute()
            && self.0.file_name().is_some()
            && self.0.join(".flea-test-sandbox").is_file()
            && self.0.parent().and_then(|path| path.canonicalize().ok()) == Some(parent);
        if ours && !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

/// mktemp -d makes the directory new under the scratch parent; the marker is the first thing written in it.
pub(super) fn root() -> Root {
    let parent = scratch();
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g2-fb1\n").unwrap();
    Root(root)
}
