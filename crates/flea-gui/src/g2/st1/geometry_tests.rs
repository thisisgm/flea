//! tests/settings-columns.qml and settings-tail.qml: band heights, and the pane's reveal at every host height.
use super::geometry::{fade_shown, row_height, FADE, FOOTER_LINE, HINT_LINE};
use super::panel::Panel;
use super::tests::panel;
use flea::ui_logic_st1::{pages::SECTIONS, rows::{focusable, Kind, Row}, Text};

#[test]
fn a_hint_band_is_4_over_18_a_line_and_2_under_and_the_footer_is_49_at_one_line() {
    let hint = Row::hint(Text::data("a"));
    assert_eq!((row_height(&hint, false, 1), row_height(&hint, false, 3)), (24., 4. + 3. * HINT_LINE + 2.));
    let footer = Row { footer: true, ..Row::hint(Text::data("Saved")) };
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
        assert!(top >= panel.scroll - 0.01 && bottom <= panel.scroll + height + 0.01, "{at}: cursor inside");
        let last = !panel.rows.iter().skip(panel.cursor + 1).any(focusable);
        if fade_shown(&heights, panel.scroll, height) && !last {
            assert!(bottom <= panel.scroll + height - FADE + 0.01, "{at}: fade over the cursor");
            if let Some(next) = panel.rows.get(panel.cursor + 1) {
                assert!(bottom + ink(next, false) + (FADE * 0.5).round() <= panel.scroll + height + 0.01, "{at}: the next row's ink shows under the fade");
            }
        }
        panel.key("j", "j", false);
    }
}

#[test]
fn every_section_reveals_its_cursor_at_every_host_height() {
    let mut panel = panel(r#"{"places":{"favourites":[{"label":"A","path":"/a"},{"label":"B","path":"/b"},{"label":"C","path":"/c"}]}}"#);
    let mut cut = Vec::new();
    // Plugins' rows are the plugin contract's (st2), so the page stands empty until it joins.
    panel.show_section("plugins");
    assert!(panel.rows.is_empty(), "plugins draws no rows of its own");
    for section in SECTIONS.iter().map(|section| section.id).filter(|id| *id != "plugins").chain(["columns"]) {
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
