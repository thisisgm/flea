use super::rail_palette;
use crate::palette::Theme;

// The HIG boards' --bg and --surface: the panes stand on the background, the rail on the darker surface.
const BOARD: &str =
    "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'";

#[test]
fn the_rail_stands_on_the_surface_not_the_pane_ground() {
    let theme = Theme::from_text(BOARD);
    assert_ne!(
        theme.surface, theme.background,
        "the fixture needs two grounds"
    );
    let rail = rail_palette(&gpui::TextStyle::default(), theme);
    assert_eq!(
        rail.background, theme.surface,
        "the rail ground is the surface role"
    );
    assert_ne!(
        rail.background, theme.background,
        "the rail ground is not the pane ground"
    );
    assert_eq!(
        rail.metadata, theme.metadata,
        "the rail caption keeps its floored role"
    );
}

#[test]
fn an_inherited_ground_keeps_its_own_colour() {
    let theme = Theme::from_text(BOARD);
    let ground = Theme::from_text("background='#223344'").background;
    let style = gpui::TextStyle {
        background_color: Some(ground),
        ..Default::default()
    };
    assert_eq!(rail_palette(&style, theme).background, ground);
}
