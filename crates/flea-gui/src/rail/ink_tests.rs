use super::{label_ink, rail_palette};
use crate::palette::Theme;

const BOARD: &str =
    "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'";

#[test]
fn a_broken_favourite_label_takes_the_error_ink() {
    let palette = rail_palette(&gpui::TextStyle::default(), Theme::from_text(BOARD));
    assert_ne!(
        palette.error, palette.foreground,
        "the fixture needs two inks"
    );
    assert_eq!(
        label_ink(true, &palette),
        palette.error,
        "SidebarRow.qml: an error label uses Theme.color.error"
    );
    assert_eq!(
        label_ink(false, &palette),
        palette.foreground,
        "every other label keeps the foreground"
    );
}

// catppuccin-latte and sunset as Omarchy ships them: the two themes the muted caption was caught on.
const LATTE: &str = "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e3e4e8'\nmuted='#acb0be'";
const SUNSET: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'\nmuted='#68544b'";

/// EY-401, EY-383: RailHeading.qml and SidebarRow.qml's detail draw in Theme.color.muted, never the metadata ink.
#[test]
fn ey401_rail_headings_and_row_details_take_the_muted_caption() {
    use super::{heading_ink, paint::detail_ink, RowsModel};
    for (name, text) in [("catppuccin-latte", LATTE), ("sunset", SUNSET)] {
        let theme = Theme::from_text(text);
        let palette = rail_palette(&gpui::TextStyle::default(), theme);
        assert_ne!(
            theme.muted, theme.metadata,
            "{name}: the fixture needs two inks"
        );
        assert_eq!(heading_ink(&palette), theme.muted, "{name}: rail heading");
        let model = RowsModel::default();
        assert_eq!(
            detail_ink(&model, 0, &palette),
            theme.muted,
            "{name}: row detail"
        );
    }
    let latte = rail_palette(&gpui::TextStyle::default(), Theme::from_text(LATTE));
    assert!(
        super::heading_ink(&latte).l > label_ink(false, &latte).l,
        "on a light theme the heading reads lighter than the row labels"
    );
}
