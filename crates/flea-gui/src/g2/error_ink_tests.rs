use crate::{list::Palette, palette::Theme};
use gpui::TextStyle;

// EY-13, EY-06 and SW-23 constructors; the mounted status line and plugins pane have call-site tests of their own.
#[test]
fn menu_status_and_settings_ink_constructors_take_the_error_roles() {
    for name in ["cool-dawn", "sunset"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tests/fixtures/{name}/colors.toml"));
        let theme = Theme::from_text(&std::fs::read_to_string(path).unwrap());
        let palette = Palette::inherited(&TextStyle::default(), theme);
        assert_ne!(palette.error, palette.accent, "{name}");
        assert_ne!(palette.error, palette.foreground, "{name}");
        assert_ne!(palette.error_on_surface, palette.accent, "{name}");
        let menu = crate::g2::mn1::element::MenuInk::from_palette(&palette);
        assert_eq!(menu.error, palette.error, "{name}: Move to Trash ink");
        let footer = crate::g2::fb1::Ink::on_surface(&palette);
        assert_eq!(
            footer.error, palette.error_on_surface,
            "{name}: status bar error ink"
        );
        let settings = crate::g2::st1::seams::Ink::inherited(&TextStyle::default(), theme);
        assert_eq!(
            settings.error, palette.error_on_surface,
            "{name}: Settings Delete Permanently ink"
        );
        assert_eq!(
            settings.surface, theme.surface,
            "{name}: Settings card ground"
        );
    }
}
