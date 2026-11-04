use super::*;
use crate::tui::render::frame_tests::{entry, plain};

#[test]
fn t3_widths_truncation_combining_and_controls_stay_in_cells() {
    assert_eq!(text_width("議事録.md"), 9);
    assert_eq!(text_width("e\u{301}"), 1);
    assert_eq!(fit("議事録", 5), "議事 ");
    assert_eq!(ellipsis("議事録.md", 5), "議事…");
    assert_eq!(ellipsis("e\u{301}long", 2), "e\u{301}…");
    assert_eq!(clean("name\x1b[31m\u{202e}txt"), "name[31mtxt");
    assert_eq!(fit("\u{301}a", 1), "a");
    for limit in 0..12 {
        assert_eq!(text_width(&ellipsis("議事録.md", limit)), limit);
        assert_eq!(text_width(&fit("議事録.md", limit)), limit);
    }
}
#[test]
fn t3_every_cursor_focus_mark_dim_combination_uses_one_rung_and_one_mark_gap() {
    let row = entry("notes.txt", false, 99);
    let theme = Theme::from_text("foreground='#c6b395'\nbackground='#1c120d'\naccent='#d28a41'");
    for cursor in [false, true] {
        for focused in [false, true] {
            for marked in [false, true] {
                for dimmed in [false, true] {
                    let style = RowStyle {
                        cursor,
                        focused,
                        marked,
                        dimmed,
                        ..Default::default()
                    };
                    let output = super::row(&row, 51, Some("99 B"), true, style, &theme, "");
                    let text = plain(&output);
                    assert_eq!(text_width(&text), 51);
                    assert_eq!(text.chars().nth(1), Some('\u{ec5e}'));
                    assert_eq!(text.chars().nth(2), Some(' '));
                    assert_eq!(
                        text.chars().skip(3).take(9).collect::<String>(),
                        "notes.txt"
                    );
                    assert!(text.ends_with("99 B  "));
                    let fill = if cursor {
                        if focused {
                            &theme.cursor
                        } else {
                            &theme.selected
                        }
                    } else if marked {
                        &theme.marked
                    } else {
                        &theme.background
                    };
                    assert!(output.starts_with(&format!("\x1b[0m{fill}")));
                    assert_eq!(text.starts_with('▎'), cursor && focused);
                    if dimmed {
                        assert!(output.starts_with(&format!("\x1b[0m{fill}{}", theme.muted)));
                    }
                    assert!(!output.contains("\x1b[1m"));
                }
            }
        }
    }
}
#[test]
fn t3_text_fallback_keeps_columns_and_only_filter_matches_are_bold() {
    let theme = Theme::from_text("");
    for directory in [false, true] {
        let row = entry("notes.txt", directory, 99);
        for marks in [true, false] {
            let output = super::row(&row, 30, None, marks, RowStyle::default(), &theme, "note");
            let text = plain(&output);
            assert_eq!(
                text.chars().skip(3).take(9).collect::<String>(),
                "notes.txt"
            );
            assert!(output.contains("\x1b[1m"));
            assert_eq!(text.chars().nth(2), Some(' '));
            if !marks {
                assert_eq!(text.chars().nth(1), Some(if directory { '›' } else { ' ' }));
            }
        }
    }
}
#[test]
fn wide_symbol_width_in_c_locale() {
    if crate::tui::render::frame_tests::isolated_test(
        "tui::cells::tests::wide_symbol_width_in_c_locale",
        &[("LC_ALL", "C")],
    ) {
        return;
    }
    extern "C" {
        fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char;
    }
    assert!(!unsafe { setlocale(0, c"".as_ptr()) }.is_null());
    assert_eq!(
        unsafe { wcwidth(0x1f600) },
        -1,
        "the C locale measured the symbol itself, so the fallback table went untested"
    );
    println!(
        "C locale: wide symbol measured {} cells",
        width('\u{1f600}')
    );
    assert_eq!(width('\u{1f600}'), 2);
    tui_cells_preserve_combining_clusters_at_edges();
}
#[test]
fn tui_cells_wide_range_zero_width_marks_in_c_locale() {
    if crate::tui::render::frame_tests::isolated_test(
        "tui::cells::tests::tui_cells_wide_range_zero_width_marks_in_c_locale",
        &[("LC_ALL", "C")],
    ) {
        return;
    }
    extern "C" {
        fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char;
    }
    assert!(!unsafe { setlocale(0, c"".as_ptr()) }.is_null());
    for mark in "\u{302a}\u{302b}\u{302c}\u{302d}\u{3099}\u{309a}\u{3164}".chars() {
        let measured = unsafe { wcwidth(mark as i32) };
        assert_eq!(measured, -1, "{mark:?}: fallback untested");
        assert_eq!(width(mark), 0, "{mark:?}");
        let cluster = format!("一{mark}");
        let text = format!("{cluster}z");
        assert_eq!(text_width(&cluster), 2, "{mark:?}");
        assert_eq!(fit(&text, 2), cluster);
        assert_eq!(ellipsis(&format!("{text}z"), 3), format!("{cluster}…"));
        let wrapped = Wrapped::new(&text, 2).collect::<Vec<_>>();
        assert_eq!(wrapped, [cluster.as_str(), "z"]);
        assert_eq!(clean(&format!("{mark}一")), "一");
        assert_eq!(fit(&format!("{mark}一"), 3), "一 ");
    }
    for mark in ['\u{302e}', '\u{302f}', '\u{16ff0}', '\u{16ff1}'] {
        assert_eq!(unsafe { wcwidth(mark as i32) }, -1);
        assert_eq!(width(mark), 2, "spacing mark {mark:?} must stay wide");
    }
}
#[test]
fn wide_symbol_width_in_utf8_locale() {
    // Named here, not left to isolated_test's default, so the measured locale is this test's own.
    if crate::tui::render::frame_tests::isolated_test(
        "tui::cells::tests::wide_symbol_width_in_utf8_locale",
        &[("LC_ALL", "C.UTF-8")],
    ) {
        return;
    }
    extern "C" {
        fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char;
    }
    assert!(!unsafe { setlocale(0, c"".as_ptr()) }.is_null());
    assert_eq!(
        unsafe { wcwidth(0x1f600) },
        2,
        "the UTF-8 locale left the symbol to the fallback table, so wcwidth went untested"
    );
    println!(
        "UTF-8 locale: wide symbol measured {} cells",
        width('\u{1f600}')
    );
    assert_eq!(width('\u{1f600}'), 2);
    tui_cells_preserve_combining_clusters_at_edges();
}
#[test]
fn wrapping_keeps_wide_word_parts_separate() {
    let lines = words("a議議", 4);
    println!(
        "wrapped lines: {lines:?}, widths: {:?}",
        lines.iter().map(|s| text_width(s)).collect::<Vec<_>>()
    );
    assert_eq!(lines, ["a議", "議"]);
}

#[test]
fn tui_cells_clip_cjk_without_half_cells() {
    for columns in 0..=20 {
        for text in ["議事録.md", "写真", "見積書_2025.pdf", "a界b"] {
            assert_eq!(text_width(&fit(text, columns)), columns);
            assert_eq!(text_width(&ellipsis(text, columns)), columns);
            for part in Wrapped::new(text, columns) {
                assert!(text_width(part) <= columns, "{part:?} exceeds {columns}");
            }
            for line in words(text, columns) {
                assert!(text_width(&line) <= columns, "{line:?} exceeds {columns}");
            }
        }
    }
}

#[test]
fn tui_cells_preserve_combining_clusters_at_edges() {
    for (text, cells) in [
        ("e\u{301}", 1),
        ("ا\u{64e}", 1),
        ("か\u{3099}", 2),
        ("界\u{e0100}", 2),
    ] {
        assert_eq!(text_width(text), cells, "{text:?}");
        assert_eq!(plain(&fit(&format!("{text}z"), cells)), text);
        assert_eq!(
            plain(&ellipsis(&format!("{text}zz"), cells + 1)),
            format!("{text}…")
        );
        assert_eq!(Wrapped::new(&format!("{text}z"), cells).next(), Some(text));
    }
    for mark in ['\u{301}', '\u{64e}', '\u{3099}', '\u{fe0f}', '\u{e0100}'] {
        assert_eq!(clean(&format!("{mark}a")), "a");
        assert_eq!(fit(&format!("{mark}a"), 2), "a ");
        assert_eq!(centered(&format!("{mark}a"), 5), "  a  ");
        assert_eq!(
            name_title("› ", &format!("{mark}a"), 5),
            "› \x1b[1ma\x1b[22m  "
        );
    }
}

#[test]
fn tui_cells_isolate_mixed_names_and_extensions() {
    let theme = Theme::from_text("");
    for name in [
        "تقرير 2025.pdf",
        "draft-تقرير.pdf",
        "تَقرير-notes.txt",
        "議事録-تقرير.md",
    ] {
        let file = entry(name, false, 99);
        let output = super::row(
            &file,
            51,
            Some("99 B"),
            true,
            RowStyle::default(),
            &theme,
            "2025",
        );
        let visible = plain(&output);
        assert!(
            visible.contains(name),
            "the bounded name must retain its suffix and digits: {visible:?}"
        );
        assert!(visible.ends_with("99 B  "));
        assert_eq!(text_width(&visible), 51);
        let mut injected = file.clone();
        injected.name = format!("\u{202e}{name}\u{2069}");
        let injected = plain(&super::row(
            &injected,
            51,
            Some("99 B"),
            true,
            RowStyle::default(),
            &theme,
            "2025",
        ));
        assert_eq!(
            injected.chars().skip(3).collect::<String>(),
            visible.chars().skip(3).collect::<String>()
        );
    }
}

#[test]
fn tui_cells_refuse_filename_control_injection() {
    let theme = Theme::from_text("");
    for control in [
        '\x00', '\x07', '\x1b', '\x7f', '\u{85}', '\u{61c}', '\u{200e}', '\u{200f}', '\u{2028}',
        '\u{2029}', '\u{202a}', '\u{202e}', '\u{2066}', '\u{2069}', '\u{206a}', '\u{206f}',
    ] {
        let mut file = entry("a.txt", false, 99);
        file.name = format!("a{control}.txt");
        file.link = format!("{control}target");
        let output = super::row(
            &file,
            40,
            Some("99 B"),
            true,
            RowStyle::default(),
            &theme,
            "",
        );
        assert!(
            !plain(&output).contains(control),
            "filename emitted {control:?}"
        );
        assert_eq!(text_width(&plain(&output)), 40);
        assert!(plain(&output).ends_with("99 B  "));
        assert!(!plain(&name_title("› ", &file.name, 20)).contains(control));
        assert!(words(&file.name, 20)
            .iter()
            .all(|line| !line.contains(control)));
    }
}

#[test]
fn tui_cells_real_rows_fit_all_scripts_and_narrow_limits() {
    let theme = Theme::from_text("");
    let invalid = String::from_utf8_lossy(b"bad\xff.txt");
    for name in [
        "notes.txt",
        "議事録.md",
        "تَقرير 2025.pdf",
        "e\u{301}long.txt",
        "\u{301}x",
        "\u{fe0f}",
        &invalid,
    ] {
        let mut file = entry("name", false, 99);
        file.name = name.into();
        file.link = "\u{301}target/界".into();
        for columns in 0..=130 {
            for marks in [false, true] {
                for query in ["", "e", "\u{301}", "2025", "界"] {
                    let output = super::row(
                        &file,
                        columns,
                        Some("99 B"),
                        marks,
                        RowStyle::default(),
                        &theme,
                        query,
                    );
                    let visible = plain(&output);
                    assert_eq!(text_width(&visible), columns, "{name:?} at {columns}");
                    assert!(!visible.contains("→ \u{301}"));
                    if columns >= 4 {
                        assert_eq!(visible.chars().nth(2), Some(' '));
                    }
                    assert_eq!(
                        text_width(&plain(&name_title("› ", name, columns))),
                        columns
                    );
                }
            }
        }
    }
}

#[test]
fn tui_cells_isolates_follow_shared_rtl_policy() {
    let shared =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/i18n/rtl-policy.json"))
            .expect("the shared policy is readable");
    let parsed = runs_from(&shared).expect("the shared policy maps to cell isolates");
    assert_eq!(
        parsed,
        Runs {
            name: ('\u{2068}', '\u{2069}'),
            number: ('\u{2066}', '\u{2069}')
        }
    );
    let swapped = shared.replacen(
        "\"FSI+unchanged-text+PDI\"",
        "\"RLI+unchanged-text+PDI\"",
        1,
    );
    assert_eq!(
        runs_from(&swapped).map(|runs| runs.name),
        Some(('\u{2067}', '\u{2069}'))
    );
    for refused in [
        shared.replacen("\"reverse_bytes\": false", "\"reverse_bytes\": true", 1),
        shared.replacen(
            "\"strip_raw_display_controls\": true",
            "\"strip_raw_display_controls\": false",
            1,
        ),
        shared.replacen(
            "\"LRI+unchanged-text+PDI\"",
            "\"LRI+unchanged-text+LRI\"",
            1,
        ),
        shared.replacen("\"FSI+unchanged-text+PDI\"", "\"FSI+reversed-text+PDI\"", 1),
    ] {
        assert_ne!(refused, shared);
        assert_eq!(runs_from(&refused), None);
    }
    for name in [
        "تقرير 2025.pdf",
        "draft-تقرير.pdf",
        "תיק/2025",
        "議事録-تقرير.md",
    ] {
        assert_eq!(isolate(Run::Name, name), format!("\u{2068}{name}\u{2069}"));
        let cut = ellipsis(name, 6);
        assert_eq!(text_width(&isolate(Run::Name, &cut)), 6);
    }
    assert_eq!(
        isolate(Run::Number, "2025-10-25"),
        "\u{2066}2025-10-25\u{2069}"
    );
    assert_eq!(
        isolate(Run::Number, "ctrl+shift+ب"),
        "\u{2066}ctrl+shift+ب\u{2069}"
    );
    assert_eq!(
        isolate(Run::Name, "\u{2069}אבג\u{202e}.pdf\u{2066}\x1b[2J"),
        "\u{2068}אבג.pdf[2J\u{2069}"
    );
}
