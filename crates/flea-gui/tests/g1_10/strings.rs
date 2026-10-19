use super::*;

const MENU: [(&str, &str); 6] = [
    ("tabs.home", "Home"),
    ("tabs.new", "New Tab"),
    ("tabs.duplicate", "Duplicate Tab"),
    ("tabs.move_to_window", "Move Tab to New Window"),
    ("tabs.close", "Close Tab"),
    ("tabs.close_others", "Close Other Tabs"),
];

fn verdicts() -> [(Say, &'static str); 8] {
    [
        (Say::NoTab(3), "No tab 3"),
        (Say::LastTab, "The last tab cannot be closed"),
        (Say::Most, "Nine tabs is the most."),
        (Say::OnlyFolder, "Only a folder opens in a new tab."),
        (Say::NotReceived, "That tab could not be received."),
        (Say::FolderGone, "That folder is no longer there."),
        (
            Say::FinishRename,
            "Finish the rename before dragging a tab out.",
        ),
        (Say::Busy, "A directory is already loading."),
    ]
}

#[test]
fn every_tab_sentence_and_label_resolves_from_the_catalog_with_its_argument() {
    let catalog = english();
    for (say, text) in verdicts() {
        assert_eq!(
            say.text(&catalog),
            text,
            "{} resolves to the shipped sentence",
            say.key()
        );
        assert!(
            say.key().starts_with("tabs.") || say.key() == "pane.busy",
            "{} sits in the strip's own region or reuses the shared one",
            say.key()
        );
    }
    for (key, text) in MENU {
        assert_eq!(catalog.tr(key), text, "{key} names the board's label");
    }
    assert_eq!(
        Say::NoTab(9).text(&catalog),
        "No tab 9",
        "the number fills the sentence's own hole"
    );
}

#[test]
fn a_language_change_rereads_the_sentences_and_a_missing_one_falls_back_to_english() {
    let root = root();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "en").unwrap();
    std::fs::write(
        root.join("catalog/de/flea.json"),
        r#"{"tabs.no_tab":"Kein Tab %1","tabs.close":"Tab schließen"}"#,
    )
    .unwrap();
    let mut catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    });
    assert_eq!(Say::NoTab(2).text(&catalog), "No tab 2", "English first");
    std::fs::write(root.join("language.next"), "de\n").unwrap();
    std::fs::rename(root.join("language.next"), root.join("language")).unwrap();
    assert!(catalog.poll_changed(), "the language file changing is seen");
    assert_eq!(
        Say::NoTab(2).text(&catalog),
        "Kein Tab 2",
        "a translated sentence takes its argument in its own hole"
    );
    assert_eq!(
        catalog.tr("tabs.close"),
        "Tab schließen",
        "a translated label replaces the English one"
    );
    assert_eq!(
        Say::Most.text(&catalog),
        "Nine tabs is the most.",
        "a sentence the translation lacks falls back to English"
    );
}
