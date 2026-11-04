//! A marked scratch root for fb1 tests that write files, made by mktemp under .superpowers as g1_04.rs Root is.
use flea::i18n::{I18n, Paths};
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-fb1-tests")
}

/// A fresh root that removes itself when its test passes and stays behind for a failure.
pub(super) struct Root(PathBuf);

impl std::ops::Deref for Root {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
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

/// Japanese whose first beat is mixed-case Latin: a caseless language must draw it as written, never uppercased.
pub(super) fn caseless(root: &Path) -> I18n {
    std::fs::create_dir_all(root.join("catalog/ja")).unwrap();
    std::fs::write(
        root.join("catalog/ja/flea.json"),
        format!(r#"{{"pane.empty.initial":"{CASELESS_BEAT}"}}"#),
    )
    .unwrap();
    fallback(root, "ja")
}

/// `code` with no catalog of its own, so the text is the Latin fallback and only the language decides case and tracking.
pub(super) fn fallback(root: &Path, code: &str) -> I18n {
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        format!(r#"{{"en":{{}},"{code}":{{"name":"{code}"}}}}"#),
    )
    .unwrap();
    std::fs::write(root.join("language"), code).unwrap();
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog"), crate::test_catalogs::english_only()],
    })
}

/// The caseless catalog's first beat, in mixed case so an uppercased draw shows.
pub(super) const CASELESS_BEAT: &str = "Nothing here yet";
