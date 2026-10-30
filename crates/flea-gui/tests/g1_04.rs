#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use flea::keys::{self, Context, Frontend, Input, Match};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[path = "g1_04/allocations.rs"]
mod allocations;
#[path = "g1_04/chords.rs"]
mod chords;
#[path = "g1_04/dispatch.rs"]
mod dispatch;
#[path = "g1_04/generator.rs"]
mod generator;
#[path = "g1_04/keypad.rs"]
mod keypad;
#[path = "g1_04/presentation.rs"]
mod presentation;
#[path = "g1_04/vectors.rs"]
mod vectors;

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-04")
}

/// A marked scratch root that removes itself when its test passes and stays behind for a failure.
struct Root(PathBuf);

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

fn root() -> Root {
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
    std::fs::write(root.join(".flea-test-sandbox"), "g1-04\n").unwrap();
    Root(root)
}

fn action(name: &str, context: Context, key: &str, text: &str, modifiers: u8) -> &'static str {
    keys::lookup(
        name,
        context,
        Frontend::Gui,
        Input {
            key,
            text,
            modifiers,
        },
    )
    .action()
}
