use super::facts::{self, FileFacts};
use super::wire::{self, State};
use crate::i18n::{I18n, Paths};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Every file this test writes stays in a marked mktemp root under the worktree's scratch directory.
struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g2-pv4");
        fs::create_dir_all(&base).unwrap();
        let out = Command::new("mktemp").arg("-d").arg(base.join("locale.XXXXXX")).output().unwrap();
        assert!(out.status.success());
        let path = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(&base) && path != base);
        fs::write(path.join(".pv4-test-root"), b"flea pv4 locale test\n").unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        assert!(self.0.join(".pv4-test-root").is_file());
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g2-pv4");
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty() && self.0.starts_with(&base) && self.0 != base);
        assert!(self.0.join(".pv4-test-root").is_file());
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn german_numbers_take_the_comma_and_an_untranslated_string_falls_back_to_english() {
    let root = Root::new();
    root.write("omarchy/default/i18n/locales.json", include_str!("../i18n/fixtures/locales.json"));
    let language = root.write("config/omarchy/language", "de\n");
    root.write("i18n/de/flea.json", r#"{"model3d.fact.dimensions":"Abmessungen","model3d.plate":"Platte %1"}"#);
    let i = I18n::load(Paths {
        omarchy: root.0.join("omarchy"),
        language_file: language,
        catalogs: vec![root.0.join("i18n"), crate::i18n::test_catalogs::english_only()],
    });
    assert_eq!(i.language(), "de");
    assert_eq!(facts::size_mm(&i, [60., 40., 40.]), "60,0 × 40,0 × 40,0 mm");
    assert_eq!(facts::layer_label(&i, 128, 240, 25.6), "Layer 128 of 240 · 25,60 mm");
    assert_eq!(facts::plate_words(&i, 2), ["Platte 1", "Platte 2"]);
    let State::Ready { data, .. } = wire::parse(super::tests::READY).unwrap().state else { panic!("ready") };
    let file = FileFacts { kind: "3MF", size: "1,2 MB", modified: "15.06.2025 10:00" };
    let rows = facts::rows(&i, file, &data, 0);
    assert_eq!(rows[0][3], ("Abmessungen".into(), "110,0 × 96,0 × 40,0 mm".into()));
    assert_eq!(rows[1][2], ("Layer height".into(), "0,20 mm".into()));
    assert!(rows.iter().flatten().all(|(l, v)| !l.contains("model3d.") && !v.contains("model3d.")));
}
