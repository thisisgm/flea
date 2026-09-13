use std::collections::HashMap;

pub struct Theme {
    pub foreground: String,
    pub muted: String,
    pub accent: String,
    pub error: String,
    pub background: String,
    pub symlink: String,
    pub executable: String,
    pub selected: String,
    pub border: String,
}
impl Theme {
    pub fn load() -> Self {
        let path = std::env::var("HOME").unwrap_or_default()
            + "/.local/state/omarchy/current/theme/colors.toml";
        Self::from_text(&std::fs::read_to_string(path).unwrap_or_default())
    }
    pub(super) fn from_text(text: &str) -> Self {
        let values = parse(text);
        let pick = |keys: &[&str], fallback: &str| {
            keys.iter().find_map(|key| values.get(*key)).map(String::as_str).unwrap_or(fallback).to_owned()
        };
        let foreground = pick(&["foreground", "color7"], "#cacccc");
        let background = pick(&["background", "color0"], "#101315");
        let accent = pick(&["accent", "color4"], "#cacccc");
        let surface = pick(&["dark_background", "background", "selection"], "#181825");
        let role_background = pick(&["background"], "#101315");
        let muted = contrast(&contrast(&pick(&["muted"], "#707880"), &role_background), &surface);
        Self {
            foreground: ansi(&foreground, false),
            muted: ansi(&muted, false),
            accent: ansi(&accent, false),
            error: ansi(&pick(&["urgent"], "#a55555"), false),
            background: ansi(&background, true),
            symlink: ansi(&contrast(&pick(&["cyan", "color6"], "#94e2d5"), &role_background), false),
            executable: ansi(&contrast(&pick(&["green", "color2"], "#a6e3a1"), &role_background), false),
            selected: ansi(&blend(&accent, &background, 0.22), true),
            border: ansi(&muted, false),
        }
    }
}
fn blend(foreground: &str, background: &str, opacity: f64) -> String {
    let channel = |i| {
        let fg = u8::from_str_radix(&foreground[i..i + 2], 16).unwrap_or(0) as f64;
        let bg = u8::from_str_radix(&background[i..i + 2], 16).unwrap_or(0) as f64;
        (fg * opacity + bg * (1.0 - opacity)).round() as u8
    };
    format!("#{:02x}{:02x}{:02x}", channel(1), channel(3), channel(5))
}
// Sample input: accent = "#a9b665"; values outside six-digit RGB are ignored.
fn parse(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_start_matches(['"', '\'']);
            let value = value.get(..7).unwrap_or("");
            if value.len() == 7
                && value.starts_with('#')
                && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                out.insert(key.trim().into(), value.into());
                if matches!(key.trim(), "red" | "color1") { out.insert("urgent".into(), value.into()); }
            }
        }
    }
    out
}
fn rgb(hex: &str) -> [f64; 3] {
    [1, 3, 5].map(|start| u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or(0) as f64 / 255.0)
}
fn luminance(rgb: [f64; 3]) -> f64 {
    let linear = rgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}
fn ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn contrast(foreground: &str, background: &str) -> String {
    // Keep this calculation aligned with ui/js/Contrast.js, including its rounded RGB result.
    const MIN_RATIO: f64 = 4.5;
    const SEARCH_STEPS: usize = 18;
    let (fg, bg) = (rgb(foreground), rgb(background));
    if ratio(fg, bg) >= MIN_RATIO { return foreground.into(); }
    let toward = if luminance(bg) > 0.179 { 0.0 } else { 1.0 };
    let (mut low, mut high) = (0.0, 1.0);
    let mut best = [toward; 3];
    for _ in 0..SEARCH_STEPS {
        let mid = (low + high) / 2.0;
        let candidate = fg.map(|c| c + (toward - c) * mid);
        if ratio(candidate, bg) >= MIN_RATIO { best = candidate; high = mid; }
        else { low = mid; }
    }
    let [red, green, blue] = best.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{red:02x}{green:02x}{blue:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_matches_oem_precedence_and_flea_roles() {
        let theme = Theme::from_text("color7='#112233'\ncolor0='#010203'\ncolor4='#445566'\naccent='#123456'\nred='#654321'\ncolor1='#abcdef'");
        assert_eq!(theme.foreground, ansi("#112233", false));
        assert_eq!(theme.background, ansi("#010203", true));
        assert_eq!(theme.accent, ansi("#123456", false));
        assert_eq!(theme.error, ansi("#abcdef", false));
        assert_eq!(Theme::from_text("color1='#abcdef'\nred='#654321'").error, ansi("#654321", false));
        assert_eq!(Theme::from_text("foreground='oops'").foreground, ansi("#cacccc", false));
        assert_eq!(contrast("#ffffff", "#000000"), "#ffffff");
        assert_eq!(contrast("#707880", "#101315"), "#767e85");
    }
}
fn ansi(hex: &str, bg: bool) -> String {
    let channel = |start| u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or(0);
    format!(
        "\x1b[{};2;{};{};{}m",
        if bg { 48 } else { 38 },
        channel(1),
        channel(3),
        channel(5)
    )
}
