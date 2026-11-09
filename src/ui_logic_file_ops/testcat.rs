// An English catalog over a marked mktemp root holding Omarchy's roster and the two shell words the cards reuse.
use crate::i18n::{I18n, Paths};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct Catalog {
    root: PathBuf,
    pub(crate) i18n: I18n,
}

fn base() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g2-op1/core-tests")
}

pub(crate) const SHELL: &str = r#"{"common.cancel": "Cancel", "common.delete": "Delete"}"#;

impl Catalog {
    pub(crate) fn english() -> Self {
        Self::with_language("en")
    }

    pub(crate) fn with_language(code: &str) -> Self {
        let base = base();
        fs::create_dir_all(&base).unwrap();
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(base.join("cat.XXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute() && root.starts_with(&base) && root != base);
        fs::write(root.join(".op1-test-root"), b"op1\n").unwrap();
        let i18n_dir = root.join("omarchy/default/i18n");
        fs::create_dir_all(i18n_dir.join("en")).unwrap();
        fs::write(
            i18n_dir.join("locales.json"),
            include_str!("../i18n/fixtures/locales.json"),
        )
        .unwrap();
        fs::write(i18n_dir.join("en/shell.json"), SHELL).unwrap();
        fs::create_dir_all(root.join("config/omarchy")).unwrap();
        fs::write(root.join("config/omarchy/language"), format!("{code}\n")).unwrap();
        let i18n = I18n::load(Paths {
            omarchy: root.join("omarchy"),
            language_file: root.join("config/omarchy/language"),
            catalogs: vec![root.join("catalogs")],
        });
        Self { root, i18n }
    }

    pub(crate) fn write_catalog(&mut self, code: &str, text: &str) {
        assert!(self.root.join(".op1-test-root").is_file());
        let dir = self.root.join("catalogs").join(code);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("flea.json"), text).unwrap();
        self.i18n.reload();
    }
}

impl Drop for Catalog {
    fn drop(&mut self) {
        let base = base();
        assert!(self.root.is_absolute() && !self.root.as_os_str().is_empty());
        assert!(self.root.starts_with(&base) && self.root != base);
        assert!(self.root.join(".op1-test-root").is_file());
        fs::remove_dir_all(&self.root).unwrap();
    }
}
