use super::Ink;
use crate::list::Palette;
use crate::palette::Theme;
use gpui::TextStyle;

#[test]
fn tile_ink_takes_the_style_ground_and_the_style_colour() {
    let theme = Theme::default();
    // A style with its own background and its own colour: the tile draws that ground and that colour, never the theme's.
    let background = theme.foreground.opacity(0.5);
    let colour = theme.background;
    let style = TextStyle {
        color: colour,
        background_color: Some(background),
        ..Default::default()
    };
    let ink = Ink::inherited(&style, theme);
    assert_eq!(
        ink.background, background,
        "the tile ground is the style background"
    );
    assert_eq!(ink.foreground, colour, "the tile text is the style colour");
    assert_eq!(
        ink.accent, theme.accent,
        "the tile accent is the theme role"
    );
    assert_eq!(
        ink.accent_frame, theme.accent_frame,
        "the tile accent frame is the theme role"
    );
    assert_eq!(
        ink.muted_frame,
        crate::palette::Roles::for_inherited(background, colour, theme.urgent).muted_frame,
        "the tile muted frame follows the inherited ground contrast lift"
    );
    let list = Palette::inherited(&style, theme);
    assert_eq!(
        (
            ink.background,
            ink.foreground,
            ink.muted_frame,
            ink.accent,
            ink.accent_frame
        ),
        (
            list.background,
            list.foreground,
            list.muted_frame,
            list.accent,
            list.accent_frame
        ),
        "the tile ink reads the list palette role for role"
    );
}

#[test]
fn tile_ink_falls_back_to_the_theme_without_a_style_ground() {
    let theme = Theme::default();
    // A style with its own colour but no background: the theme sets the ground and the ink, the style colour unused.
    let style = TextStyle {
        color: theme.foreground.opacity(0.5),
        ..Default::default()
    };
    let ink = Ink::inherited(&style, theme);
    assert_eq!(
        ink.background, theme.background,
        "the tile ground is the theme background"
    );
    assert_eq!(
        ink.foreground, theme.foreground,
        "the tile text is the theme foreground"
    );
    assert_eq!(
        ink.accent, theme.foreground,
        "the tile accent is the theme foreground"
    );
    assert_eq!(
        ink.accent_frame, theme.foreground,
        "the tile accent frame is the theme foreground"
    );
    assert_eq!(
        ink.muted_frame, theme.muted_frame,
        "the tile muted frame is the theme's muted frame role"
    );
}
