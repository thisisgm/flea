use flea::{colours, ui_logic_pl4::tagtheme};
use gpui::{Global, Hsla, Rgba};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub surface: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub roles: Roles,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParsedTheme(pub bool);

impl Global for ParsedTheme {}

impl Global for Theme {}

impl Default for Theme {
    fn default() -> Self {
        Self::from_text("")
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
        let roles = Roles::resolve(&pick, background, foreground, surface);
        let theme = Self {
            background: gpui::rgb(background).into(),
            foreground: gpui::rgb(foreground).into(),
            surface: gpui::rgb(surface).into(),
            accent: gpui::rgb(accent).into(),
            accent_frame: gpui::rgb(frame).into(),
            roles,
        };
        // ui/js/Palette.js:31-33 isPalette: any parsed colour makes the theme ready, ui/Theme.qml:357.
        let loaded = !values.is_empty();
        (theme, ParsedTheme(loaded))
    }
}

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

fn hex(value: u32) -> String {
    format!("#{value:06x}")
}

fn colour(value: &str) -> Hsla {
    gpui::rgb(u32::from_str_radix(&value[1..], 16).expect("resolved RGB")).into()
}

impl Roles {
    fn resolve(
        pick: &impl Fn(&[&str], u32) -> u32,
        background: u32,
        foreground: u32,
        surface: u32,
    ) -> Self {
        let bg = hex(background);
        let surface = hex(surface);
        let fg = hex(foreground);
        // Qt.darker(foreground, 1.4) scales HSV value, equivalently each RGB channel.
        let step = tagtheme::hex_of(tagtheme::parse(&fg).unwrap().map(|v| v / 1.4));
        let ensure = tagtheme::ensure_ratio;
        let text = |ink: &str| ensure(&ensure(ink, &bg, 4.5), &surface, 4.5);
        let muted_frame = ensure(
            &hex(pick(
                &["muted"],
                u32::from_str_radix(&step[1..], 16).unwrap(),
            )),
            &bg,
            3.,
        );
        let muted = text(&muted_frame);
        let red = hex(pick(&["red", "color1"], 0xf38ba8));
        let rgb = tagtheme::parse(&red).unwrap();
        let high = rgb.into_iter().fold(0.0_f64, f64::max);
        let low = rgb.into_iter().fold(1.0_f64, f64::min);
        // Theme.qml hueFloor: a grey urgent role falls back to foreground.
        let hued = high > 0. && (high - low) / high > 0.2;
        let error = |ground: &str| {
            if hued {
                ensure(
                    &red,
                    ground,
                    4.5_f64.max(tagtheme::ratio_of(
                        tagtheme::parse(&muted).unwrap(),
                        tagtheme::parse(ground).unwrap(),
                    )),
                )
            } else {
                fg.clone()
            }
        };
        Self {
            muted: colour(&muted),
            muted_frame: colour(&muted_frame),
            metadata: colour(&text(&step)),
            metadata_frame: colour(&step),
            symlink: colour(&ensure(&hex(pick(&["cyan", "color6"], 0x94e2d5)), &bg, 4.5)),
            executable: colour(&ensure(
                &hex(pick(&["green", "color2"], 0xa6e3a1)),
                &bg,
                4.5,
            )),
            error: colour(&error(&bg)),
            error_on_surface: colour(&error(&surface)),
        }
    }

    /// Explicit inherited colours retain their own ground and ink, including the contrast lifts.
    pub fn for_inherited(background: Hsla, foreground: Hsla) -> Self {
        let rgb = |colour: Hsla| {
            let rgba: Rgba = colour.into();
            ((rgba.r * 255.).round() as u32) << 16
                | ((rgba.g * 255.).round() as u32) << 8
                | (rgba.b * 255.).round() as u32
        };
        Self::resolve(
            &|_, fallback| fallback,
            rgb(background),
            rgb(foreground),
            rgb(background),
        )
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

    fn assert_accent_roles(case: &str, text: &str, foreground: u32, accent: u32, surface: u32) {
        let theme = Theme::from_text(text);
        assert_eq!(
            theme.foreground,
            gpui::rgb(foreground).into(),
            "foreground: {case}"
        );
        assert_eq!(
            theme.accent,
            gpui::rgb(accent).into(),
            "accent precedence: {case}"
        );
        assert_eq!(
            theme.surface,
            gpui::rgb(surface).into(),
            "surface precedence: {case}"
        );
        let rgb =
            |color| flea::ui_logic_pl4::tagtheme::parse(&crate::ipc::rows::colour(color)).unwrap();
        let ratio = flea::ui_logic_pl4::tagtheme::ratio_of;
        assert!(
            ratio(rgb(theme.accent_frame), rgb(theme.surface)) >= 3.,
            "frame contrast on surface: {case}"
        );
        if ratio(rgb(theme.accent), rgb(theme.surface)) >= 3. {
            assert_eq!(
                theme.accent_frame, theme.accent,
                "already legible frame: {case}"
            );
        } else {
            assert_ne!(theme.accent_frame, theme.accent, "raised frame: {case}");
        }
    }

    #[test]
    fn accent_precedence_explicit_wins_blue_and_color4() {
        assert_accent_roles(
            "explicit accent",
            "background='#101315'\nforeground='#cacccc'\naccent='#e5a55b'\nblue='#123456'\ncolor4='#abcdef'\ndark_background='#181825'\nselection='#ffffff'",
            0xcacccc,
            0xe5a55b,
            0x181825,
        );
    }

    #[test]
    fn accent_precedence_blue_wins_color4() {
        assert_accent_roles(
            "blue precedence",
            "background='#eff1f5'\nforeground='#4c4f69'\nblue='#1e66f5'\ncolor4='#abcdef'\nselection='#101315'",
            0x4c4f69,
            0x1e66f5,
            0xeff1f5,
        );
    }

    #[test]
    fn accent_precedence_color4_wins_foreground() {
        assert_accent_roles(
            "color4 precedence",
            "foreground='#abcdef'\ncolor4='#123456'\nselection='#101315'",
            0xabcdef,
            0x123456,
            0x101315,
        );
    }

    #[test]
    fn accent_precedence_custom_foreground_fallback() {
        assert_accent_roles(
            "custom foreground fallback",
            "background='#eff1f5'\nforeground='#112233'\ncolor7='#abcdef'",
            0x112233,
            0x112233,
            0xeff1f5,
        );
    }

    #[test]
    fn accent_precedence_color7_foreground_fallback() {
        assert_accent_roles(
            "color7 foreground fallback",
            "color7='#345678'",
            0x345678,
            0x345678,
            0x181825,
        );
    }
}
