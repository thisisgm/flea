use super::parse::{finite, merge_shell};
use super::{
    ensure_ratio, parse_colors, parse_shell, ratio, Color, Decoration, Font, Spacing, Values,
};
use std::collections::{BTreeMap, BTreeSet};

pub const TAG_NAMES: [&str; 7] = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"];
pub const TAG_ROLES: [&str; 7] = [
    "red",
    "orange",
    "yellow",
    "green",
    "blue",
    "magenta",
    "dark_foreground",
];

#[derive(Clone, Debug)]
pub struct TagTheme {
    pub colors: [Option<Color>; 7],
    pub menu_colors: [Option<Color>; 7],
    pub fallback: bool,
    pub swatch_size: u32,
}

impl TagTheme {
    pub fn resolve(found: &BTreeMap<String, Color>, bg: Color, surface: Color) -> Self {
        let mut out = Self {
            colors: [None; 7],
            menu_colors: [None; 7],
            fallback: false,
            swatch_size: 14,
        };
        let mut seen = BTreeSet::new();
        for (i, key) in TAG_ROLES.iter().enumerate() {
            if let Some(&c) = found.get(*key) {
                if !seen.insert(c.hex()) || (i != 6 && c.saturation() < 0.12) {
                    out.fallback = true;
                }
                out.colors[i] = Some(ensure_ratio(c, bg, 3.0));
                out.menu_colors[i] = Some(ensure_ratio(c, surface, 3.0));
            } else {
                out.fallback = true;
            }
        }
        out
    }
}

// Field names are the QML contract, so golden rows and GUI migration use exactly the same names.
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct ThemeColors {
    pub background: Color,
    pub foreground: Color,
    pub opaqueForegroundBright: Color,
    pub foregroundBright: Color,
    pub opaqueMuted: Color,
    pub muted: Color,
    pub opaqueMutedFrame: Color,
    pub mutedFrame: Color,
    pub opaqueMetadata: Color,
    pub metadata: Color,
    pub opaqueMetadataFrame: Color,
    pub metadataFrame: Color,
    pub accent: Color,
    pub opaqueError: Color,
    pub error: Color,
    pub opaqueErrorOnSurface: Color,
    pub errorOnSurface: Color,
    pub surface: Color,
    pub opaqueSymlink: Color,
    pub symlink: Color,
    pub opaqueExecutable: Color,
    pub executable: Color,
    pub opaqueAccentFrame: Color,
    pub accentFrame: Color,
    pub accentHasHue: bool,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub colors: ThemeColors,
    pub palette: BTreeMap<String, Color>,
    pub values: Values,
    pub font: Font,
    pub spacing: Spacing,
    pub tags: TagTheme,
    pub decoration: Decoration,
    pub ready: bool,
    pub fills: BTreeMap<String, Color>,
    pub numbers: BTreeMap<String, f64>,
}

fn hex(s: &str) -> Color {
    Color::parse(s).expect("literal color")
}

impl Theme {
    // Pass None to follow Commons, or Some(12)/Some(14) to pin a Flea size stop.
    pub fn resolve(
        colors: &str,
        shell: &str,
        user_shell: &str,
        stop: Option<u32>,
        family: &str,
    ) -> Self {
        let palette = parse_colors(colors);
        let pick = |keys: &[&str], default| {
            keys.iter()
                .find_map(|k| palette.get(*k).copied())
                .unwrap_or(default)
        };
        let bg = pick(&["background", "color0"], hex("#101315"));
        let fg = pick(&["foreground", "color7"], hex("#cacccc"));
        let accent = pick(&["accent", "color4"], hex("#cacccc"));
        let mut urgent = hex("#a55555");
        // Commons assigns red/color1 in source order, unlike the first-key palette ladders.
        for line in colors.lines() {
            let found = parse_colors(line);
            if let Some(c) = found.get("red").or_else(|| found.get("color1")) {
                urgent = *c;
            }
        }
        let surface = pick(
            &["dark_background", "background", "selection"],
            hex("#181825"),
        );
        let contrast_bg = pick(&["background"], hex("#101315"));
        let muted_frame = ensure_ratio(pick(&["muted"], fg.darker()), contrast_bg, 3.0);
        let muted = ensure_ratio(ensure_ratio(muted_frame, contrast_bg, 4.5), surface, 4.5);
        let metadata_frame = fg.darker();
        let metadata = ensure_ratio(ensure_ratio(metadata_frame, contrast_bg, 4.5), surface, 4.5);
        let bright = pick(&["bright_foreground", "color15"], fg);
        let bright = if ratio(bright, contrast_bg) > ratio(fg, contrast_bg) {
            bright
        } else {
            fg
        };
        let accent_frame = ensure_ratio(accent, surface, 3.0);
        let error = |ground| {
            if urgent.qcolor_saturation() > 0.2 {
                ensure_ratio(urgent, ground, 4.5_f64.max(ratio(muted, ground)))
            } else {
                fg
            }
        };
        let symlink = ensure_ratio(pick(&["cyan", "color6"], hex("#94e2d5")), contrast_bg, 4.5);
        let executable = ensure_ratio(pick(&["green", "color2"], hex("#a6e3a1")), contrast_bg, 4.5);
        let colors = ThemeColors {
            background: bg,
            foreground: fg,
            opaqueForegroundBright: bright,
            foregroundBright: if ratio(bright, bg) > ratio(fg, bg) {
                bright
            } else {
                fg
            },
            opaqueMuted: muted,
            muted,
            opaqueMutedFrame: muted_frame,
            mutedFrame: muted_frame,
            opaqueMetadata: metadata,
            metadata,
            opaqueMetadataFrame: metadata_frame,
            metadataFrame: metadata_frame,
            accent,
            opaqueError: error(contrast_bg),
            error: error(contrast_bg),
            opaqueErrorOnSurface: error(surface),
            errorOnSurface: error(surface),
            surface,
            opaqueSymlink: symlink,
            symlink,
            opaqueExecutable: executable,
            executable,
            opaqueAccentFrame: accent_frame,
            accentFrame: accent_frame,
            accentHasHue: accent.qcolor_saturation() > 0.2,
        };
        let mut values = parse_shell(shell);
        values.extend(parse_shell(user_shell));
        let font = Font::resolve(&values, stop, family);
        let spacing = Spacing::resolve(&values, Font::resolve(&values, None, family).base_size);
        let tags = TagTheme::resolve(&palette, contrast_bg, surface);
        let mut out = Self {
            colors,
            ready: !palette.is_empty(),
            palette,
            values,
            font,
            spacing,
            tags,
            decoration: Decoration {
                gaps_out: 5,
                ..Decoration::default()
            },
            fills: BTreeMap::new(),
            numbers: BTreeMap::new(),
        };
        out.resolve_fills(fg, accent, urgent, &merge_shell(shell, user_shell));
        out
    }

    fn state_color(
        &self,
        token: &str,
        fg: Color,
        accent: Color,
        urgent: Color,
        fallback: Color,
    ) -> Color {
        match token.trim().to_ascii_lowercase().as_str() {
            "foreground" | "text" => fg,
            "accent" => accent,
            "urgent" => urgent,
            "background" => self.colors.background,
            "transparent" => Color::BLACK.alpha(0.0),
            _ => Color::parse(token).unwrap_or(fallback),
        }
    }

    fn resolve_fills(
        &mut self,
        fg: Color,
        accent: Color,
        urgent: Color,
        entries: &[(String, String)],
    ) {
        let mut controls = Values::new();
        for (key, value) in entries {
            if let Some(k) = key
                .strip_prefix("controls.")
                .or_else(|| key.strip_prefix("style."))
            {
                controls.insert(k.to_owned(), value.clone());
            }
        }
        let mut hover = fg;
        let mut hover_token = "foreground".to_owned();
        let mut normal_width = 1.0;
        let mut hover_width = 1.0;
        for (name, key, fill, border) in [
            ("normal", "normal", 0.04, Some(0.4)),
            ("hover", "hover-cursor", 0.08, Some(0.25)),
            ("selected", "selected", 0.18, Some(1.0)),
            ("pressed", "pressed", 0.22, None),
            ("focus", "focus", 0.08, Some(0.25)),
            ("selection", "selection", 0.35, None),
        ] {
            let token = controls
                .get(&format!("{key}-color"))
                .filter(|s| !s.trim().is_empty())
                .map(String::as_str)
                .unwrap_or(if matches!(name, "pressed" | "focus") {
                    &hover_token
                } else {
                    "foreground"
                });
            let color = if name == "focus"
                && matches!(
                    token.to_ascii_lowercase().as_str(),
                    "hover" | "hover-cursor" | "inherit"
                ) {
                hover
            } else {
                self.state_color(
                    token,
                    fg,
                    accent,
                    urgent,
                    if matches!(name, "pressed" | "focus") {
                        hover
                    } else {
                        fg
                    },
                )
            };
            if name == "hover" {
                hover = color;
                hover_token = token.to_owned();
            }
            let alpha = finite(
                &controls,
                &format!("{key}-fill-alpha"),
                if name == "focus" {
                    finite(&controls, "hover-cursor-fill-alpha", fill)
                } else {
                    fill
                },
            )
            .clamp(0.0, 1.0);
            let alpha = if alpha == 0.0 { 0.0 } else { alpha };
            self.fills
                .insert(format!("style.{name}Fill"), color.alpha(alpha));
            self.numbers.insert(format!("style.{name}FillAlpha"), alpha);
            if let Some(border) = border {
                let fallback = match name {
                    "normal" => 1.0,
                    "hover" => normal_width,
                    "focus" => hover_width,
                    _ => 0.0,
                };
                let width = finite(&controls, &format!("{key}-border-width"), fallback);
                let width = if width > 0.0 { width.round() } else { 0.0 };
                if name == "normal" {
                    normal_width = width;
                }
                if name == "hover" {
                    hover_width = width;
                }
                self.numbers
                    .insert(format!("style.{name}BorderWidth"), width);
                let alpha = finite(
                    &controls,
                    &format!("{key}-border-alpha"),
                    if name == "focus" {
                        finite(&controls, "hover-cursor-border-alpha", border)
                    } else {
                        border
                    },
                )
                .clamp(0.0, 1.0);
                let alpha = if alpha == 0.0 { 0.0 } else { alpha };
                self.fills
                    .insert(format!("style.{name}BorderColor"), color.alpha(alpha));
            }
        }
        let selected = self.numbers["style.selectedFillAlpha"];
        self.fills
            .insert("style.selectedAccentFill".into(), accent.alpha(selected));
        let inactive = self.fills["style.selectedFill"];
        self.fills.insert("inactiveCursorWash".into(), inactive);
        for (name, color, alpha) in [
            ("cursorWash", accent, selected),
            ("menuCursorWash", fg, 0.08),
            ("windowHairline", fg, 0.12),
            ("rule", self.colors.mutedFrame, 0.4),
            ("primaryWash", accent, 0.14),
        ] {
            self.fills.insert(name.into(), color.alpha(alpha));
        }
        let selection = self.fills["style.selectionFill"];
        let mut alpha = (selection.a * 100.0).round() as i32;
        while alpha > 12
            && ratio(
                fg,
                selection
                    .alpha(alpha as f64 / 100.0)
                    .over(self.colors.background)
                    .quantized(),
            ) < 4.5
        {
            alpha -= 1;
        }
        self.fills
            .insert("markedPlate".into(), selection.alpha(alpha as f64 / 100.0));
        self.numbers
            .insert("markedPlateAlpha".into(), alpha as f64 / 100.0);
    }
}
