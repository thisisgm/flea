//! Every key this unit draws resolves, and the German witness falls back per string.
use super::{english, language};

fn used_keys() -> Vec<String> {
    let sources = [
        include_str!("archive.rs"),
        include_str!("cards.rs"),
        include_str!("flash.rs"),
        include_str!("flashcard.rs"),
        include_str!("flashendings.rs"),
        include_str!("ops.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/cards.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/compress.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/draw.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/draw_flash.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/extract.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/flash/mod.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/flash/run.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/flash/view.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/keys.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/mod.rs"),
    ];
    let mut keys: Vec<String> = Vec::new();
    for source in sources {
        for piece in source.split('"').skip(1).step_by(2) {
            if piece.starts_with("archive.") && !keys.iter().any(|k| k == piece) {
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
    assert!(keys.len() > 60, "the scan found the unit's keys");
    for key in keys {
        assert_ne!(i18n.tr(&key), key, "{key} leaks as a key");
    }
    assert_eq!(i18n.tr("archive.cancel"), "Cancel", "Cancel is Omarchy's own word");
}

#[test]
fn byte_line_templates_fill_by_position() {
    let i18n = english();
    for key in [
        "archive.extracting.progress",
        "archive.extracting.progress_no_total",
        "archive.flash.flush",
        "archive.flash.known",
        "archive.flash.known_stalled",
        "archive.flash.unknown",
        "archive.flash.unknown_stalled",
    ] {
        let template = i18n.tr(key);
        assert_ne!(template, key, "{key} resolves");
        assert!(!template.contains("{f:"), "{key} names no brace hole: {template}");
        assert!(template.contains('%'), "{key} fills by position: {template}");
    }
}

#[test]
fn every_status_line_is_a_whole_sentence() {
    let i18n = english();
    for key in [
        "archive.flash.verify_cancelled_line",
        "archive.flash.prewrite_line",
        "archive.flash.cancelled_at_unknown_line",
        "archive.flash.cancelled_at_line",
        "archive.flash.unmount_line",
        "archive.flash.changed_line",
        "archive.flash.unplugged_line",
        "archive.flash.io_line",
        "archive.flash.too_big_line",
        "archive.flash.no_unpack_line",
        "archive.flash.need_bubble_line",
        "archive.flash.no_open_line",
    ] {
        assert_ne!(i18n.tr(key), key, "{key} resolves as one sentence");
    }
}

#[test]
fn gpui_cards_draw_no_raw_user_visible_strings() {
    for source in [
        include_str!("../../crates/flea-gui/src/g2/archive_flash/draw.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/draw_flash.rs"),
        include_str!("../../crates/flea-gui/src/g2/archive_flash/flash/view.rs"),
    ] {
        for literal in ["\"Image\"", "\"Drives\"", "\"Plug in a USB drive or SD card\""] {
            assert!(!source.contains(literal), "the catalog owns the string, not the view: {literal}");
        }
    }
    let draw = include_str!("../../crates/flea-gui/src/g2/archive_flash/draw.rs");
    assert!(!draw.contains("section(&draw, \"Flash\")"), "the Flash section resolves through the catalog");
    let flash = include_str!("../../crates/flea-gui/src/g2/archive_flash/draw_flash.rs");
    assert!(!flash.contains("section(&draw, \"Image\")"), "the flash sections resolve through the catalog");
}

#[test]
fn german_counts_the_unknown_index_head() {
    let de = language("language-de");
    assert_eq!(de.trn("archive.extracting.head_unknown_count", 1), "Extracting 1 item");
    assert_eq!(de.trn("archive.extracting.head_unknown_count", 11), "Extracting 11 items");
}

#[test]
fn german_takes_omarchys_cancel_falls_back_per_string_and_counts_with_its_comma() {
    let de = language("language-de");
    assert_eq!(de.language(), "de");
    assert_eq!(de.tr("archive.cancel"), "Abbrechen", "Cancel is Omarchy's own Abbrechen");
    assert_eq!(de.tr("archive.compress.action"), "Compress", "untranslated Flea keys fall back per string");
    assert_eq!(de.trn("archive.compress.title", 1), "Compress 1 Item");
    let size = de.locale().format_size(22100);
    assert_eq!(size, "22,1 kB", "sizes take the German decimal comma");
}
