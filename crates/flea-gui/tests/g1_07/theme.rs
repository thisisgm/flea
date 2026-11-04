use flea_gui::palette::Theme;

pub fn test_theme() -> Theme {
    let base = Theme::default();
    Theme {
        background: base.foreground,
        foreground: base.background,
        ..base
    }
}
