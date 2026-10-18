use gpui::{Global, Hsla};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub background: Hsla,
    pub foreground: Hsla,
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

    fn from_text(text: &str) -> Self {
        let values: Vec<_> = text
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                let value = value.trim().trim_start_matches(['"', '\'']);
                let hex = value.strip_prefix('#')?.get(..6)?;
                if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                Some((key.trim(), u32::from_str_radix(hex, 16).ok()?))
            })
            .collect();
        let pick = |keys: &[&str], fallback| {
            keys.iter()
                .find_map(|key| values.iter().rev().find(|(name, _)| name == key))
                .map(|(_, value)| *value)
                .unwrap_or(fallback)
        };
        // Reuse ui/Theme.qml's canvas and src/tui/theme.rs's foreground fallbacks.
        Self {
            background: gpui::rgb(pick(&["background", "color0"], 0x101315)).into(),
            foreground: gpui::rgb(pick(&["foreground", "color7"], 0xcacccc)).into(),
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
}
