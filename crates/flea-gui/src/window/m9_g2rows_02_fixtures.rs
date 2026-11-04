//! Private filesystem and catalog fixtures shared by mounted search acceptance.
pub(super) fn fixture() -> std::path::PathBuf {
    // Keep the marked fixture beside the unit's evidence for inspection after a broker run.
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap()
        .join(".superpowers/m9-g2rows-02");
    std::fs::create_dir_all(&parent).unwrap();
    let output = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("search.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute() && root.starts_with(parent));
    std::fs::write(
        root.join(".flea-test-sandbox"),
        "Settings search acceptance\n",
    )
    .unwrap();
    root
}

pub(super) fn german(root: &std::path::Path) -> std::sync::Arc<flea::i18n::I18n> {
    let registry = root.join("omarchy/default/i18n");
    let catalog = root.join("i18n/de");
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(
        registry.join("locales.json"),
        r#"{"de":{"name":"Deutsch","formatLocale":"de-DE"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "de\n").unwrap();
    std::fs::write(catalog.join("flea.json"), r#"{"settings.group.thumbnails":"Vorschaubilder","settings.row.thumbnails":"Vorschaubilder","settings.row.thumb_size":"Vorschaubildgröße","settings.row.thumb_usb":"Auf USB-Laufwerken","settings.hint.thumbs_kept":"Vorhandene Bilder bleiben erhalten.","settings.menus.ext_thumbs":"Vorschaubilder anzeigen"}"#).unwrap();
    std::sync::Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("i18n")],
    }))
}
