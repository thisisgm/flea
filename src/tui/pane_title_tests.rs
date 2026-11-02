use super::*;
use crate::jsondoc::parse;
use crate::tui::model::Row;
use crate::tui::render::frame_tests::{entry, model, plain};
use crate::tui::wire::echo_wire;

const LISTED: &str = r#"{"t":"trashbrowse","id":1,"op":"list","ok":true,"total":1,"start":0,"rows":[{"uri":"trash:///old-draft.txt","original":"/fx/v/work/old-draft.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"}]}"#;

// The title row as preview() writes it, between the base colour and the padding around it.
fn title_row(m: &Model, columns: usize) -> String {
    let theme = Theme::from_text("");
    let row = preview(m, 0, columns, 6, &theme);
    let base = theme.base();
    let pad = " ".repeat(pane_padding(columns));
    let body = row.strip_prefix(&format!("{base}{pad}")).unwrap();
    body.strip_suffix(&format!("{base}{pad}{base}")).unwrap().to_string()
}
fn file_title(name: &str, marks: bool, columns: usize) -> String {
    let mut m = model();
    m.font_marks = marks;
    let cursor = m.cursor;
    m.rows.insert(cursor, entry(name, false, 99));
    m.preview_path = m.current_path().unwrap();
    title_row(&m, columns)
}
fn linked_row(name: &str, link: &str) -> Row {
    let mut row = entry(name, false, 99);
    row.link = link.into();
    row
}
fn linked_title(name: &str, link: &str, columns: usize) -> String {
    let mut m = model();
    let cursor = m.cursor;
    m.rows.insert(cursor, linked_row(name, link));
    m.preview_path = m.current_path().unwrap();
    title_row(&m, columns)
}
fn selection_title(count: usize, marks: bool, columns: usize) -> String {
    let mut m = model();
    m.font_marks = marks;
    m.selected.extend(0..count);
    title_row(&m, columns)
}
// The bold name between the file title's foreground and meta inks, then the plain cells after it.
fn named(theme: &Theme, mark: &str, name: &str, tail: &str) -> String {
    format!("{}{mark}\x1b[1m{name}\x1b[22m{}{tail}", theme.foreground, theme.meta)
}
// The exact sequence, its plain cells, and plain text equal to ellipsis(mark + name, width).
fn check(title: &str, mark: &str, name: &str, width: usize, expected: &str) {
    let at = format!("{name:?} mark={mark:?} width={width}");
    assert_eq!(title, expected, "{at}");
    assert_eq!(text_width(&plain(title)), width, "{at}");
    assert_eq!(plain(title), plain(&ellipsis(&format!("{mark}{name}"), width)), "{at}");
}

#[test]
fn file_title_bolds_only_the_name_cells_at_every_width() {
    let theme = Theme::from_text("");
    let icon = icons::row(&entry("main.rs", false, 99), true);
    let mark = format!("{icon} ");
    let cases = vec![
        ("main.rs", true, 30, named(&theme, &mark, "main.rs", &" ".repeat(19))),
        ("main.rs", false, 30, named(&theme, "", "main.rs", &" ".repeat(21))),
        ("main.rs", true, 11, named(&theme, &mark, "main.rs", "")),
        ("a-very-long-file-name.rs", true, 16, named(&theme, &mark, "a-very-long\u{2026}", "")),
        ("a-very-long-file-name.rs", false, 16, named(&theme, "", "a-very-long-f\u{2026}", "")),
        ("界界.rs", true, 11, named(&theme, &mark, "界界.rs", "")),
        ("界界.rs", true, 7, named(&theme, &mark, "界\u{2026}", "")),
        ("a界b", false, 3, format!("{}\x1b[1ma\x1b[22m \x1b[1m\u{2026}\x1b[22m{}", theme.foreground, theme.meta)),
        ("\u{1f3b5} song", false, 5, named(&theme, "", "\u{1f3b5} s\u{2026}", "")),
        ("main.rs", true, 3, format!("{}{icon} \u{2026}{}", theme.foreground, theme.meta)),
        ("main.rs", true, 2, format!("{}{icon}\u{2026}{}", theme.foreground, theme.meta)),
        ("main.rs", true, 1, format!("{}\u{2026}{}", theme.foreground, theme.meta)),
        ("main.rs", false, 2, named(&theme, "", "m\u{2026}", "")),
        ("main.rs", true, 0, format!("{}{}", theme.foreground, theme.meta)),
        ("main.rs", false, 0, format!("{}{}", theme.foreground, theme.meta)),
    ];
    for (name, marks, columns, expected) in cases {
        let width = columns - 2 * pane_padding(columns);
        let mark = if marks { mark.as_str() } else { "" };
        check(&file_title(name, marks, columns), mark, name, width, &expected);
    }
}

#[test]
fn file_title_at_the_smoke_width_bolds_wide_names_in_cells_81_to_128() {
    // The T smoke frame is 130 wide: the title spans cells 81 to 128, which is 48 cells.
    assert_eq!(preview_area(130, false), (81, 48));
    let theme = Theme::from_text("");
    let icon = icons::row(&entry("x.rs", false, 99), true);
    let mark = format!("{icon} ");
    let long = format!("{}.rs", "界".repeat(30));
    let cases = vec![
        (
            long,
            format!("{}{mark}\x1b[1m{}\x1b[22m \x1b[1m\u{2026}\x1b[22m{}", theme.foreground, "界".repeat(22), theme.meta),
        ),
        ("界界.rs".to_string(), named(&theme, &mark, "界界.rs", &" ".repeat(39))),
    ];
    for (name, expected) in cases {
        check(&file_title(&name, true, 50), &mark, &name, 48, &expected);
    }
}

#[test]
fn selection_title_bolds_only_the_count_text_at_every_width() {
    let icon = icons::FILES;
    let mark = format!("{icon} ");
    let cases = vec![
        (2, true, 30, format!("{mark}\x1b[1m2 items selected\x1b[22m{}", " ".repeat(10))),
        (2, false, 30, format!("\x1b[1m2 items selected\x1b[22m{}", " ".repeat(12))),
        (2, true, 50, format!("{mark}\x1b[1m2 items selected\x1b[22m{}", " ".repeat(30))),
        (120, true, 50, format!("{mark}\x1b[1m120 items selected\x1b[22m{}", " ".repeat(28))),
        (2, true, 12, format!("{mark}\x1b[1m2 items\u{2026}\x1b[22m")),
        (2, true, 3, format!("{mark}\u{2026}")),
    ];
    for (count, marks, columns, expected) in cases {
        let width = columns - 2 * pane_padding(columns);
        let name = format!("{} selected", crate::tui::render::items(count));
        let mark = if marks { mark.as_str() } else { "" };
        check(&selection_title(count, marks, columns), mark, &name, width, &expected);
    }
}

#[test]
fn symlink_target_stays_plain_when_it_fits() {
    let theme = Theme::from_text("");
    let row = linked_row("notes.txt", "../shared/notes.txt");
    let mark = format!("{} ", icons::row(&row, true));
    let target = " → ../shared/notes.txt";
    let title = {
        let mut m = model();
        m.font_marks = true;
        let cursor = m.cursor;
        m.rows.insert(cursor, row);
        m.preview_path = m.current_path().unwrap();
        title_row(&m, 60)
    };
    let width = 60 - 2 * pane_padding(60);
    let tail = " ".repeat(width - text_width(&format!("{mark}notes.txt")) - text_width(target));
    let expected = format!("{}{mark}\x1b[1mnotes.txt\x1b[22m{}{target}{tail}", theme.foreground, theme.meta);
    assert_eq!(title, expected);
    assert_eq!(text_width(&plain(&title)), width);
}

#[test]
fn symlink_target_is_cut_plain_with_an_ellipsis() {
    let theme = Theme::from_text("");
    let mark = format!("{} ", icons::row(&linked_row("notes.txt", "x"), true));
    let title = linked_title("notes.txt", "../a/very/long/path/to/the/target/notes.txt", 40);
    let expected = format!("{}{mark}\x1b[1mnotes.txt\x1b[22m{} → ../a/very/long/path/to/\u{2026}", theme.foreground, theme.meta);
    assert_eq!(title, expected);
    assert_eq!(text_width(&plain(&title)), 38);
}

#[test]
fn name_cut_before_the_symlink_target_drops_the_target() {
    let theme = Theme::from_text("");
    let mark = format!("{} ", icons::row(&linked_row("a-very-long-file-name.rs", "x"), true));
    let title = linked_title("a-very-long-file-name.rs", "../shared/x", 22);
    let expected = format!("{}{mark}\x1b[1ma-very-long-file-\u{2026}\x1b[22m{}", theme.foreground, theme.meta);
    assert_eq!(title, expected);
    assert_eq!(text_width(&plain(&title)), 20);
}

#[test]
fn trash_title_bolds_only_the_name_cells() {
    let (mut w, _reader) = echo_wire();
    let mut m = model();
    m.font_marks = true;
    (m.cursor, m.top) = (7, 2);
    crate::tui::trashview::open(&mut m, &mut w).unwrap();
    m.receive(parse(LISTED).unwrap(), &mut w).unwrap();
    m.cursor = 0;
    let row = m.rows.get(&0).unwrap();
    assert_eq!(row.name, "old-draft.txt");
    let mark = format!("{} ", icons::row(row, true));
    let whole = title_row(&m, 50);
    assert_eq!(whole, format!("{mark}\x1b[1mold-draft.txt\x1b[22m{}", " ".repeat(48 - text_width(&mark) - 13)));
    let cut = title_row(&m, 14);
    assert_eq!(cut, format!("{mark}\x1b[1mold-draft\u{2026}\x1b[22m"));
    assert_eq!(text_width(&plain(&cut)), 12);
    m.font_marks = false;
    let plain_name = title_row(&m, 50);
    assert_eq!(plain_name, format!("\x1b[1mold-draft.txt\x1b[22m{}", " ".repeat(48 - 13)));
}

// Private catalogs for the typed title cases: each right-to-left language words its prose around the count hole.
const TYPED_ENGLISH: &str = r#"{"preview.selected":{"one":"%1 item selected","other":"%1 items selected"},"preview.fact.combined":"Combined","preview.link_target":"→ %1","tui.frame.outside":{"one":"%1 marked item outside loaded rows","other":"%1 marked items outside loaded rows"},"tui.frame.selected":{"one":"%1 item selected","other":"%1 items selected"},"tui.frame.combined":"Combined · %1"}"#;
const TYPED_CATALOGS: [(&str, &str); 4] = [
    ("ar", r#"{"preview.selected":{"other":"اختير <FSI>%1<PDI> عناصر."},"preview.fact.combined":"المجموع","tui.frame.outside":{"other":"(%1) عناصر خارج الصفوف"}}"#),
    ("he", r#"{"preview.selected":{"other":"<FSI>%1<PDI> פריטים נבחרו"},"preview.fact.combined":"סך הכול","tui.frame.outside":{"other":"%1 פריטים מחוץ לשורות"}}"#),
    ("fa", r#"{"preview.selected":{"other":"«%1» مورد انتخاب شد"},"preview.fact.combined":"مجموع","tui.frame.outside":{"other":"%1 مورد بیرون از ردیف‌ها"}}"#),
    ("ur", r#"{"preview.selected":{"other":"%1 اشیاء منتخب ہیں۔"},"preview.fact.combined":"کل","tui.frame.outside":{"other":"%1 اشیاء قطاروں سے باہر"}}"#),
];
fn typed_language(root: &crate::backend::testdir::TestDir, code: &str) -> crate::i18n::I18n {
    root.dir("omarchy/default/i18n");
    root.file("omarchy/default/i18n/locales.json", include_str!("../i18n/fixtures/locales.json"));
    for (lang, body) in [("en", TYPED_ENGLISH)].into_iter().chain(TYPED_CATALOGS) {
        root.dir(&format!("catalog/{lang}"));
        // The catalogs' own isolates around a hole, as the shipped Arabic and Hebrew catalogs write them.
        root.file(&format!("catalog/{lang}/flea.json"), &body.replace("<FSI>", "\u{2068}").replace("<PDI>", "\u{2069}"));
    }
    root.file("language", code);
    crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    })
}
fn typed_model(root: &crate::backend::testdir::TestDir, code: &str, count: usize) -> Model {
    let mut m = model();
    m.i18n = typed_language(root, code);
    crate::tui::preview::refresh_locale(&mut m);
    for index in 0..count {
        m.rows.insert(index, entry(&format!("f{index}.txt"), false, 1536));
        m.selected.insert(index);
    }
    m
}
// Every isolate opened is closed, in order, and none is closed that was not opened.
fn balanced(text: &str) -> bool {
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '\u{2066}'..='\u{2068}' => depth += 1,
            '\u{2069}' if depth == 0 => return false,
            '\u{2069}' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}
// Sample input: "\x1b[1mاختير \u{2066}3\u{2069} عناصر.\x1b[22m" gives [('\u{2066}', "3")].
// The visible run each isolate wraps, in order, with its opening marker.
fn isolated(text: &str) -> Vec<(char, String)> {
    let plain_text: String = {
        let mut chars = text.chars();
        let mut out = String::new();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                if chars.next() == Some('[') { for c in chars.by_ref() { if ('@'..='~').contains(&c) { break; } } }
            } else { out.push(c); }
        }
        out
    };
    let mut runs = Vec::new();
    let mut open: Option<(char, String)> = None;
    for c in plain_text.chars() {
        match c {
            '\u{2066}'..='\u{2068}' => open = Some((c, String::new())),
            '\u{2069}' => runs.extend(open.take()),
            _ => if let Some((_, run)) = &mut open { run.push(c) },
        }
    }
    runs
}

#[test]
fn ey_tui_typedtitle_rtl_selection_header_isolates_only_the_count() {
    let root = crate::backend::testdir::TestDir::new("ey-typedtitle-header");
    for (code, digits, prose) in [("ar", "3", "اختير"), ("he", "3", "פריטים"), ("fa", "۳", "مورد"), ("ur", "3", "منتخب")] {
        let m = typed_model(&root, code, 3);
        assert!(m.preview_state.i18n.locale().right_to_left, "{code}");
        let title = title_row(&m, 50);
        assert!(balanced(&title), "{code}: {title:?}");
        assert_eq!(text_width(&plain(&title)), 50 - 2 * pane_padding(50), "{code}: {title:?}");
        assert!(plain(&title).contains(prose), "{code}: {title:?}");
        // The count is its own left-to-right number run; the translated prose is never wrapped as data.
        assert_eq!(isolated(&title), vec![('\u{2066}', digits.to_string())], "{code}: {title:?}");
        // The header keeps its icon recipe: the plain mark, then the bold words.
        assert!(title.starts_with(&format!("{} \x1b[1m", icons::FILES)), "{code}: {title:?}");
    }
}

#[test]
fn ey_tui_typedtitle_rtl_combined_and_outside_lines_isolate_sizes_and_counts() {
    let root = crate::backend::testdir::TestDir::new("ey-typedtitle-facts");
    for code in ["ar", "he", "fa", "ur"] {
        let m = typed_model(&root, code, 3);
        let theme = Theme::from_text("");
        let facts = preview(&m, 1, 50, 8, &theme);
        assert!(balanced(&facts), "{code}: {facts:?}");
        let size = m.preview_state.i18n.locale().format_size(3 * 1536);
        assert_eq!(isolated(&facts), vec![('\u{2066}', size)], "{code}: {facts:?}");
        // No selected row is loaded, so the list's first line counts the marked items outside the loaded rows.
        let outside = preview(&m, 3, 50, 8, &theme);
        assert!(balanced(&outside), "{code}: {outside:?}");
        let count = m.i18n.locale().format_count(3);
        assert_eq!(isolated(&outside), vec![('\u{2066}', count)], "{code}: {outside:?}");
        assert_eq!(text_width(&plain(&outside)), 50, "{code}");
    }
}

#[test]
fn ey_tui_typedtitle_titles_fit_every_narrow_width_and_the_smoke_frame() {
    let root = crate::backend::testdir::TestDir::new("ey-typedtitle-widths");
    for code in ["ar", "he", "fa", "ur", "en"] {
        for marks in [true, false] {
            let mut m = typed_model(&root, code, 12);
            m.font_marks = marks;
            for columns in (0..=12).chain([50]) {
                let width = columns - 2 * pane_padding(columns);
                let title = title_row(&m, columns);
                let at = format!("{code} marks={marks} columns={columns}: {title:?}");
                assert!(balanced(&title), "{at}");
                assert_eq!(text_width(&plain(&title)), width, "{at}");
                assert_eq!(title.matches("\x1b[1m").count(), title.matches("\x1b[22m").count(), "{at}");
            }
            // The 130-column smoke frame draws the same header in cells 81 to 128.
            let frame = crate::tui::render::frame(&m, &Theme::from_text(""), &crate::tui::keymap::Map::load(), 130, 27, std::time::Duration::ZERO);
            assert!(balanced(&frame), "{code} marks={marks}");
            let header = m.preview_state.i18n.trn("preview.selected", 12);
            let words: String = header.chars().filter(|c| !matches!(c, '\u{2066}'..='\u{2069}')).collect();
            assert!(plain(&frame).contains(&words), "{code} marks={marks}: {words:?}");
        }
    }
}

#[test]
fn ey_tui_typedtitle_rtl_file_title_sanitizes_a_hostile_name_without_touching_it() {
    let root = crate::backend::testdir::TestDir::new("ey-typedtitle-hostile");
    let hostile = "تقرير\u{202e}gpj.exe\u{2066}\n2025\u{2069}.txt";
    for code in ["ar", "he"] {
        let mut m = typed_model(&root, code, 0);
        let cursor = m.cursor;
        m.rows.insert(cursor, linked_row(hostile, "../مجلد/target 7.txt"));
        m.preview_path = m.current_path().unwrap();
        for (columns, clean_name) in [(8, "تقر\u{2026}"), (20, "تقريرgpj.exe202\u{2026}"), (50, "تقريرgpj.exe2025.txt")] {
            let title = title_row(&m, columns);
            assert!(balanced(&title), "{code} {columns}: {title:?}");
            assert!(!title.contains(['\u{202e}', '\n']), "{code} {columns}: {title:?}");
            assert_eq!(text_width(&plain(&title)), columns - 2 * pane_padding(columns), "{code} {columns}: {title:?}");
            let runs = isolated(&title);
            assert!(runs.iter().all(|(open, _)| *open == '\u{2068}'), "{code} {columns}: {runs:?}");
            // HIG CHANGES.md:2012,2838: the name leads after its mark; the target follows it.
            let mut expected = vec![('\u{2068}', clean_name.to_string())];
            if columns == 50 { expected.push(('\u{2068}', "→ ../مجلد/target 7.txt".into())); }
            assert_eq!(runs, expected, "{code} {columns}: {title:?}");
            let transport = cells::visual::frame(&title);
            assert!(!transport.contains(['\u{202e}', '\n', '\u{2066}', '\u{2068}', '\u{2069}']), "{code} {columns}: {transport:?}");
            if columns == 50 {
                let shown = plain(&transport);
                assert!(shown.find("gpj.exe2025.txtريرقت").zip(shown.find("target 7.txt")).is_some_and(|(name, target)| name < target), "{code}: {transport:?}");
            }
        }
        assert_eq!(m.rows[&cursor].name, hostile, "the stored name keeps its original bytes");
        assert_eq!(m.rows[&cursor].link, "../مجلد/target 7.txt");
    }
}

#[test]
fn ey_tui_typedtitle_english_rtl_filename_keeps_the_name_only_bold_recipe() {
    let theme = Theme::from_text("");
    let icon = icons::row(&entry("שלום עולם 12.txt", false, 99), true);
    let mark = format!("{icon} ");
    let name = "שלום עולם 12.txt";
    // HIG CHANGES.md:2019,2838 and rtl-policy.json:42: mark plain, bold name isolated before transport.
    for (columns, expected, visual_name) in [
        (30, named(&theme, &mark, &format!("\u{2068}{name}\u{2069}"), &" ".repeat(30 - 2 * pane_padding(30) - text_width(&mark) - text_width(name))), "txt.12 םלוע םולש          "),
        (12, named(&theme, &mark, "\u{2068}שלום עו\u{2026}\u{2069}", ""), "\u{2026}וע םולש"),
        (4, named(&theme, &mark, "\u{2068}ש\u{2026}\u{2069}", ""), "\u{2026}ש"),
        (3, format!("{}{icon} \u{2026}{}", theme.foreground, theme.meta), "\u{2026}"),
    ] {
        let width = columns - 2 * pane_padding(columns);
        let title = file_title(name, true, columns);
        check(&title, &mark, name, width, &expected);
        assert!(balanced(&title), "{columns}: {title:?}");
        let transport = cells::visual::frame(&title);
        assert_eq!(plain(&transport), format!("{mark}{visual_name}"), "{columns}: {transport:?}");
        assert!(!transport.contains(['\u{2068}', '\u{2069}']), "{columns}: {transport:?}");
    }
}

#[test]
fn ey_tui_typedtitle_language_switch_rebuilds_the_header_from_retained_selection() {
    let root = crate::backend::testdir::TestDir::new("ey-typedtitle-switch");
    let mut m = typed_model(&root, "en", 4);
    let rows_before: Vec<String> = m.rows.values().map(|row| row.name.clone()).collect();
    let selected_before = m.selected.clone();
    let english = title_row(&m, 50);
    assert!(isolated(&english).is_empty(), "{english:?}");
    assert!(plain(&english).contains("4 items selected"));
    for code in ["ar", "he", "fa", "ur"] {
        root.file("language", code);
        assert!(m.refresh_frame_locale(), "{code}");
        let title = title_row(&m, 50);
        let count = m.preview_state.i18n.locale().format_count(4);
        assert_eq!(isolated(&title), vec![('\u{2066}', count)], "{code}: {title:?}");
        assert_eq!(m.selected, selected_before);
        assert_eq!(m.rows.values().map(|row| row.name.clone()).collect::<Vec<_>>(), rows_before);
    }
}
