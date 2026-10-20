use super::Ink;
use crate::list::Palette;
use crate::palette::Theme;
use gpui::TextStyle;

#[test]
fn tile_ink_pins_its_shared_roles_to_the_list_palette_under_an_inherited_background() {
    let theme = Theme::default();
    // A style that carries its own background, distinct from the theme's: the list palette and the tile ink both read it.
    let style = TextStyle {
        color: theme.background,
        background_color: Some(theme.background.opacity(0.5)),
        ..Default::default()
    };
    let ink = Ink::inherited(&style, theme);
    let list = Palette::inherited(&style, theme);
    assert_eq!(ink.background, list.background, "the tile ground");
    assert_eq!(ink.muted_frame, list.muted_frame, "the tile muted frame");
    assert_eq!(ink.accent, list.accent, "the tile accent");
    assert_eq!(ink.accent_frame, list.accent_frame, "the tile accent frame");
    assert_eq!(ink.foreground, list.foreground, "the tile foreground");
}
