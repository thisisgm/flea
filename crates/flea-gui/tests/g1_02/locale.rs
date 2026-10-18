use flea::i18n::{I18n, Paths};
use flea_gui::pane::states::{Kind, State};

#[test]
fn pane_catalog_keys_arguments_plurals_and_language_refresh_keep_english_fallback() {
    let root = super::transport::root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "en").unwrap();
    std::fs::write(root.join("catalog/de/flea.json"), r#"{"pane.reading":"Lesen","pane.permissions_other_owner":"%1 · fremd","items.count":{"one":"%1 Eintrag","other":"%1 Einträge"}}"#).unwrap();
    let mut catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    let mut state = State::default();
    assert_eq!(state.text(&catalog, 0).message, "reading");
    std::fs::write(root.join("language.next"), "de\n").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed());
    assert_eq!(state.text(&catalog, 0).message, "Lesen");
    state.kind = Kind::Locked;
    state.mode = 0o40700;
    let text = state.text(&catalog, 0);
    assert_eq!(text.detail, "rwx------ · fremd");
    assert_eq!(
        text.message,
        "You don’t have permission to open this folder."
    );
    state.kind = Kind::Ready;
    assert_eq!(state.text(&catalog, 1).status, "1 Eintrag");
    assert_eq!(state.text(&catalog, 2).status, "2 Einträge");
}
