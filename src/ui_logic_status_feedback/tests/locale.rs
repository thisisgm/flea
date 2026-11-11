use super::root;
use crate::i18n::{I18n, Paths};
use crate::ui_logic_status_feedback::empty::PHRASES;
use crate::ui_logic_status_feedback::errors::{extract_refusal, sentence};
use crate::ui_logic_status_feedback::messages::Messages;
use crate::ui_logic_status_feedback::reload_line;
use crate::ui_logic_status_feedback::status::Hint;
use crate::ui_logic_file_ops::lines::{transfer_failure, trashed};

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

/// The shipped German catalog says the extract refusals, a trash cause and a drive's name refusals the backend words in English.
#[test]
fn new_extract_refusals_and_a_trash_cause_read_in_the_shipped_german() {
    let root = root();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../../i18n/fixtures/locales.json"),
    );
    for (code, text) in [
        ("en", include_str!("../../../i18n/en/flea.json")),
        ("de", include_str!("../../../i18n/de/flea.json")),
    ] {
        std::fs::create_dir_all(root.join(format!("catalog/{code}"))).unwrap();
        root.file(&format!("catalog/{code}/flea.json"), text);
    }
    root.file("language", "de");
    let german = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    let refused = |message: &str| extract_refusal(message).unwrap().render(&german);
    assert_eq!(
        refused("'clip/Readme' and 'clip/README' would take one name on this drive, so nothing was extracted"),
        "„clip/Readme“ und „clip/README“ hätten auf diesem Laufwerk denselben Namen, daher wurde nichts entpackt."
    );
    assert_eq!(
        refused("'set/clip: final.txt' cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive"),
        "„set/clip: final.txt“ kann nicht auf dieses Laufwerk entpackt werden: „:“ ist in einem Namen auf diesem vfat-Laufwerk nicht erlaubt."
    );
    assert_eq!(
        trashed(&german, "default", 0, 1, "the drive is read-only"),
        "Das Element konnte nicht in den Papierkorb verschoben werden: Das Laufwerk ist schreibgeschützt."
    );
    // A drive's name refusal reads whole in German wherever rename, mkdir, paste or extract says it.
    for (place, message, german_words) in [
        ("rename", "':' is not allowed in a name on this vfat drive", "„:“ ist in einem Namen auf diesem vfat-Laufwerk nicht erlaubt."),
        ("mkdir", "a name cannot hold control character U+0001 on this exfat drive", "Ein Name darf auf diesem exfat-Laufwerk kein Steuerzeichen U+0001 enthalten."),
        ("rename", "a name cannot end in a dot on this vfat drive", "Ein Name darf auf diesem vfat-Laufwerk nicht mit einem Punkt enden."),
        ("rename", "a name cannot end in a space on this exfat drive", "Ein Name darf auf diesem exfat-Laufwerk nicht mit einem Leerzeichen enden."),
        ("mkdir", "'CON' is reserved on this ntfs3 drive", "„CON“ ist auf diesem ntfs3-Laufwerk ein reservierter Name."),
    ] {
        assert_eq!(sentence(place, message, "").render(&german), german_words);
    }
    assert_eq!(
        transfer_failure(&german, false, "a?b", "'?' is not allowed in a name on this exfat drive"),
        "Kopieren fehlgeschlagen: a?b · „?“ ist in einem Namen auf diesem exfat-Laufwerk nicht erlaubt"
    );
}
