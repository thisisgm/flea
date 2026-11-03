use crate::tui::{cells, keymap::Map, layout, render, theme::Theme};
use std::time::Duration;

fn painted(text: String) -> String {
    let mut output = Vec::new();
    render::present(text, &mut String::new(), &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn sol_tui_arabic_board_strings_have_visual_unjoined_cells() {
    for (logical, visual) in [
        ("الاسم", "مسالا"),
        ("10 عناصر", "رصانع 10"),
        ("? الاختصارات", "تاراصتخالا ?"),
        ("tmpfs · 64.1 MB متاحة", "tmpfs · 64.1 MB ةحاتم"),
        ("تقرير 2025.pdf", "pdf.2025 ريرقت"),
    ] {
        let line = cells::fit(logical, cells::text_width(logical) + 3);
        let output = painted(line);
        assert_eq!(render::frame_tests::plain(&output), format!("{visual}   "));
        assert!(!output
            .chars()
            .any(|c| matches!(c, '\u{fb50}'..='\u{fdff}' | '\u{fe70}'..='\u{feff}')));
    }
}

#[test]
fn sol_tui_arabic_mixed_name_keeps_width_truncation_and_highlight() {
    let entry = render::frame_tests::entry("تقرير 2025.pdf", false, 21800);
    let theme = Theme::from_text("");
    for width in 0..55 {
        let logical = cells::row(
            &entry,
            width,
            Some("21.8 kB"),
            true,
            cells::RowStyle::default(),
            &theme,
            "قر",
        );
        let shown = painted(logical);
        let plain = render::frame_tests::plain(&shown);
        assert_eq!(cells::text_width(&plain), width, "{plain:?}");
        if width == 54 {
            assert!(plain.contains("pdf.2025 ريرقت"), "{plain:?}");
            assert!(shown.contains("\x1b[1m"));
            assert!(plain.ends_with("21.8 kB  "));
        }
        assert_eq!(entry.name, "تقرير 2025.pdf");
    }
}

#[test]
fn sol_tui_arabic_board_panes_and_heading_keep_leading_alignment() {
    let fixture = crate::backend::testdir::TestDir::new("sol-tui-arabic");
    for directory in ["catalog/ar", "omarchy/default/i18n"] {
        std::fs::create_dir_all(fixture.join(directory)).unwrap();
    }
    std::fs::write(fixture.join("catalog/ar/flea.json"), r#"{"window.column_name":"الاسم","items.count":{"other":"%1 عناصر"},"tui.frame.keys":"%1 الاختصارات"}"#).unwrap();
    std::fs::write(
        fixture.join("omarchy/default/i18n/locales.json"),
        include_str!("../i18n/fixtures/locales.json"),
    )
    .unwrap();
    std::fs::write(fixture.join("language"), "ar\n").unwrap();
    let mut m = render::frame_tests::model();
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: fixture.join("omarchy"),
        language_file: fixture.join("language"),
        catalogs: vec![fixture.join("catalog")],
    });
    m.prompts.i18n = m.i18n.clone();
    m.total = 10;
    let theme = Theme::from_text("");
    let output = painted(render::frame(
        &m,
        &theme,
        &Map::load(),
        130,
        31,
        Duration::ZERO,
    ));
    let rows = render::frame_tests::grid(&output, 130, 31);
    assert!(rows[0].starts_with(" /fx/v/work"));
    assert!(rows[0].ends_with("مسالا ▴ "), "{:?}", rows[0]);
    assert_eq!(layout::visual_start(&m, 0, 27), 0);
    assert_eq!(rows[1].chars().nth(27), Some('┬'));
    assert_eq!(rows[1].chars().nth(79), Some('┬'));
    assert!(rows[30].contains("رصانع 10"));
    let label = "الصفحة 1 من 1 · 100%";
    let width = cells::text_width(label);
    let page = crate::tui::pane::direction(label, width, &m.i18n, false);
    let page = crate::tui::pane::paint(&[(crate::tui::pane::Ink::Meta, page)], width, &theme);
    assert_eq!(
        render::frame_tests::plain(&painted(page)),
        "100% · 1 نم 1 ةحفصلا"
    );
}

#[test]
fn sol_tui_arabic_diacritics_stay_on_their_base_cells() {
    let shown = painted(cells::fit("ب\u{64e}ت", 4));
    assert_eq!(render::frame_tests::plain(&shown), "تب\u{64e}  ");
}

#[test]
fn sol_tui_arabic_editor_caret_uses_visual_boundary() {
    let editor = crate::tui::editor::Editor::new("rename", "تقرير".into(), "/fx/تقرير".into());
    let (field, width, caret) = editor.field("", 20, &Theme::from_text(""));
    assert_eq!(caret, Some(0));
    assert_eq!(
        cells::text_width(&render::frame_tests::plain(&painted(field))),
        width
    );
    assert_eq!(editor.value, "تقرير");
}

#[test]
fn sol_tui_arabic_highlight_moves_with_source_letters() {
    let logical = "\u{2068}ت\x1b[1mقر\x1b[22mير\u{2069}";
    let shown = painted(logical.into());
    let mut rest = shown.as_str();
    let (mut bold, mut highlighted) = (false, String::new());
    while !rest.is_empty() {
        if rest.starts_with("\x1b[") {
            let end = rest.find('m').unwrap();
            for code in rest[2..end].split(';') {
                match code {
                    "0" | "22" => bold = false,
                    "1" => bold = true,
                    _ => {}
                }
            }
            rest = &rest[end + 1..];
        } else {
            let c = rest.chars().next().unwrap();
            if bold {
                highlighted.push(c);
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    assert_eq!(render::frame_tests::plain(&shown), "ريرقت");
    assert_eq!(highlighted, "رق");
}

#[test]
fn sol_tui_arabic_review_mirrors_paired_punctuation() {
    assert_eq!(
        render::frame_tests::plain(&painted(cells::fit("(تقرير)", 7))),
        "(ريرقت)"
    );
}

#[test]
fn sol_tui_arabic_review_combining_caret_boundaries_share_base_edge() {
    assert_eq!(super::caret("ب\u{64e}ت", "ب".len()), 1);
    assert_eq!(super::caret("ب\u{64e}", "ب\u{64e}".len()), 0);
}

#[test]
fn sol_tui_arabic_preview_percent_stays_ltr() {
    let label = "الصفحة 1 من 1 · 100%";
    let shown = painted(cells::fit(label, cells::text_width(label)));
    assert_eq!(render::frame_tests::plain(&shown), "100% · 1 نم 1 ةحفصلا");
    let fitted = cells::ellipsis(label, cells::text_width(label));
    let isolated = cells::isolate(cells::Run::Name, &fitted);
    assert_eq!(
        render::frame_tests::plain(&painted(isolated)),
        "100% · 1 نم 1 ةحفصلا"
    );
    let label = "الصفحة · ١٠٠٪";
    assert_eq!(
        render::frame_tests::plain(&painted(cells::fit(label, cells::text_width(label)))),
        "١٠٠٪ · ةحفصلا"
    );
}

#[test]
fn sol_tui_arabic_ltr_frame_bytes_are_unchanged_at_transport() {
    let m = render::frame_tests::model();
    let logical = render::frame(
        &m,
        &Theme::from_text(""),
        &Map::load(),
        130,
        31,
        Duration::ZERO,
    );
    assert_eq!(painted(logical.clone()), format!("{logical}\x1b[0m"));
}
