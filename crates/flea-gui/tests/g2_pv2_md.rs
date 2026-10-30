#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[path = "g2_pv2_md/harness.rs"]
mod harness;
#[path = "g2_pv2_md/rendered.rs"]
mod rendered;
#[path = "g2_pv2_md/source.rs"]
mod source;

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-pv2-md")
}

/// A marked scratch root that removes itself when its test passes and stays behind for a failure.
pub struct Root(PathBuf);

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

pub fn root() -> Root {
    std::fs::create_dir_all(scratch()).unwrap();
    // Canonical, so no path a test hands the view holds "..", which the picture reader refuses.
    let parent = scratch().canonicalize().unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g2-pv2-md\n").unwrap();
    Root(root)
}

/// Writes a file into the root and answers its path.
pub fn file(root: &Root, name: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}
