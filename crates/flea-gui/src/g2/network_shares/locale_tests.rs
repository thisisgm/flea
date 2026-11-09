use super::{store_tests::Root, words::Words};
use flea::i18n::{I18n, Paths};
use std::fs;

#[test]
fn pl2_live_language_fallback_arguments_and_locale_headings() {
    let root = Root::new();
    let omarchy = root.0.join("omarchy");
    fs::create_dir_all(omarchy.join("default/i18n/en")).unwrap();
    fs::write(
        omarchy.join("default/i18n/en/shell.json"),
        r#"{"common.cancel":"Cancel"}"#,
    )
    .unwrap();
    fs::write(
        omarchy.join("default/i18n/locales.json"),
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src/i18n/fixtures/locales.json"
        )),
    )
    .unwrap();
    let catalogs = root.0.join("catalogs");
    fs::create_dir_all(catalogs.join("de")).unwrap();
    fs::create_dir_all(catalogs.join("tr")).unwrap();
    fs::write(
        catalogs.join("de/flea.json"),
        r#"{"network.title":"Netzwerkfreigabe hinzufügen","network.host_caption":"%1 bei %2"}"#,
    )
    .unwrap();
    fs::write(
        catalogs.join("tr/flea.json"),
        r#"{"network.username":"isim"}"#,
    )
    .unwrap();
    let language = root.0.join("language");
    fs::write(&language, "de\n").unwrap();
    let mut catalog = I18n::load(Paths {
        omarchy,
        language_file: language.clone(),
        catalogs: vec![catalogs, crate::test_catalogs::english_only()],
    });
    let words = Words::new(&catalog);
    assert_eq!(words.title, "Netzwerkfreigabe hinzufügen");
    assert_eq!(words.fields[2], "PORT");
    assert_eq!(
        catalog.tr_args("network.host_caption", &["SMB", "nas.local"]),
        "SMB bei nas.local"
    );
    assert_eq!(
        catalog.tr_args("network.mounted_as", &["other-user"]),
        "Could not connect: that share is already mounted as other-user."
    );
    fs::write(&language, "tr\n").unwrap();
    assert!(catalog.poll_changed());
    assert_eq!(Words::new(&catalog).fields[5], "İSİM");
    fs::write(&language, "en\n").unwrap();
    assert!(catalog.poll_changed());
    assert_eq!(Words::new(&catalog).title, "Add a Network Share");
    for key in [
        "network.title",
        "network.connect",
        "network.invalid_form",
        "network.no_prompt",
    ] {
        assert_ne!(catalog.tr(key), key);
    }
}
