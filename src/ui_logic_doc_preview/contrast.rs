//! WCAG 2.1 contrast and the lift that walks an ink toward black or white until it holds a ratio, as ui/js/Contrast.js.

/// An sRGB colour with channels from 0 to 1.
pub type Rgb = [f64; 3];

/// "#c8ccd0" or "#ffc8ccd0" (alpha first, ignored); None for anything else.
pub fn parse(hex: &str) -> Option<Rgb> {
    let s = hex.strip_prefix('#').unwrap_or(hex);
    let s = if s.len() == 8 { &s[2..] } else { s };
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let channel = |at: usize| {
        u8::from_str_radix(&s[at..at + 2], 16)
            .ok()
            .map(|v| f64::from(v) / 255.)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// Lowercase "#rrggbb", each channel clamped and rounded to 8 bits.
pub fn hex(rgb: Rgb) -> String {
    let byte = |v: f64| (v.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2])
    )
}

fn channel(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(rgb: Rgb) -> f64 {
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

pub fn ratio_of(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The ratio of two hex colours; an unparsable one counts as black.
pub fn ratio(fg: &str, bg: &str) -> f64 {
    ratio_of(parse(fg).unwrap_or([0.; 3]), parse(bg).unwrap_or([0.; 3]))
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Keeps the hue and walks lightness toward black or white until the ratio holds; an ink that already holds is kept as given.
pub fn ensure_ratio(fg: &str, bg: &str, min_ratio: f64) -> String {
    let need = if min_ratio > 0. { min_ratio } else { 4.5 };
    if ratio(fg, bg) >= need {
        return fg.to_string();
    }
    let (f, b) = (parse(fg).unwrap_or([0.; 3]), parse(bg).unwrap_or([0.; 3]));
    let toward = if luminance(b) > 0.179 {
        [0.; 3]
    } else {
        [1.; 3]
    };
    let (mut lo, mut hi, mut best) = (0., 1., toward);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.;
        let candidate = mix(f, toward, mid);
        if ratio_of(candidate, b) >= need {
            best = candidate;
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if ratio_of(best, b) < need {
        best = toward;
    }
    stepped(&hex(best), b, need, toward)
}

// Rounding to 8 bits can land just under the ratio asked for, so the hex walks one step at a time until it holds.
fn stepped(start: &str, bg: Rgb, need: f64, toward: Rgb) -> String {
    let mut c = parse(start).unwrap_or(toward);
    let step = if toward[0] > 0.5 {
        1. / 255.
    } else {
        -1. / 255.
    };
    for _ in 0..255 {
        if ratio_of(c, bg) >= need {
            break;
        }
        let next = [
            (c[0] + step).clamp(0., 1.),
            (c[1] + step).clamp(0., 1.),
            (c[2] + step).clamp(0., 1.),
        ];
        if hex(next) == hex(c) {
            break;
        }
        c = next;
    }
    hex(c)
}

#[cfg(test)]
#[path = "contrast_tests.rs"]
mod tests;
