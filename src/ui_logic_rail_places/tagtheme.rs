//! Tag swatch colours per theme: each role, or the canonical colour where the role is far from its name or looks like an earlier tag, lifted to 3:1 on the listing and menu grounds, plus the digit fallback.
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
// The hue window, in degrees, each colour tag's name allows; a window may wrap past red (v0.3.12 Contrast.js TAG_HUES).
pub const HUE_WINDOWS: [(f64, f64); 6] = [
    (335.0, 12.0),
    (8.0, 45.0),
    (28.0, 70.0),
    (80.0, 170.0),
    (195.0, 255.0),
    (250.0, 335.0),
];
// A slot further than this outside its tag's window reads as another colour, so the tag draws its canonical colour.
pub const FAR_FROM_NAME: f64 = 90.0;
// The canonical tag colours: Omarchy's default theme, tokyo-night, red through magenta, then its dark_foreground for Gray.
pub const CANONICAL: [&str; 7] = [
    "#f7768e", "#eb927b", "#e0af68", "#9ece6a", "#7aa2f7", "#ad8ee6", "#565f89",
];
// Two tag inks nearer than this in OKLab read as one colour on a small square: twice the 0.02 step CSS Color 4 treats as just noticeable.
pub const DISTINCT_OKLAB: f64 = 0.04;

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

// HSV hue in degrees, 0 for a grey.
pub fn hue(rgb: [f64; 3]) -> f64 {
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    let delta = hi - rgb[0].min(rgb[1]).min(rgb[2]);
    if delta == 0.0 {
        0.0
    } else if hi == rgb[0] {
        60.0 * ((rgb[1] - rgb[2]) / delta).rem_euclid(6.0)
    } else if hi == rgb[1] {
        60.0 * ((rgb[2] - rgb[0]) / delta + 2.0)
    } else {
        60.0 * ((rgb[0] - rgb[1]) / delta + 4.0)
    }
}

// Degrees from a hue to the nearer edge of a window, 0 inside it.
pub fn outside(hue: f64, (lo, hi): (f64, f64)) -> f64 {
    let inside = if lo <= hi {
        (lo..=hi).contains(&hue)
    } else {
        hue >= lo || hue <= hi
    };
    if inside {
        return 0.0;
    }
    let apart = |a: f64, b: f64| {
        let d = (a - b).abs();
        d.min(360.0 - d)
    };
    apart(hue, lo).min(apart(hue, hi))
}

// The colour a tag draws before the lift: the theme's own unless it is a colour far from the tag's name, as hackerman's green red.
pub fn named(at: usize, hex: &str) -> String {
    match (HUE_WINDOWS.get(at), parse(hex)) {
        (Some(&window), Some(rgb))
            if !is_grey(rgb) && outside(hue(rgb), window) > FAR_FROM_NAME =>
        {
            CANONICAL[at].to_owned()
        }
        _ => hex.to_owned(),
    }
}

// Euclidean distance between two "#rrggbb" inks in OKLab; infinite when either is not a colour, as an empty slot.
pub fn oklab_distance(a: &str, b: &str) -> f64 {
    let (Some(a), Some(b)) = (parse(a), parse(b)) else {
        return f64::INFINITY;
    };
    let lab = |rgb: [f64; 3]| crate::ui_logic_model_preview::color::oklab(rgb.map(|c| c as f32));
    let (a, b) = (lab(a), lab(b));
    let squared: f32 = (0..3).map(|i| (a[i] - b[i]).powi(2)).sum();
    f64::from(squared.sqrt())
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
        let drawn = named(at, &hex);
        theme.colors[at] = ensure_ratio(&drawn, bg, MIN_RATIO);
        theme.menu_colors[at] = ensure_ratio(&drawn, surface, MIN_RATIO);
        if *name != "Gray" && parse(&hex).is_some_and(is_grey) {
            theme.fallback = true;
        }
    }
    keep_apart(&mut theme, bg, surface);
    theme
}

// Two tags alike on either ground cannot be told apart: the later draws its canonical colour, or the earlier once the later has, until none are alike.
fn keep_apart(theme: &mut TagTheme, bg: &str, surface: &str) {
    let alike = |theme: &TagTheme, a: usize, b: usize| {
        oklab_distance(&theme.colors[a], &theme.colors[b]) < DISTINCT_OKLAB
            || oklab_distance(&theme.menu_colors[a], &theme.menu_colors[b]) < DISTINCT_OKLAB
    };
    let mut canonical = [false; NAMES.len()];
    'settle: loop {
        for later in 0..NAMES.len() {
            for earlier in 0..later {
                if !alike(theme, earlier, later) {
                    continue;
                }
                // Both already canonical: nothing is left to swap, and the canonical set is held apart by its own test.
                let Some(at) = [later, earlier].into_iter().find(|&at| !canonical[at]) else {
                    continue;
                };
                canonical[at] = true;
                theme.colors[at] = ensure_ratio(CANONICAL[at], bg, MIN_RATIO);
                theme.menu_colors[at] = ensure_ratio(CANONICAL[at], surface, MIN_RATIO);
                continue 'settle;
            }
        }
        return;
    }
}
