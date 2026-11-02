use super::root;
use crate::i18n::{I18n, Paths};
use crate::ui_logic_fb1::empty::PHRASES;
use crate::ui_logic_fb1::errors::sentence;
use crate::ui_logic_fb1::messages::Messages;
use crate::ui_logic_fb1::reload_line;
use crate::ui_logic_fb1::status::Hint;

/// One witness for the unit's strings: German, a key German lacks, plural and arguments, and a live switch back.
#[test]
fn feedback_follows_a_live_language_change_with_per_string_fallback() {
    let root = root();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    );
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    root.file(
        "catalog/de/flea.json",
        r#"{"feedback.error.unread_named":"%1 konnte nicht gelesen werden.","feedback.reload":{"one":"Neu geladen · %1 Element geändert","other":"Neu geladen · %1 Elemente geändert"},"feedback.hint.undo":"%1 macht rückgängig","pane.empty.initial":"NOCH NICHTS HIER"}"#,
    );
    root.file("language", "de");
    let english = crate::i18n::test_catalogs::english_only();
    let mut catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog"), english],
    });
    let named = sentence("scan", "No such file or directory", "Fotos");
    assert_eq!(named.render(&catalog), "Fotos konnte nicht gelesen werden.");
    assert_eq!(reload_line(1, &catalog), "Neu geladen · 1 Element geändert");
    assert_eq!(
        reload_line(3, &catalog),
        "Neu geladen · 3 Elemente geändert"
    );
    assert_eq!(catalog.tr(PHRASES[0]), "NOCH NICHTS HIER");
    assert_eq!(catalog.tr(PHRASES[1]), "A VERY TIDY FOLDER");
    let backend = sentence("transfer", "the destination is not a folder", "");
    assert_eq!(backend.render(&catalog), "The destination is not a folder.");
    let mut bar = Messages::new("default");
    bar.say("Kopiert", false, "", Hint::Undo, 0);
    assert_eq!(bar.line(&catalog, "").undo, "z macht rückgängig");
    for key in [
        PHRASES[1],
        "feedback.error.generic",
        "feedback.hint.dismiss",
    ] {
        assert_ne!(catalog.tr(key), key, "{key} leaked");
    }
    std::fs::write(root.join("language.next"), "en").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed());
    assert_eq!(named.render(&catalog), "Fotos could not be read.");
    assert_eq!(bar.line(&catalog, "").undo, "z undoes");
}
