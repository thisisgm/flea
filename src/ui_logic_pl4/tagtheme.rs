//! Tag swatch colours per theme: each role lifted to 3:1 on the listing and menu grounds, plus the digit fallback.
use super::tags::NAMES;

// The palette role each tag takes, member for member with ui/js/Contrast.js TAG_ROLES.
pub const ROLES: [&str; 7] = [
    "red",
    "orange",
    "yellow",
    "green",
    "blue",
    "magenta",
    "dark_foreground",
];
pub const MIN_RATIO: f64 = 3.0;
// Below this share of chroma a colour is a grey whatever its name claims.
pub const GREY_CHROMA: f64 = 0.12;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagTheme {
    // "#rrggbb" per tag in NAMES order, empty where the palette has no colour.
    pub colors: [String; 7],
    pub menu_colors: [String; 7],
    pub fallback: bool,
}

// Sample input: "#c8ccd0" or "ffc8ccd0"; None for anything that is not six or eight hex digits.
pub fn parse(hex: &str) -> Option<[f64; 3]> {
    let s = hex.strip_prefix('#').unwrap_or(hex);
    let s = if s.len() == 8 { &s[2..] } else { s };
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| {
        u8::from_str_radix(&s[at..at + 2], 16)
            .ok()
            .map(|v| f64::from(v) / 255.0)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

// WCAG 2.1 relative luminance.
pub fn luminance(rgb: [f64; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

pub fn ratio_of(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

pub fn hex_of(rgb: [f64; 3]) -> String {
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2])
    )
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

// Keep the hue and walk lightness toward black or white until the ratio holds, as Contrast.ensureRatio does.
pub fn ensure_ratio(fg_hex: &str, bg_hex: &str, need: f64) -> String {
    let (Some(fg), Some(bg)) = (parse(fg_hex), parse(bg_hex)) else {
        return fg_hex.to_owned();
    };
    if ratio_of(fg, bg) >= need {
        return fg_hex.to_owned();
    }
    let toward = if luminance(bg) > 0.179 {
        [0.0; 3]
    } else {
        [1.0; 3]
    };
    let (mut lo, mut hi, mut best) = (0.0, 1.0, toward);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        let candidate = mix(fg, toward, mid);
        if ratio_of(candidate, bg) >= need {
            best = candidate;
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if ratio_of(best, bg) < need {
        best = toward;
    }
    stepped(&hex_of(best), bg, need, toward)
}

// Rounding to 8 bits can land just under the ratio, so step each channel on its own until it holds or cannot move.
fn stepped(hex: &str, bg: [f64; 3], need: f64, toward: [f64; 3]) -> String {
    let mut c = parse(hex).unwrap_or(toward);
    let step = if toward[0] > 0.5 {
        1.0 / 255.0
    } else {
        -1.0 / 255.0
    };
    for _ in 0..255 {
        if ratio_of(c, bg) >= need {
            break;
        }
        let next = c.map(|v| (v + step).clamp(0.0, 1.0));
        if hex_of(next) == hex_of(c) {
            break;
        }
        c = next;
    }
    hex_of(c)
}

// A colour with almost no chroma is a grey, whatever its name claims.
pub fn is_grey(rgb: [f64; 3]) -> bool {
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    let lo = rgb[0].min(rgb[1]).min(rgb[2]);
    hi == 0.0 || (hi - lo) / hi < GREY_CHROMA
}

// Decided once per theme load: the digit fallback only for a missing colour, an exact shared colour or a grey (README Tags).
pub fn tag_theme(role: impl Fn(&str) -> Option<String>, bg: &str, surface: &str) -> TagTheme {
    let mut theme = TagTheme::default();
    let mut seen: Vec<String> = Vec::new();
    for (at, name) in NAMES.iter().enumerate() {
        let hex = role(ROLES[at]).filter(|hex| parse(hex).is_some());
        let Some(hex) = hex else {
            theme.fallback = true;
            continue;
        };
        let key = hex.to_ascii_lowercase();
        theme.fallback |= seen.contains(&key);
        seen.push(key);
        theme.colors[at] = ensure_ratio(&hex, bg, MIN_RATIO);
        theme.menu_colors[at] = ensure_ratio(&hex, surface, MIN_RATIO);
        if *name != "Gray" && parse(&hex).is_some_and(is_grey) {
            theme.fallback = true;
        }
    }
    theme
}
