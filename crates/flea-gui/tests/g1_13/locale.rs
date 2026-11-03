//! The status host's catalog keys: plural selection sentence, size join, English fallback and language change.
use super::root;
use flea::i18n::{I18n, Paths};
use flea_gui::strips::status_host;
use std::path::{Path, PathBuf};

fn catalog(language: &str) -> (PathBuf, I18n) {
    let root = root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), format!("{language}\n")).unwrap();
    let de = r#"{"status.host.selected":{"one":"%1 von %2 Eintrag ausgewählt","other":"%1 von %2 Einträgen ausgewählt"},"status.host.selected_size":"%1 (%2)","status.host.free":"%1 · %2 frei"}"#;
    std::fs::write(root.join("catalog/de/flea.json"), de).unwrap();
    let catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    (root, catalog)
}

fn switch_to_german(root: &Path, catalog: &mut I18n) {
    std::fs::write(root.join("language.next"), "de\n").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed(), "the language file changing is seen");
}

#[test]
fn the_selection_head_takes_its_plural_form_and_size_join_from_the_catalog() {
    let (root, mut i18n) = catalog("en");
    assert_eq!(
        status_host::selection_head(&i18n, 1, 9, -1, ""),
        "1 of 9 selected"
    );
    assert_eq!(
        status_host::selection_head(&i18n, 3, 9, 609, "609 B"),
        "3 of 9 selected · 609 B"
    );
    switch_to_german(&root, &mut i18n);
    assert_eq!(
        status_host::selection_head(&i18n, 1, 9, -1, ""),
        "1 von 9 Eintrag ausgewählt"
    );
    assert_eq!(
        status_host::selection_head(&i18n, 3, 9, 609, "609 B"),
        "3 von 9 Einträgen ausgewählt (609 B)"
    );
}

#[test]
fn a_status_key_the_translation_lacks_falls_back_to_english() {
    let (_root, i18n) = catalog("de");
    assert_eq!(
        status_host::selection_head(&i18n, 1, 9, -1, ""),
        "1 von 9 Eintrag ausgewählt"
    );
    assert_eq!(status_host::reloaded(&i18n, 1), "Reloaded · 1 item changed");
    assert_eq!(status_host::disk(&i18n, "", None, false), "unknown");
}
