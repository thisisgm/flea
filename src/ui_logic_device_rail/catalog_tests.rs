//! The English mirror the rules render with matches the shipped catalog key for key.
use super::{ENGLISH, ENGLISH_PLURAL};
use crate::jsondoc::{parse, Json};

fn catalog() -> Json {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/i18n/en/flea.json");
    parse(&std::fs::read_to_string(path).expect("catalog readable")).expect("catalog parses")
}

#[test]
fn every_mirrored_sentence_is_the_catalog_text_or_its_omarchy_word() {
    let doc = catalog();
    let words = [
        ("omarchy:shell.common.cancel", "Cancel"),
        ("omarchy:shell.network.passphrase", "Passphrase"),
        ("omarchy:shell.dock.open", "Open"),
    ];
    for (key, english) in ENGLISH {
        let value = doc.get(key).and_then(Json::as_str).unwrap_or_else(|| panic!("{key} missing from the catalog"));
        let resolved = words.iter().find(|(r, _)| *r == value).map_or(value, |(_, w)| *w);
        assert_eq!(resolved, *english, "{key}");
    }
    for (key, one, other) in ENGLISH_PLURAL {
        let value = doc.get(key).unwrap_or_else(|| panic!("{key} missing from the catalog"));
        assert_eq!(value.get("one").and_then(Json::as_str), Some(*one), "{key} one");
        assert_eq!(value.get("other").and_then(Json::as_str), Some(*other), "{key} other");
    }
}

#[test]
fn every_catalog_key_under_the_device_prefix_is_mirrored() {
    let doc = catalog();
    let keys: Vec<&str> = doc.as_object().unwrap().iter().map(|(k, _)| k.as_str()).filter(|k| k.starts_with("device.")).collect();
    for key in &keys {
        let mirrored = ENGLISH.iter().any(|(k, _)| k == key) || ENGLISH_PLURAL.iter().any(|(k, _, _)| k == key);
        assert!(mirrored, "{key} has no mirror");
    }
    assert_eq!(keys.len(), ENGLISH.len() + ENGLISH_PLURAL.len());
}
