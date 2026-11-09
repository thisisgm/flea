//! A live Omarchy language switch re-translates the whole TUI frame in every shipped language, and a folder name stays data.
use crate::tui::render::cards::tests::screen;
use crate::tui::render::frame_tests::{entry, model};
use crate::tui::theme::Theme;

fn plain(word: &str) -> String {
    word.chars()
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

// Grid cells hold visual order, so only a right-to-left word may read reversed; bidi controls never reach a cell.
fn shows(rows: &[String], word: &str, right_to_left: bool) -> bool {
    let logical = plain(word);
    let reversed: String = logical.chars().rev().collect();
    let text = rows.join("\n");
    text.contains(&logical) || (right_to_left && text.contains(&reversed))
}

#[test]
fn every_language_switch_retranslates_the_tui_frame_and_keeps_folder_names() {
    let root = crate::backend::testdir::TestDir::new("tui-live-language");
    root.dir("omarchy/default/i18n/en");
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    root.file(
        "omarchy/default/i18n/en/keybindings.json",
        include_str!("../i18n/fixtures/omarchy/default/i18n/en/keybindings.json"),
    );
    root.file("language", "en");
    let catalogs = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n");
    let mut m = model();
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![catalogs.clone()],
    });
    m.rows.insert(0, entry("Projects", true, 0));
    let theme = Theme::from_text("");
    let codes: Vec<String> = m
        .i18n
        .roster()
        .iter()
        .map(|locale| locale.code.clone())
        .collect();
    assert_eq!(codes.len(), 50, "every language Omarchy ships");
    for code in codes
        .iter()
        .filter(|code| *code != "en")
        .chain(std::iter::once(&"en".to_owned()))
    {
        root.replace_file(&root.join("language"), code);
        assert!(m.refresh_frame_locale(), "{code}: the switch is seen");
        assert_eq!(m.i18n.language(), code);
        let name = m.i18n.tr("window.column_name");
        // Read beside the loader, so a fallback to English or to the key cannot stand in for this language's word.
        let file = std::fs::read_to_string(catalogs.join(code).join("flea.json"))
            .expect("shipped catalog");
        assert!(
            file.contains(&format!("\"window.column_name\": \"{}\"", plain(&name))),
            "{code}: {name:?} is not this catalog's word"
        );
        crate::tui::layout::prepare(&mut m);
        let (rows, _, _) = screen(&m, &theme);
        let right_to_left = m.i18n.locale().right_to_left;
        for word in [name, m.i18n.trn("items.count", m.total as i64)] {
            assert!(
                shows(&rows, &word, right_to_left),
                "{code}: the frame draws {word:?}\n{}",
                rows.join("\n")
            );
        }
        assert!(
            shows(&rows, "Projects", false),
            "{code}: a folder named Projects keeps its name"
        );
    }
}
