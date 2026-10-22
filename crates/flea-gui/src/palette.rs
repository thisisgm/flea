use flea::colours;
use gpui::{Global, Hsla};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub surface: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
}

impl Global for Theme {}

impl Default for Theme {
    fn default() -> Self {
        Self::from_text("")
    }
}

impl Theme {
    pub fn load() -> Self {
        let path = std::env::var("HOME").unwrap_or_default()
            + "/.local/state/omarchy/current/theme/colors.toml";
        Self::from_text(&std::fs::read_to_string(path).unwrap_or_default())
    }

    // Sample input: color0='#101315' or background = "#101315", one key=value per line.
    pub fn from_text(text: &str) -> Self {
        let values: Vec<_> = colours::entries(text)
            .into_iter()
            .filter_map(|(key, hex)| Some((key, u32::from_str_radix(&hex[1..], 16).ok()?)))
            .collect();
        let pick = |keys: &[&str], fallback| {
            keys.iter()
                .find_map(|key| values.iter().rev().find(|(name, _)| name == key))
                .map(|(_, value)| *value)
                .unwrap_or(fallback)
        };
        let background = pick(&["background", "color0"], 0x101315);
        let foreground = pick(&["foreground", "color7"], 0xcacccc);
        let surface = pick(&["dark_background"], background);
        let accent = pick(&["accent", "blue", "color4"], foreground);
        let frame = flea::ui_logic_pl4::tagtheme::ensure_ratio(
            &format!("#{accent:06x}"),
            &format!("#{surface:06x}"),
            3.,
        );
        let accent_frame = u32::from_str_radix(&frame[1..], 16).unwrap_or(foreground);
        // Reuse the shipped canvas fallbacks and the board's accent-frame contrast recipe.
        Self {
            background: gpui::rgb(background).into(),
            foreground: gpui::rgb(foreground).into(),
            surface: gpui::rgb(surface).into(),
            accent: gpui::rgb(accent).into(),
            accent_frame: gpui::rgb(accent_frame).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_uses_oem_precedence_and_existing_fallbacks() {
        let theme = Theme::from_text(
            "color0='#cacccc'\ncolor7='#101315'\nbackground='#101315'\nforeground='#cacccc'",
        );
        assert_eq!(theme, Theme::default());
        assert_eq!(Theme::from_text("background='invalid'"), Theme::default());
        let reversed = Theme::from_text("color0='#cacccc'\ncolor7='#101315'");
        assert_eq!(reversed.background, theme.foreground);
        assert_eq!(reversed.foreground, theme.background);
    }

    #[test]
    fn identical_quotes_hex_width_and_precedence() {
        let width: Hsla = gpui::rgb(0x112233).into();
        assert_eq!(
            Theme::from_text("background=\"#112233ff\"").background,
            width
        );
        let last: Hsla = gpui::rgb(0x445566).into();
        assert_eq!(
            Theme::from_text("background='#112233'\nbackground='#445566'").background,
            last
        );
        assert_eq!(
            Theme::from_text("foreground=#12345").foreground,
            Theme::default().foreground
        );
    }
}

#[cfg(test)]
mod control_tests {
    use super::Theme;

    #[test]
    fn fix9_control_roles_keep_the_two_oracle_palettes() {
        for (text, accent, surface) in [
            ("background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\ndark_background='#150d0a'", 0xd28a41, 0x150d0a),
            ("background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e6e9ef'", 0x1e66f5, 0xe6e9ef),
        ] {
            let theme = Theme::from_text(text);
            assert_eq!(theme.accent, gpui::rgb(accent).into());
            assert_eq!(theme.accent_frame, gpui::rgb(accent).into());
            assert_eq!(theme.surface, gpui::rgb(surface).into());
        }
    }
}
