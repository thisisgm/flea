use flea::colours;
use gpui::{Global, Hsla, Rgba};

#[cfg(test)]
mod consumer_tests;
#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod golden_tests;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub surface: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    /// ThemeColors.qml's muted, metadata, frame, link, program and error inks, resolved by the shared theme core.
    pub muted: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub muted_frame: Hsla,
    pub symlink: Hsla,
    pub executable: Hsla,
    pub error: Hsla,
    pub error_on_surface: Hsla,
}

/// The floored inks a pane draws with, from the window theme or from an inherited ground and ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub muted: Hsla,
    pub muted_frame: Hsla,
    pub metadata: Hsla,
    pub metadata_frame: Hsla,
    pub symlink: Hsla,
    pub executable: Hsla,
    pub error: Hsla,
    pub error_on_surface: Hsla,
}

impl Roles {
    fn from_core(core: &flea::theme::ThemeColors) -> Self {
        Self {
            muted: role(core.muted),
            muted_frame: role(core.mutedFrame),
            metadata: role(core.metadata),
            metadata_frame: role(core.metadataFrame),
            symlink: role(core.symlink),
            executable: role(core.executable),
            error: role(core.error),
            error_on_surface: role(core.errorOnSurface),
        }
    }

    /// Explicit inherited colours keep their own ground and ink and take the same floors on that ground.
    pub fn for_inherited(background: Hsla, foreground: Hsla) -> Self {
        let opaque = |colour: Hsla| crate::ipc::rows::colour(Hsla { a: 1., ..colour });
        let (bg, fg) = (opaque(background), opaque(foreground));
        let text = format!("background='{bg}'\nforeground='{fg}'\ndark_background='{bg}'");
        Self::from_core(&flea::theme::Theme::resolve(&text, "", "", None, "monospace").colors)
    }
}

fn role(color: flea::theme::Color) -> Hsla {
    Rgba {
        r: color.r as f32,
        g: color.g as f32,
        b: color.b as f32,
        a: color.a as f32,
    }
    .into()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParsedTheme(pub bool);

impl Global for ParsedTheme {}

impl Global for Theme {}

impl Default for Theme {
    fn default() -> Self {
        // Resolved once: a frame without an installed theme reads the default and must not allocate.
        static DEFAULT: std::sync::OnceLock<Theme> = std::sync::OnceLock::new();
        *DEFAULT.get_or_init(|| Self::from_text(""))
    }
}

impl Theme {
    pub fn load() -> (Self, ParsedTheme) {
        let path = std::env::var("HOME").unwrap_or_default()
            + "/.local/state/omarchy/current/theme/colors.toml";
        Self::from_text_observed(&std::fs::read_to_string(path).unwrap_or_default())
    }

    // Sample input: color0='#101315' or background = "#101315", one key=value per line.
    pub fn from_text(text: &str) -> Self {
        Self::from_text_observed(text).0
    }

    pub fn from_text_observed(text: &str) -> (Self, ParsedTheme) {
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
        // Reuse ui/Theme.qml's canvas and src/tui/theme.rs's foreground fallbacks.
        let fallback_ink = 0xcacccc;
        let background = pick(&["background", "color0"], 0x101315);
        let foreground = pick(&["foreground", "color7"], fallback_ink);
        let accent = pick(&["accent", "blue", "color4"], foreground);
        // ui/js/Palette.js:18 SURFACE_KEYS with ui/Theme.qml:46 fallbackColor.surface, the ground Theme.qml:330 lifts against.
        let surface = pick(&["dark_background", "background", "selection"], 0x181825);
        // ui/Theme.qml:342 lifts the accent to 3:1 on that surface for every theme, an empty file included.
        let raised = flea::ui_logic_pl4::tagtheme::ensure_ratio(
            &format!("#{accent:06x}"),
            &format!("#{surface:06x}"),
            3.,
        );
        let frame = u32::from_str_radix(&raised[1..], 16).unwrap_or(accent);
        // The colour roles need no shell or font input, so an empty shell and the stock family resolve them.
        let core = flea::theme::Theme::resolve(text, "", "", None, "monospace").colors;
        let roles = Roles::from_core(&core);
        let theme = Self {
            background: gpui::rgb(background).into(),
            foreground: gpui::rgb(foreground).into(),
            surface: gpui::rgb(surface).into(),
            accent: gpui::rgb(accent).into(),
            accent_frame: gpui::rgb(frame).into(),
            muted: roles.muted,
            metadata: roles.metadata,
            metadata_frame: roles.metadata_frame,
            muted_frame: roles.muted_frame,
            symlink: roles.symlink,
            executable: roles.executable,
            error: roles.error,
            error_on_surface: roles.error_on_surface,
        };
        // ui/js/Palette.js:31-33 isPalette: any parsed colour makes the theme ready, ui/Theme.qml:357.
        let loaded = !values.is_empty();
        (theme, ParsedTheme(loaded))
    }

    pub fn roles(&self) -> Roles {
        Roles {
            muted: self.muted,
            muted_frame: self.muted_frame,
            metadata: self.metadata,
            metadata_frame: self.metadata_frame,
            symlink: self.symlink,
            executable: self.executable,
            error: self.error,
            error_on_surface: self.error_on_surface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_precedes_color4_and_frames_keep_the_oracles_contrast() {
        for text in [
            "background='#101315'\naccent='#e5a55b'\ncolor4='#123456'",
            "background='#eff1f5'\naccent='#1e66f5'\ncolor4='#123456'",
        ] {
            let theme = Theme::from_text(text);
            let raw = colours::entries(text)
                .into_iter()
                .find(|(key, _)| *key == "accent")
                .unwrap()
                .1;
            assert_eq!(crate::ipc::rows::colour(theme.accent), raw);
            let rgb = |color| {
                flea::ui_logic_pl4::tagtheme::parse(&crate::ipc::rows::colour(color)).unwrap()
            };
            assert!(
                flea::ui_logic_pl4::tagtheme::ratio_of(
                    rgb(theme.accent_frame),
                    rgb(theme.background)
                ) >= 3.
            );
        }
        let legacy = Theme::from_text("color4='#123456'");
        assert_eq!(crate::ipc::rows::colour(legacy.accent), "#123456");
    }

    #[test]
    fn palette_uses_oem_precedence_and_existing_fallbacks() {
        let theme = Theme::from_text(
            "color0='#cacccc'\ncolor7='#101315'\nbackground='#101315'\nforeground='#cacccc'",
        );
        let fallback = Theme::default();
        assert_eq!(theme.background, fallback.background);
        assert_eq!(theme.foreground, fallback.foreground);
        assert_eq!(theme.accent, fallback.accent);
        assert_eq!(theme.accent_frame, fallback.accent_frame);
        assert_eq!(theme.surface, gpui::rgb(0x101315).into());
        assert_eq!(fallback.surface, gpui::rgb(0x181825).into());
        assert_eq!(Theme::from_text("background='invalid'"), Theme::default());
        let reversed = Theme::from_text("color0='#cacccc'\ncolor7='#101315'");
        assert_eq!(reversed.background, theme.foreground);
        assert_eq!(reversed.foreground, theme.background);
        assert_eq!(reversed.surface, fallback.surface);
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
