use super::*;
use flea::{
    i18n::{I18n, Paths},
    keys::presentation::*,
};

#[test]
fn sheet_hints_are_bounded_mirrored_and_use_the_active_preset() {
    let catalog = I18n::load(Paths {
        omarchy: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/i18n/fixtures/omarchy"),
        language_file: "/nonexistent/flea-g1-04-language".into(),
        catalogs: Vec::new(),
    });
    for row in keys::PRESET_KEYS.iter().chain(keys::SHARED_KEYS) {
        assert_ne!(catalog.tr(row.label_key), row.label_key);
    }
    for row in keys::POINTER {
        assert_ne!(catalog.tr(row.label_key), row.label_key);
    }
    let preview_focus = keys::PRESET_KEYS
        .iter()
        .find(|row| row.action == "focusPreview" && row.context == "preview,pdf,media")
        .unwrap();
    assert_eq!(catalog.tr(preview_focus.label_key), "Focus listing");
    let menu_parent = keys::PRESET_KEYS
        .iter()
        .find(|row| row.action == "parent" && row.context == "menu")
        .unwrap();
    assert_eq!(catalog.tr(menu_parent.label_key), "Parent");
    for preset in keys::PRESETS {
        for frontend in [Frontend::Gui, Frontend::Tui] {
            let rows = sheet(preset, frontend, &catalog);
            assert!(rows.len() > 60);
            for row in &rows {
                assert!(!row.label.is_empty());
                assert!(!row.label.starts_with("keys."), "{row:?}");
                if row.keys.contains(" / ") {
                    assert!(row.keys.len() <= keys::SHEET_CAP_BUDGET, "{row:?}");
                }
                assert!(keys::SHEET_GROUPS.iter().any(|group| group.id == row.group));
            }
            for pair in keys::SHEET_MIRRORS {
                let first = rows.iter().find(|row| row.action == pair[0]);
                let second = rows.iter().find(|row| row.action == pair[1]);
                // A pair bound in one frontend only is absent from the other's sheet, both members together.
                assert_eq!(first.is_some(), second.is_some(), "{preset}: {pair:?}");
                let (Some(first), Some(second)) = (first, second) else {
                    continue;
                };
                assert_eq!(
                    first.keys.contains(" / "),
                    second.keys.contains(" / "),
                    "{preset}: {pair:?}"
                );
            }
        }
    }
    assert_eq!(hint_for("default", "trash"), "d");
    for (preset, frontend, action, expected) in [
        ("default", Frontend::Tui, "copy", "y / ctrl-c"),
        ("windows", Frontend::Tui, "paste", "p / ctrl-v"),
        ("vim", Frontend::Tui, "copy", "yy / ctrl-insert"),
        ("mac", Frontend::Gui, "parent", "ctrl-up / left"),
    ] {
        assert_eq!(
            sheet(preset, frontend, &catalog)
                .iter()
                .find(|row| row.action == action)
                .unwrap()
                .keys,
            expected
        );
    }
    assert_eq!(hint_for("vim", "trash"), "D");
    assert_eq!(hint_for("mac", "rename"), "enter");
    assert_eq!(key_of("mac", "copy", "super"), "Super+C");
    assert_eq!(key_for("default", "missing"), "");
    assert_eq!(spelled("ctrl-shift-plus"), "Ctrl+Shift++");
    let rows = sheet("default", Frontend::Gui, &catalog);
    assert_eq!(
        rows.iter().find(|row| row.action == "menu").unwrap().keys,
        "m / shift-f10"
    );
    assert_eq!(
        rows.iter().find(|row| row.action == "parent").unwrap().keys,
        "h / left"
    );
    for (action, label) in [
        ("cursorDown", "Cursor down"),
        ("parent", "Parent folder"),
        ("pathBar", "Go to folder"),
        ("trash", "Move to Trash"),
        ("keymapSheet", "Keyboard Shortcuts"),
    ] {
        assert_eq!(
            rows.iter().find(|row| row.action == action).unwrap().label,
            label
        );
    }
}

#[test]
fn language_refresh_rebuilds_resolved_sheet_labels_and_keeps_fallback() {
    let root = root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("catalog/en")).unwrap();
    std::fs::write(
        root.join("catalog/en/flea.json"),
        r#"{"keys.test_hint":"%1 then %2"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("catalog/de/flea.json"),
        r#"{"keys.action.cursorDown":"Nach unten"}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "en").unwrap();
    let mut catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    let first = sheet("default", Frontend::Gui, &catalog);
    std::fs::write(root.join("language.next"), "de").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed());
    let second = sheet("default", Frontend::Gui, &catalog);
    assert_eq!(
        first
            .iter()
            .find(|row| row.action == "cursorDown")
            .unwrap()
            .label,
        "Cursor down"
    );
    assert_eq!(
        second
            .iter()
            .find(|row| row.action == "cursorDown")
            .unwrap()
            .label,
        "Nach unten"
    );
    assert_eq!(
        second
            .iter()
            .find(|row| row.action == "parent")
            .unwrap()
            .label,
        "Parent folder"
    );
    assert_eq!(
        first
            .iter()
            .map(|row| (&row.action, &row.keys))
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(|row| (&row.action, &row.keys))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hinted(&catalog, "keys.hint.filter", "default", "filter"),
        "Press / to filter this folder by name"
    );
    assert_eq!(
        hinted(&catalog, "keys.hint.filter", "default", "missing"),
        ""
    );
    assert_eq!(
        hinted_all(&catalog, "keys.test_hint", "default", &["copy", "paste"]),
        "y then p"
    );
    assert_eq!(
        hinted_all(&catalog, "keys.test_hint", "default", &["copy", "missing"]),
        ""
    );
}
