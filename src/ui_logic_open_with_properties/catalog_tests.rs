// Every key this unit draws resolves, and the German witness falls back per string and formats with its own comma.
use super::{english, language};

fn used_keys() -> Vec<String> {
    let sources = [
        include_str!("openwith.rs"),
        include_str!("permissions.rs"),
        include_str!("permissions/skips.rs"),
        include_str!("bulkrename.rs"),
        include_str!("runfile.rs"),
        include_str!("opener.rs"),
        include_str!("properties.rs"),
        include_str!("viewerorder.rs"),
    ];
    let mut keys: Vec<String> = Vec::new();
    for source in sources {
        for piece in source.split('"').skip(1).step_by(2) {
            if piece.starts_with("dialogs.file.") && !keys.iter().any(|k| k == piece) {
                keys.push(piece.to_string());
            }
        }
    }
    keys
}

#[test]
fn every_key_the_unit_draws_is_in_the_english_catalog() {
    let i18n = english();
    let keys = used_keys();
    assert!(keys.len() > 80, "the scan found the unit's keys");
    for key in keys {
        assert_ne!(i18n.tr(&key), key, "{key} leaks as a key");
    }
    assert_eq!(
        i18n.tr("dialogs.file.cancel"),
        "Cancel",
        "Cancel is Omarchy's own word"
    );
    assert_eq!(i18n.tr("dialogs.file.apply"), "Apply");
}

#[test]
fn german_takes_omarchys_cancel_falls_back_per_string_and_counts_with_its_comma() {
    let de = language("language-de");
    assert_eq!(de.language(), "de");
    assert_eq!(
        de.tr("dialogs.file.cancel"),
        "Abbrechen",
        "Cancel is Omarchy's own Abbrechen"
    );
    assert_eq!(
        de.tr("dialogs.file.properties.title"),
        "Properties",
        "an untranslated string falls back to English"
    );
    assert_eq!(
        super::runfile::subtitle(&de, super::runfile::Kind::Script, 22_100),
        "Shell script \u{b7} 22,1 kB"
    );
    assert_eq!(super::bulkrename::rename_label(&de, 1204), "Rename 1.204");
    let note = super::permissions::skip_note(
        &de,
        &[super::permissions::Skip {
            path: "/d/a\u{202e}b".into(),
            why: "Gone.".into(),
        }],
    );
    assert_eq!(
        note, "a\u{202e}b keeps its mode: Gone.",
        "an opaque backend reason keeps its identity"
    );
}
