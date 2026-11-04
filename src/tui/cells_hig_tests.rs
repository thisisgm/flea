use crate::tui::cells::*;
use crate::tui::render::frame_tests::entry;
use crate::tui::theme::Theme;

#[test]
fn tui_parity17_hig_filter_matches_keep_the_rows_background_and_use_bold_only() {
    let theme = Theme::from_text("foreground='#c6b395'\nbackground='#1c120d'\naccent='#d28a41'");
    let entry = entry("notes.txt", false, 99);
    for cursor in [false, true] {
        let output = crate::tui::cells::row(
            &entry,
            51,
            Some("99 B"),
            true,
            RowStyle {
                cursor,
                ..Default::default()
            },
            &theme,
            "o",
        );
        assert!(output.contains("\x1b[1mo"));
        assert!(
            !output.contains(&theme.match_fill),
            "filter changed the row background: {output:?}"
        );
    }
}

#[test]
fn detail_off_returns_the_empty_metadata_lane_to_the_name() {
    let theme = Theme::from_text("");
    let file = entry("abcdefghijklmnop", false, 1);
    let out = crate::tui::cells::row(&file, 20, Some(""), false, RowStyle::default(), &theme, "");
    assert!(crate::tui::render::frame_tests::plain(&out).contains("abcdefghijklmnop"));
}
