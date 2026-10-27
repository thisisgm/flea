//! tests/settings-columns.qml and settings-tail.qml: band heights, and the pane's reveal at every host height.
use super::geometry::{
    estimated_lines, fade_shown, hint_line_count, hint_units, row_height, FADE, FOOTER_LINE,
    HINT_LINE,
};
use super::panel::Panel;
use super::tests::panel;
use flea::ui_logic_st1::{
    pages::SECTIONS,
    rows::{focusable, Kind, Row},
    Text,
};

#[test]
fn a_hint_band_is_4_over_18_a_line_and_2_under_and_the_footer_is_49_at_one_line() {
    let hint = Row::hint(Text::data("a"));
    assert_eq!(
        (row_height(&hint, false, 1), row_height(&hint, false, 3)),
        (24., 4. + 3. * HINT_LINE + 2.)
    );
    let footer = Row {
        footer: true,
        ..Row::hint(Text::data("Saved"))
    };
    assert_eq!(row_height(&footer, false, 1), 49.);
    assert_eq!(row_height(&footer, false, 2), 49. + FOOTER_LINE);
}

// Where the row's first ink starts under its top, as the reveal rule reads it.
fn ink(row: &Row, first: bool) -> f32 {
    match row.kind {
        Kind::Group if first => 16.,
        Kind::Group => 27.,
        _ => 8.,
    }
}

fn walk(panel: &mut Panel, section: &'static str, height: f32) {
    panel.show_section(section);
    panel.laid_out((panel.pane.0, height), Vec::new());
    assert!(!panel.rows.is_empty(), "{section} builds its rows");
    for _ in 0..panel.rows.len() {
        let heights = panel.heights();
        let top: f32 = heights.iter().take(panel.cursor).sum();
        let bottom = top + heights[panel.cursor];
        let at = format!("{section} at {height}: row {}", panel.cursor);
        assert!(
            top >= panel.scroll - 0.01 && bottom <= panel.scroll + height + 0.01,
            "{at}: cursor inside"
        );
        let last = !panel.rows.iter().skip(panel.cursor + 1).any(focusable);
        if fade_shown(&heights, panel.scroll, height) && !last {
            assert!(
                bottom <= panel.scroll + height - FADE + 0.01,
                "{at}: fade over the cursor"
            );
            if let Some(next) = panel.rows.get(panel.cursor + 1) {
                assert!(
                    bottom + ink(next, false) + (FADE * 0.5).round()
                        <= panel.scroll + height + 0.01,
                    "{at}: the next row's ink shows under the fade"
                );
            }
        }
        panel.key("j", "j", false);
    }
}

#[test]
fn settings_hint_keeps_every_chord_whole_for_layout() {
    let raw =
        "Ctrl+Shift++ and Ctrl+Shift+- step through them; Ctrl+Shift+0 follows Omarchy again.";
    let units = hint_units(raw);
    // The laid-out units are the same words in the same order, spaces kept on each word.
    assert_eq!(units.concat(), raw);
    // Every chord is one whole unit, never a fragment the layout could wrap inside.
    for chord in ["Ctrl+Shift++ ", "Ctrl+Shift+- ", "Ctrl+Shift+0 "] {
        assert!(
            units.iter().any(|unit| unit == chord),
            "{chord} is not one unit in {units:?}"
        );
    }
    assert!(
        !units.iter().any(|unit| unit.starts_with('+')),
        "a chord fragment in {units:?}"
    );
    // Packed whole, a chord that does not fit moves to the next line instead of splitting.
    assert_eq!(hint_line_count(&[90., 20.], 100.), 2);
    assert_eq!(hint_line_count(&[40., 20.], 100.), 1);
    assert_eq!(hint_line_count(&[120.], 100.), 1);
}

#[test]
fn an_unspaced_wide_script_hint_still_wraps_between_characters() {
    assert_eq!(
        hint_units("按Ctrl+Shift+0逐级"),
        ["按", "Ctrl+Shift+0", "逐", "级"]
    );
    assert_eq!(hint_units("a 你好 b").concat(), "a 你好 b");
    assert_eq!(
        hint_units("か\u{3099}き"),
        ["か\u{3099}", "き"],
        "a voicing mark stays with its kana"
    );
    assert_eq!(
        hint_units("你好，世界。"),
        ["你", "好，", "世", "界。"],
        "closing punctuation never starts a line"
    );
    assert_eq!(
        hint_units("按「回车」"),
        ["按", "「回", "车」"],
        "opening punctuation never ends a line"
    );
    assert_eq!(
        hint_units("Ctrl+K, then"),
        ["Ctrl+K, ", "then"],
        "a chord keeps its own comma"
    );
    assert_eq!(
        hint_units("辻\u{E0100}道"),
        ["辻\u{E0100}", "道"],
        "a variation selector stays with its ideograph"
    );
    assert_eq!(
        hint_units("按(回车)"),
        ["按", "(回", "车)"],
        "an ASCII bracket never ends a line"
    );
    assert_eq!(
        hint_units("\u{1100}\u{1161}\u{11A8}가"),
        ["\u{1100}\u{1161}\u{11A8}", "가"],
        "a conjoined syllable stays whole"
    );
    let sentence = "按下组合键可以逐级放大或缩小文字并在需要时恢复跟随系统设置的大小";
    assert!(
        estimated_lines(sentence, 14., 100.) > 1,
        "thirty wide characters fit no 100px line"
    );
}

#[test]
fn every_section_reveals_its_cursor_at_every_host_height() {
    let mut panel = panel(
        r#"{"places":{"favourites":[{"label":"A","path":"/a"},{"label":"B","path":"/b"},{"label":"C","path":"/c"}]}}"#,
    );
    let mut cut = Vec::new();
    // Plugins' rows are the plugin contract's (st2), so the page stands empty until it joins.
    panel.show_section("plugins");
    assert!(panel.rows.is_empty(), "plugins draws no rows of its own");
    for section in SECTIONS
        .iter()
        .map(|section| section.id)
        .filter(|id| *id != "plugins")
        .chain(["columns"])
    {
        for height in (160..=640).step_by(7) {
            walk(&mut panel, section, height as f32);
        }
        let total: f32 = panel.heights().iter().sum();
        if total > 300. {
            cut.push(section);
        }
    }
    assert!(cut.contains(&"menus") && cut.len() > 1, "{cut:?}");
}
