//! I50-24: the sheet and menu hints draw the active keyboard group's keycaps from one shared snapshot.
use super::sheet::{Sheet, Sources};
use super::tests::english;
use crate::g2::context_menus::{
    keycaps,
    lines::{self, Env},
};
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use flea::keys::{display::KeycapSnapshot, presentation};
use flea::ui_logic_context_menus::{Entry, Text as MenuText};
use gpui::AppContext;
use std::path::PathBuf;
use std::sync::Arc;

fn typed(sheet: &mut Sheet, word: &str) {
    for letter in word.chars() {
        let text = letter.to_string();
        sheet.key(&text, &text);
    }
}

fn group(name: &str) -> KeycapSnapshot {
    KeycapSnapshot::from_group_name(name, 0, 1).expect("a group the keycap table knows")
}

// The repo's catalog for `code`, loaded through a fresh roster and language file that are removed once read.
fn language(code: &str, direction: &str) -> Arc<I18n> {
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("flea-i50-24-{}-{run}", std::process::id()));
    std::fs::create_dir(&root).expect("a fresh directory, never one already there");
    let registry = root.join("omarchy/default/i18n");
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(
        registry.join("locales.json"),
        format!(r#"{{"{code}":{{"name":"{code}","direction":"{direction}"}}}}"#),
    )
    .unwrap();
    std::fs::write(root.join("language"), format!("{code}\n")).unwrap();
    let catalogs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../i18n");
    let catalog = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![catalogs],
    });
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(catalog.language(), code);
    Arc::new(catalog)
}

// Each part as the shared snapshot labels it; a part it cannot settle keeps its canonical legend.
fn expected(keys: &str, snapshot: &KeycapSnapshot) -> String {
    let parts = |alternative: &str| {
        let parts = alternative.split(' ');
        parts
            .map(|part| snapshot.label(part).unwrap_or(part))
            .collect::<Vec<_>>()
            .join(" ")
    };
    keys.split(" / ").map(parts).collect::<Vec<_>>().join(" / ")
}

#[test]
fn i50_gui_keycap_active_layout() {
    assert_eq!(keycaps::sync("German").map(|s| s.layout()), Some("de"));
    assert_eq!(keycaps::sync("zed.keyboard.example"), None);
    let mut sheet = Sheet::new("default", english(), Sources::default());
    assert!(
        sheet
            .rows
            .iter()
            .all(|row| sheet.cap(&row.keys) == row.keys),
        "an unknown group draws canonical legends"
    );
    for name in ["German", "French", "English (Dvorak)", "Russian"] {
        let snapshot = group(name);
        sheet.set_keycaps(Some(snapshot));
        for row in &sheet.rows {
            assert_eq!(
                sheet.cap(&row.keys),
                expected(&row.keys, &snapshot),
                "{name} {}",
                row.action
            );
        }
    }
    sheet.set_keycaps(Some(group("German")));
    assert!(
        sheet
            .rows
            .iter()
            .any(|row| sheet.cap(&row.keys) != row.keys),
        "German relabels at least one key the GUI sheet shows"
    );
    // I50-11-AMBIG: an ambiguous part keeps its canonical legend and never withholds the rest.
    let german = group("German");
    assert!(german.label("-").is_err() && german.label("[").is_ok());
    assert_eq!(keycaps::chord("[ - / -", Some(&german), false), "ü - / -");
    assert_eq!(
        keycaps::chord("ctrl-wrong-key", Some(&german), false),
        "ctrl-wrong-key"
    );
}

#[test]
fn i50_gui_keycap_language_independent() {
    let mut sheet = Sheet::new("default", english(), Sources::default());
    sheet.set_keycaps(Some(group("German")));
    let drawn = |sheet: &Sheet| -> Vec<(&str, String, String)> {
        let row = |row: &presentation::SheetRow| {
            (
                row.action,
                row.keys.clone(),
                sheet.cap(&row.keys).to_owned(),
            )
        };
        sheet.rows.iter().map(row).collect()
    };
    let german = drawn(&sheet);
    let english_labels: Vec<String> = sheet.rows.iter().map(|row| row.label.clone()).collect();
    for (code, direction) in [("ru", "ltr"), ("de", "ltr"), ("en", "ltr")] {
        sheet.set_catalog(language(code, direction));
        assert_eq!(
            drawn(&sheet),
            german,
            "{code} keeps the actions, the canonical keys and their keycaps"
        );
        assert_eq!(sheet.keycaps, Some(group("German")));
    }
    sheet.set_catalog(language("ru", "ltr"));
    let russian_labels: Vec<String> = sheet.rows.iter().map(|row| row.label.clone()).collect();
    assert_ne!(
        russian_labels, english_labels,
        "the Russian catalog relabels the actions"
    );
    sheet.set_keycaps(Some(group("French")));
    let french = drawn(&sheet);
    assert_eq!(
        french.iter().map(|row| (row.0, &row.1)).collect::<Vec<_>>(),
        german.iter().map(|row| (row.0, &row.1)).collect::<Vec<_>>(),
        "a group switch never remaps an action"
    );
    assert_ne!(french, german, "a group switch relabels the keycaps");
    let labels: Vec<String> = sheet.rows.iter().map(|row| row.label.clone()).collect();
    assert_eq!(
        labels, russian_labels,
        "a group switch never touches the catalog's words"
    );
}

#[test]
fn i50_gui_keycap_query_refresh() {
    let mut sheet = Sheet::new("default", english(), Sources::default());
    let canonical: std::collections::HashMap<&str, String> = sheet
        .rows
        .iter()
        .map(|row| (row.action, row.keys.clone()))
        .collect();
    typed(&mut sheet, "o");
    assert!(sheet.ranked.len() > 2);
    sheet.key("down", "");
    let highlighted = sheet.ranked[sheet.cursor].action.clone();
    let german = group("German");
    sheet.set_keycaps(Some(german));
    assert_eq!(sheet.query, "o");
    assert_eq!(
        sheet.ranked[sheet.cursor].action, highlighted,
        "the highlight stays on its result"
    );
    for (index, row) in sheet.ranked.iter().enumerate() {
        let drawn = expected(&canonical[row.action.as_str()], &german);
        assert_eq!(
            (row.keys.as_str(), sheet.result_keys(index)),
            (drawn.as_str(), drawn.as_str())
        );
    }
    // A keycap only the German group draws finds its row, as the drawn text is what the query matches.
    let (action, cap) = sheet
        .rows
        .iter()
        .find_map(|row| {
            let parts = row.keys.split([' ', '/']).filter(|part| !part.is_empty());
            let cap = parts
                .filter_map(|part| german.label(part).ok().filter(|cap| *cap != part))
                .find(|cap| sheet.rows.iter().all(|other| !other.keys.contains(cap)))?;
            let unworded = !row.label.to_lowercase().contains(&cap.to_lowercase());
            unworded.then_some((row.action, cap))
        })
        .expect("a row whose key German draws as a keycap no canonical key holds");
    sheet.key("backspace", "");
    typed(&mut sheet, cap);
    assert!(
        sheet.ranked.iter().any(|result| result.action == action),
        "{cap} finds {action}"
    );
    let mut canonical_sheet = Sheet::new("default", english(), Sources::default());
    typed(&mut canonical_sheet, cap);
    assert!(
        canonical_sheet
            .ranked
            .iter()
            .all(|result| result.action != action),
        "without the group {cap} finds no {action}"
    );
}

#[test]
fn i50_gui_keycap_rtl_isolation() {
    let german = group("German");
    assert_eq!(
        keycaps::chord("ctrl-c / [ y", Some(&german), true),
        "\u{2066}ctrl-c\u{2069} / \u{2066}ü y\u{2069}"
    );
    assert_eq!(
        keycaps::chord("ctrl-c / [ y", Some(&german), false),
        "ctrl-c / ü y"
    );
    let isolated = |drawn: &str| {
        let mut alternatives = drawn.split(" / ");
        alternatives.all(|part| part.starts_with('\u{2066}') && part.ends_with('\u{2069}'))
    };
    let mut live = Sheet::new("default", english(), Sources::default());
    live.set_keycaps(Some(german));
    typed(&mut live, "o");
    live.set_catalog(language("ar", "rtl"));
    assert!(
        live.rows.iter().all(|row| isolated(live.cap(&row.keys))),
        "a live switch to Arabic isolates the rows"
    );
    assert!(!live.ranked.is_empty());
    assert!(
        (0..live.ranked.len()).all(|index| isolated(live.result_keys(index))),
        "and the results"
    );
    for (code, direction) in [("ar", "rtl"), ("he", "rtl")] {
        let mut sheet = Sheet::new("default", language(code, direction), Sources::default());
        sheet.set_keycaps(Some(german));
        for row in &sheet.rows {
            let drawn = sheet.cap(&row.keys);
            for alternative in drawn.split(" / ") {
                assert!(
                    alternative.starts_with('\u{2066}') && alternative.ends_with('\u{2069}'),
                    "{drawn:?}"
                );
            }
            let bare: String = drawn
                .chars()
                .filter(|c| !matches!(c, '\u{2066}' | '\u{2069}'))
                .collect();
            assert_eq!(
                bare,
                expected(&row.keys, &german),
                "the parts keep their logical order"
            );
        }
    }
}

#[gpui::test]
fn i50_gui_keycap_menu_hint_same_snapshot(cx: &mut gpui::TestAppContext) {
    let window = cx.add_empty_window();
    let snapshot = keycaps::sync("German");
    let mut sheet = Sheet::new("default", english(), Sources::default());
    sheet.set_keycaps(keycaps::current());
    assert_eq!(
        sheet.keycaps, snapshot,
        "the sheet and the menus read one snapshot"
    );
    let entries: Vec<Entry> = sheet
        .rows
        .iter()
        .map(|row| Entry::row(row.action, MenuText::Data(row.label.clone()), "copy"))
        .collect();
    let catalog = english();
    let typeface = Typeface::omarchy_default();
    let env = Env {
        catalog: &catalog,
        preset: "default",
        key_hints: true,
        typeface: &typeface,
        inset: 4.,
    };
    let built = window.update(|window, _| lines::build(&entries, &env, window, 0., 1100.));
    let mut relabelled = 0;
    for (line, entry) in built.lines.iter().zip(&entries) {
        let canonical = entry.key_text("default", true);
        let drawn = line.key.as_ref().map_or("", |key| key.text.as_str());
        assert_eq!(
            drawn,
            expected(canonical, &snapshot.unwrap()),
            "{}",
            entry.action
        );
        relabelled += usize::from(drawn != canonical);
        if let Some(row) = sheet.rows.iter().find(|row| row.keys == canonical) {
            assert_eq!(
                drawn,
                sheet.cap(&row.keys),
                "a hint and its sheet row draw one keycap"
            );
        }
    }
    assert!(relabelled > 0, "German relabels at least one menu hint");
}

#[gpui::test]
fn i50_gui_keycap_sheet_view_reads_the_platform_group(cx: &mut gpui::TestAppContext) {
    let window = cx.add_empty_window();
    let mut sheet = Sheet::new("default", english(), Sources::default());
    sheet.set_keycaps(Some(group("German")));
    let view = window.update(|_, cx| cx.new(|cx| super::SheetView::new(sheet, cx)));
    // The test platform names its group "zed.keyboard.example", which the keycap table does not know.
    let keycaps = window.update(|_, cx| view.read(cx).sheet.keycaps);
    assert_eq!(
        keycaps, None,
        "a new sheet takes the platform's group, not the one it was built with"
    );
    assert_eq!(keycaps::current(), None);
    assert!(
        keycaps::watching(),
        "the sheet leaves the group watched for the menus"
    );
}

#[test]
fn i50_gui_keycap_switch_keeps_the_highlight_by_identity() {
    let identity = |row: &flea::ui_logic_settings_pages::sheet::Candidate| {
        (
            row.action.clone(),
            row.menu_action.clone(),
            row.path.clone(),
        )
    };
    let mut moved = 0;
    for query in ["[", "]", ";", "'", "/", "-", "=", ",", ".", "o", "e"] {
        let mut probe = Sheet::new("default", english(), Sources::default());
        typed(&mut probe, query);
        for index in 0..probe.ranked.len() {
            let mut sheet = Sheet::new("default", english(), Sources::default());
            typed(&mut sheet, query);
            sheet.cursor = index;
            let highlighted = identity(&sheet.ranked[index]);
            sheet.set_keycaps(Some(group("German")));
            let found = sheet
                .ranked
                .iter()
                .position(|row| identity(row) == highlighted);
            if let Some(found) = found {
                assert_eq!(sheet.cursor, found, "{query:?} keeps {highlighted:?}");
                moved += usize::from(found != index);
            }
        }
    }
    assert!(moved > 0, "German moves at least one highlighted result");
}

#[test]
fn i50_gui_keycap_switch_keeps_the_resting_scroll() {
    let mut sheet = Sheet::new("default", english(), Sources::default());
    sheet.scroll = 120.;
    sheet.set_keycaps(Some(group("German")));
    assert_eq!(
        (sheet.cursor, sheet.scroll, sheet.query.as_str()),
        (0, 120., "")
    );
}
