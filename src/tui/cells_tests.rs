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
    for cursor in [false, true] { for focused in [false, true] { for marked in [false, true] { for dimmed in [false, true] {
        let style = RowStyle { cursor, focused, marked, dimmed, ..Default::default() };
        let output = super::row(&row, 51, Some("99 B"), true, style, &theme, "");
        let text = plain(&output);
        assert_eq!(text_width(&text), 51);
        assert_eq!(text.chars().nth(1), Some('\u{ec5e}'));
        assert_eq!(text.chars().nth(2), Some(' '));
        assert_eq!(text.chars().skip(3).take(9).collect::<String>(), "notes.txt");
        assert!(text.ends_with("99 B  "));
        let fill = if cursor { if focused { &theme.cursor } else { &theme.selected } } else if marked { &theme.marked } else { &theme.background };
        assert!(output.starts_with(&format!("\x1b[0m{fill}")));
        assert_eq!(text.starts_with('▎'), cursor && focused);
        if dimmed { assert!(output.starts_with(&format!("\x1b[0m{fill}{}", theme.muted))); }
        assert!(!output.contains("\x1b[1m"));
    }}}}
}
#[test]
fn t3_text_fallback_keeps_columns_and_only_filter_matches_are_bold() {
    let theme = Theme::from_text("");
    for directory in [false, true] {
        let row = entry("notes.txt", directory, 99);
        for marks in [true, false] {
            let output = super::row(&row, 30, None, marks, RowStyle::default(), &theme, "note");
            let text = plain(&output);
            assert_eq!(text.chars().skip(3).take(9).collect::<String>(), "notes.txt");
            assert!(output.contains("\x1b[1m"));
            assert_eq!(text.chars().nth(2), Some(' '));
            if !marks { assert_eq!(text.chars().nth(1), Some(if directory { '›' } else { ' ' })); }
        }
    }
}
#[test]
fn wide_symbol_width_in_c_locale() {
    if crate::tui::render::frame_tests::isolated_test("tui::cells::tests::wide_symbol_width_in_c_locale", &[("LC_ALL", "C")]) { return; }
    extern "C" { fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char; }
    assert!(!unsafe { setlocale(0, c"".as_ptr()) }.is_null());
    assert_eq!(unsafe { wcwidth(0x1f600) }, -1, "the C locale measured the symbol itself, so the fallback table went untested");
    println!("C locale: wide symbol measured {} cells", width('\u{1f600}'));
    assert_eq!(width('\u{1f600}'), 2);
}
#[test]
fn wide_symbol_width_in_utf8_locale() {
    if crate::tui::render::frame_tests::isolated_test("tui::cells::tests::wide_symbol_width_in_utf8_locale", &[]) { return; }
    extern "C" { fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char; }
    assert!(!unsafe { setlocale(0, c"".as_ptr()) }.is_null());
    assert_eq!(unsafe { wcwidth(0x1f600) }, 2, "the UTF-8 locale left the symbol to the fallback table, so wcwidth went untested");
    println!("UTF-8 locale: wide symbol measured {} cells", width('\u{1f600}'));
    assert_eq!(width('\u{1f600}'), 2);
}
#[test]
fn wrapping_keeps_wide_word_parts_separate() {
    let lines = words("a議議", 4);
    println!("wrapped lines: {lines:?}, widths: {:?}", lines.iter().map(|s| text_width(s)).collect::<Vec<_>>());
    assert_eq!(lines, ["a議", "議"]);
}
