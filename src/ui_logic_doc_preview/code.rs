//! A code reply's runs: the ink role each run draws in and its text with every tab filled to its stop.

/// The ink names after the foreground, in the order the code reply numbers its roles: 1 to 6 are bat's SGR 31 to 36.
pub const INKS: [&str; 6] = ["red", "green", "yellow", "blue", "magenta", "cyan"];
/// A tab fills to the next multiple of this many columns.
pub const TAB: usize = 8;
/// The inks a theme without that colour draws in, in INKS order.
pub const FALLBACK: [&str; 6] = [
    "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#cba6f7", "#94e2d5",
];
/// The theme's surface ladder: the chrome plane, then background, then selection (ui/js/Palette.js).
pub const SURFACE_KEYS: [&str; 3] = ["dark_background", "background", "selection"];
/// The contrast every ink and the line numbers hold on the ground they contrast less with.
pub const MIN_RATIO: f64 = 4.5;

/// The first key the theme set wins, else the fallback (ui/js/Palette.js pick).
fn pick<'a>(found: &impl Fn(&str) -> Option<&'a str>, keys: &[&str], fallback: &'a str) -> &'a str {
    keys.iter().find_map(|key| found(key)).unwrap_or(fallback)
}

// The ground of the two an ink contrasts less with, so the lift holds on both.
fn lifted(ink: &str, background: &str, surface: &str) -> String {
    let ground = if super::contrast::ratio(ink, background) < super::contrast::ratio(ink, surface) {
        background
    } else {
        surface
    };
    super::contrast::ensure_ratio(ink, ground, MIN_RATIO)
}

/// The foreground, then bat's six colours: the palette's own (its ANSI slot when unnamed), each lifted to 4.5:1.
pub fn inks<'a>(
    found: impl Fn(&str) -> Option<&'a str>,
    background: &'a str,
    surface: &'a str,
    foreground: &str,
) -> [String; 7] {
    let background = pick(&found, &["background"], background);
    let surface = pick(&found, &SURFACE_KEYS, surface);
    let mut out: [String; 7] = Default::default();
    out[0] = foreground.to_string();
    for (i, name) in INKS.iter().enumerate() {
        let slot = format!("color{}", i + 1);
        let ink = pick(&found, &[name, &slot], FALLBACK[i]);
        out[i + 1] = lifted(ink, background, surface);
    }
    out
}

/// The line numbers' ink: the muted role lifted to 4.5:1 on both grounds, where v0.3.12 drew it at 2.29:1.
pub fn gutter_ink(muted: &str, background: &str, surface: &str) -> String {
    lifted(muted, background, surface)
}

/// A role's ink name; 0 and any unknown role are the foreground.
pub fn role_name(role: i64) -> &'static str {
    if (1..=INKS.len() as i64).contains(&role) {
        INKS[role as usize - 1]
    } else {
        "foreground"
    }
}

/// The reply's runs as (role, text) pairs with every tab filled to its stop, counting the columns of the runs before it.
pub fn pairs(runs: &[(i64, &str)]) -> Vec<(i64, String)> {
    let mut column = 0;
    runs.iter()
        .map(|&(role, raw)| {
            let mut text = String::with_capacity(raw.len());
            for c in raw.chars() {
                if c == '\t' {
                    let fill = TAB - column % TAB;
                    text.extend(std::iter::repeat_n(' ', fill));
                    column += fill;
                } else {
                    text.push(c);
                    column += 1;
                }
            }
            (role, text)
        })
        .collect()
}

/// The line's text alone.
pub fn plain(runs: &[(i64, &str)]) -> String {
    pairs(runs).into_iter().map(|(_, text)| text).collect()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
