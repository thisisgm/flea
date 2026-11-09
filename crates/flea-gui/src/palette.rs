pub use crate::window::glass::Inputs as GlassInputs;
use flea::colours;
use gpui::{Global, Hsla, Rgba};

#[cfg(test)]
mod consumer_tests;
#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod tag_hue_tests;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub glass: GlassInputs,
    pub background: Hsla,
    pub foreground: Hsla,
    pub foreground_bright: Hsla,
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
    /// The code reply's roles 1 to 6, each the theme's own colour lifted to 4.5:1 (ui/js/CodeRuns.js inks).
    pub code_inks: [Hsla; 6],
    /// The seven solid tag swatches on the listing ground, resolved once per theme load.
    pub tag_listing_inks: [Hsla; 7],
    /// The shared tag palette's seven surface swatches, resolved once per theme load.
    pub tag_menu_inks: [Hsla; 7],
    /// The line numbers' ink, the metadata role lifted to 4.5:1 on both grounds.
    pub gutter: Hsla,
    /// The theme's own red, unlifted (Commons.Color.urgent: red or color1, last in source order), for inherited grounds.
    pub urgent: Hsla,
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
    pub fn for_inherited(background: Hsla, foreground: Hsla, urgent: Hsla) -> Self {
        // A frame repaints with the last ground and ink, so only a theme change resolves again.
        if let Some((key, roles)) = INHERITED.get() {
            if key == (background, foreground, urgent) {
                return roles;
            }
        }
        let opaque = |colour: Hsla| crate::ipc::rows::colour(Hsla { a: 1., ..colour });
        let (bg, fg, red) = (opaque(background), opaque(foreground), opaque(urgent));
        let text =
            format!("background='{bg}'\nforeground='{fg}'\ndark_background='{bg}'\ncolor1='{red}'");
        let roles =
            Self::from_core(&flea::theme::Theme::resolve(&text, "", "", None, "monospace").colors);
        INHERITED.set(Some(((background, foreground, urgent), roles)));
        #[cfg(test)]
        INHERITED_RESOLVES.set(INHERITED_RESOLVES.get() + 1);
        roles
    }
}

type Inherited = Option<((Hsla, Hsla, Hsla), Roles)>;
thread_local! {
    static INHERITED: std::cell::Cell<Inherited> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
thread_local! {
    pub(crate) static INHERITED_RESOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Drops the cached inherited roles and zeroes the resolve count, so a test counts only its own resolves.
#[cfg(test)]
pub(crate) fn forget_inherited() {
    INHERITED.set(None);
    INHERITED_RESOLVES.set(0);
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

mod defaults;

/// The requested output-image background, independent of the window theme.
pub(crate) fn output_background(
    background: flea::ui_logic_op4::composition::Background,
) -> Option<Hsla> {
    use flea::ui_logic_op4::composition::Background;
    match background {
        Background::Transparent => None,
        Background::White => Some(Hsla::white()),
        Background::Black => Some(Hsla::black()),
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
        // src/theme/tokens.rs takes the last red or color1 line, else #a55555, as Commons.Color.urgent.
        let urgent = values
            .iter()
            .rfind(|(key, _)| *key == "red" || *key == "color1")
            .map_or(0xa55555, |(_, value)| *value);
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
        let hex = |value: u32| format!("#{value:06x}");
        let (ground, plane) = (hex(background), hex(surface));
        let named: Vec<(&str, String)> = values
            .iter()
            .map(|(name, value)| (*name, hex(*value)))
            .collect();
        let lifted = flea::ui_logic_pv2::code::inks(
            |key| {
                named
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| value.as_str())
            },
            &ground,
            &plane,
            &hex(foreground),
        );
        let ink = |text: &str| -> Hsla {
            u32::from_str_radix(text.trim_start_matches('#'), 16)
                .map_or(gpui::rgb(foreground).into(), |value| {
                    gpui::rgb(value).into()
                })
        };
        let metadata = Rgba::from(role(core.metadata));
        let muted = format!(
            "#{:02x}{:02x}{:02x}",
            (metadata.r * 255.).round() as u8,
            (metadata.g * 255.).round() as u8,
            (metadata.b * 255.).round() as u8
        );
        let gutter = ink(&flea::ui_logic_pv2::code::gutter_ink(
            &muted, &ground, &plane,
        ));
        let tags = flea::ui_logic_pl4::tagtheme::tag_theme(
            |key| {
                named
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == key)
                    .map(|(_, hex)| hex.clone())
            },
            &ground,
            &plane,
        );
        let theme = Self {
            tag_listing_inks: tags.colors.each_ref().map(|hex| ink(hex)),
            glass: Default::default(),
            background: gpui::rgb(background).into(),
            foreground: gpui::rgb(foreground).into(),
            foreground_bright: role(core.foregroundBright),
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
            code_inks: [
                ink(&lifted[1]),
                ink(&lifted[2]),
                ink(&lifted[3]),
                ink(&lifted[4]),
                ink(&lifted[5]),
                ink(&lifted[6]),
            ],
            gutter,
            tag_menu_inks: tags.menu_colors.each_ref().map(|hex| ink(hex)),
            urgent: gpui::rgb(urgent).into(),
        };
        // ui/js/Palette.js:31-33 isPalette: any parsed colour makes the theme ready, ui/Theme.qml:357.
        let loaded = !values.is_empty();
        (theme, ParsedTheme(loaded))
    }

    /// HIG list and rail-favorites draw the chrome strip on color-mix(in srgb, --surface 55%, --bg).
    pub fn chrome(&self) -> Hsla {
        let (s, b): (Rgba, Rgba) = (self.surface.into(), self.background.into());
        let mix = |s: f32, b: f32| s * 0.55 + b * 0.45;
        Rgba {
            r: mix(s.r, b.r),
            g: mix(s.g, b.g),
            b: mix(s.b, b.b),
            a: 1.,
        }
        .into()
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

    #[test]
    fn inherited_roles_resolve_once_per_ground_and_ink() {
        let (ground, ink): (Hsla, Hsla) = (gpui::rgb(0x202020).into(), gpui::rgb(0xe0e0e0).into());
        let start = INHERITED_RESOLVES.get();
        let red: Hsla = gpui::rgb(0xbf616a).into();
        let first = Roles::for_inherited(ground, ink, red);
        assert_eq!(Roles::for_inherited(ground, ink, red), first);
        assert_eq!(INHERITED_RESOLVES.get(), start + 1);
        Roles::for_inherited(gpui::rgb(0xf0f0f0).into(), ink, red);
        assert_eq!(INHERITED_RESOLVES.get(), start + 2);
    }

    // Sample input: the stock catppuccin and tokyo-night colors.toml background, surface, foreground and red lines.
    const STOCK: [&str; 2] = [
        "background = \"#1e1e2e\"\ndark_background = \"#161622\"\nforeground = \"#cdd6f4\"\nred = \"#f38ba8\"",
        "background = \"#1a1b26\"\ndark_background = \"#13141c\"\nforeground = \"#a9b1d6\"\nred = \"#f7768e\"",
    ];

    #[test]
    fn an_inherited_ground_lifts_the_themes_own_red() {
        let hue = |colour: Hsla| colour.h;
        let mut errors = Vec::new();
        for text in STOCK {
            let theme = Theme::from_text(text);
            // The printer menu's ground: the card surface under the theme's ink.
            let style = gpui::TextStyle {
                color: theme.foreground,
                background_color: Some(theme.surface),
                ..Default::default()
            };
            let error = crate::list::Palette::inherited(&style, theme).error;
            // Lifting mixes toward black or white, which keeps the hue up to 8-bit rounding.
            assert!(
                (hue(error) - hue(theme.urgent)).abs() < 0.005,
                "{text}: {error:?} is not {:?}",
                theme.urgent
            );
            errors.push(error);
        }
        assert_ne!(
            errors[0], errors[1],
            "two stock reds must give two inherited reds"
        );
    }
}
