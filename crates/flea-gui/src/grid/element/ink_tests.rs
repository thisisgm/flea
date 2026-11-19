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
        ink.muted, list.muted,
        "hidden names share the list's quiet role"
    );
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

#[test]
fn hidden_tile_names_fade_only_at_rest() {
    let theme = Theme::default();
    let mut ink = Ink::inherited(&TextStyle::default(), theme);
    let same = |color| color;
    let faded = crate::row::hidden_name_ink(ink.muted);
    assert_eq!(
        ink.name_in(".env", false, false, same),
        faded,
        "hidden at rest"
    );
    assert_eq!(
        ink.name_in("notes.txt", false, false, same),
        ink.foreground,
        "ordinary at rest"
    );
    // Cursor, selection and a drop target arrive as lit.
    assert_eq!(
        ink.name_in(".env", true, false, same),
        ink.foreground,
        "lit"
    );
    assert_eq!(
        ink.name_in(".env", false, true, same),
        faded,
        "hover without the setting"
    );
    ink.hover_wash = true;
    assert_eq!(
        ink.name_in(".env", false, true, same),
        ink.foreground,
        "hover with the setting"
    );
    assert_eq!(
        ink.name_in("notes.txt", false, true, same),
        ink.foreground,
        "ordinary hovered"
    );
}

#[test]
fn hidden_tile_names_fade_after_the_glass_lift() {
    let ink = Ink::inherited(&TextStyle::default(), Theme::default());
    // Like the glass recipe, this lift recognizes only the exact theme role and leaves any other colour alone.
    let lifted = gpui::hsla(0.5, 0.4, 0.8, 1.);
    let lift = |color| if color == ink.muted { lifted } else { color };
    assert_eq!(
        ink.name_in(".env", false, false, lift),
        crate::row::hidden_name_ink(lifted),
        "the hidden name takes the lifted muted role before fading"
    );
    let bright = |color| {
        if color == ink.foreground {
            lifted
        } else {
            color
        }
    };
    assert_eq!(
        ink.name_in(".env", true, false, bright),
        lifted,
        "a lit name is lifted too"
    );
}
