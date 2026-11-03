use super::*;

fn shipped(code: &str) -> (crate::backend::testdir::TestDir, I18n) {
    let root = crate::backend::testdir::TestDir::new("ar-tui-copy");
    let roster = root.dir("omarchy/default/i18n");
    std::fs::write(
        roster.join("locales.json"),
        include_str!("fixtures/locales.json"),
    )
    .unwrap();
    for (code, text) in [
        ("en", include_str!("../../i18n/en/flea.json")),
        ("ar", include_str!("../../i18n/ar/flea.json")),
        ("ru", include_str!("../../i18n/ru/flea.json")),
    ] {
        let catalog = root.dir(&format!("catalog/{code}"));
        std::fs::write(catalog.join("flea.json"), text).unwrap();
    }
    let paths = Paths {
        omarchy: root.join("omarchy"),
        language_file: root.file("language", code),
        catalogs: vec![root.join("catalog")],
    };
    (root, I18n::load(paths))
}

#[test]
fn ar_tui_copy_shipped_arabic_forms_reach_gui_and_tui() {
    let (_root, tui) = shipped("ar");
    let mut gui = tui.clone();
    gui.mark_paragraphs();
    for (count, category, expected) in [
        (0, "zero", "\u{2068}0\u{2069} عنصر"),
        (1, "one", "عنصر واحد (\u{2068}1\u{2069})"),
        (2, "two", "عنصران (\u{2068}2\u{2069})"),
        (3, "few", "\u{2068}3\u{2069} عناصر"),
        (9, "few", "\u{2068}9\u{2069} عناصر"),
        (10, "few", "\u{2068}10\u{2069} عناصر"),
        (11, "many", "\u{2068}11\u{2069} عنصرًا"),
        (99, "many", "\u{2068}99\u{2069} عنصرًا"),
        (100, "other", "\u{2068}100\u{2069} عنصر"),
    ] {
        assert_eq!(plural::category("ar", count), category, "{count}");
        assert_eq!(tui.trn("items.count", count), expected, "TUI {count}");
        assert_eq!(gui.trn("items.count", count), expected, "GUI {count}");
        eprintln!("ar count={count} category={category} GUI=TUI {expected:?}");
    }
}

#[test]
fn ar_tui_copy_shipped_russian_forms_and_english_bytes() {
    let (_root, russian) = shipped("ru");
    for (count, category) in [
        (0, "many"),
        (1, "one"),
        (2, "few"),
        (3, "few"),
        (9, "many"),
        (10, "many"),
        (11, "many"),
        (21, "one"),
        (22, "few"),
        (99, "many"),
        (100, "many"),
    ] {
        let noun = match category {
            "one" => "элемент",
            "few" => "элемента",
            _ => "элементов",
        };
        assert_eq!(plural::category("ru", count), category);
        assert_eq!(russian.trn("items.count", count), format!("{count} {noun}"));
    }
    let (_root, english) = shipped("en");
    for count in [0, 1, 2, 3, 9, 10, 11, 99, 100] {
        let expected = format!("{count} {}", if count == 1 { "item" } else { "items" });
        assert_eq!(
            english.trn("items.count", count).as_bytes(),
            expected.as_bytes()
        );
    }
}

#[test]
fn ar_tui_copy_shared_lookup_keeps_every_cardinal_category() {
    let english = catalog::parse(
        r#"{"count":{"one":"one source","other":"other source"}}"#,
        false,
    );
    let translated = catalog::parse(
        r#"{"count":{"zero":"zero","one":"one","two":"two","few":"few","many":"many","other":"other"}}"#,
        false,
    );
    for (code, count, expected) in [
        ("ar", 0, "zero"),
        ("ar", 1, "one"),
        ("ar", 2, "two"),
        ("ar", 9, "few"),
        ("ar", 11, "many"),
        ("ar", 100, "other"),
        ("ru", 22, "few"),
        ("ru", 25, "many"),
        ("sl", 2, "two"),
        ("lv", 0, "zero"),
        ("ga", 8, "many"),
    ] {
        assert_eq!(
            catalog::lookup(&english, &translated, "count", code, Some(count)),
            Some(expected)
        );
    }
}

#[test]
fn ar_tui_copy_shortcut_hint_keeps_the_complete_hig_board_word() {
    let (_root, i18n) = shipped("ar");
    assert_eq!(
        i18n.tr_args("tui.frame.keys", &["?"]),
        "\u{2066}?\u{2069} الاختصارات"
    );
}
