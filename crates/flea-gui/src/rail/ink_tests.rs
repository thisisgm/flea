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
