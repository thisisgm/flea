use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::io::Read;

// The one background fallback, and ui/Theme.qml carries the same literal for the same reason.
const BACKGROUND_FALLBACK: &str = "#101315";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Depth { Sixteen, Indexed, #[default] Truecolor }
impl Depth {
    pub fn detect(cancel: &AtomicBool) -> Self {
        let output = |name: &str, arguments: &[&str]| {
            let mut command = std::process::Command::new(name);
            command.args(arguments);
            super::icons::output(&mut command, cancel).unwrap_or_default()
        };
        let tmux = std::env::var_os("TMUX").is_some_and(|value| !value.is_empty());
        let rgb = if tmux {
            output("tmux", &["display-message", "-p", "#{client_termfeatures}"]).split(',').any(|s| s.trim() == "RGB")
        } else {
            matches!(std::env::var("COLORTERM").unwrap_or_default().as_str(), "truecolor" | "24bit")
        };
        if rgb { Self::Truecolor }
        else if output("tput", &["colors"]).trim().parse::<usize>().unwrap_or(0) >= 256 { Self::Indexed }
        else { Self::Sixteen }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Theme {
    pub foreground: String,
    pub muted: String,
    pub meta: String,
    pub accent: String,
    pub error: String,
    pub background: String,
    pub symlink: String,
    pub executable: String,
    pub selected: String,
    pub cursor: String,
    pub marked: String,
    pub tab: String,
    pub match_fill: String,
    pub border: String,
    pub depth: Depth,
}
impl Theme {
    pub fn path() -> std::path::PathBuf {
        std::env::var_os("HOME").filter(|s| !s.is_empty()).map(std::path::PathBuf::from)
            .map(|home| home.join(".local/state/omarchy/current/theme/colors.toml")).unwrap_or_default()
    }
    #[cfg(test)]
    pub fn load() -> Self { Self::from_text(&read(&Self::path()).unwrap_or_default()) }
    pub fn base(&self) -> String { format!("\x1b[0m{}{}", self.background, self.foreground) }
    #[cfg(test)]
    pub(super) fn from_text(text: &str) -> Self { Self::with_depth(text, Depth::Truecolor) }
    pub fn with_depth(text: &str, depth: Depth) -> Self {
        let values = parse(text);
        let pick = |keys: &[&str], fallback: &str| {
            keys.iter().find_map(|key| values.get(*key)).map(String::as_str).unwrap_or(fallback).to_owned()
        };
        let foreground = pick(&["foreground", "color7"], "#cacccc");
        let background = pick(&["background", "color0"], BACKGROUND_FALLBACK);
        let accent = pick(&["accent", "color4", "blue"], "#cacccc");
        let surface = pick(&["dark_background", "background", "selection", "color0"], BACKGROUND_FALLBACK);
        let darker = rgb(&foreground).map(|v| (v * 255.0 / 1.4).round() as u8);
        let darker = format!("#{:02x}{:02x}{:02x}", darker[0], darker[1], darker[2]);
        let muted = contrast(&contrast(&pick(&["muted"], &darker), &background), &surface);
        let meta = contrast(&contrast(&darker, &background), &surface);
        let mut mark_alpha = 0.35;
        while mark_alpha > 0.12 && ratio(rgb(&foreground), rgb(&blend(&foreground, &background, mark_alpha))) < 4.5 { mark_alpha -= 0.01; }
        let error = pick(&["urgent"], "#a55555");
        let channels = rgb(&error);
        let high = channels.into_iter().fold(0.0_f64, f64::max);
        let low = channels.into_iter().fold(1.0_f64, f64::min);
        let error = if high == 0.0 || (high - low) / high <= 0.2 { foreground.clone() } else { contrast(&error, &background) };
        let encode = |hex: &str, bg| color(hex, bg, depth);
        Self {
            foreground: encode(&foreground, false), muted: encode(&muted, false), meta: encode(&meta, false),
            accent: encode(&ensure(&accent, &surface, 3.0), false), error: encode(&error, false),
            background: encode(&background, true),
            symlink: encode(&contrast(&pick(&["cyan", "color6"], "#94e2d5"), &background), false),
            executable: encode(&contrast(&pick(&["green", "color2"], "#a6e3a1"), &background), false),
            selected: encode(&blend(&foreground, &background, 0.18), true),
            cursor: encode(&blend(&accent, &background, 0.18), true),
            marked: encode(&blend(&foreground, &background, mark_alpha), true),
            tab: encode(&blend(&foreground, &background, 0.08), true),
            match_fill: encode(&blend(&foreground, &background, 0.14), true),
            border: encode(&blend(&foreground, &background, 0.12), false), depth,
        }
    }
}
fn read(path: &std::path::Path) -> Option<String> {
    let file = crate::backend::regfile::open_if_regular(path, 0).ok()?;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= 65536).then(|| String::from_utf8_lossy(&bytes).into_owned())
}
// A single worker follows theme switches without file reads in the event loop.
pub struct Watch {
    pub updates: std::sync::mpsc::Receiver<String>,
    stop: std::sync::mpsc::Sender<()>,
}
impl Watch {
    pub fn start(path: std::path::PathBuf) -> Self {
        let (tx, updates) = std::sync::mpsc::sync_channel(1);
        let (stop, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut previous = None;
            loop {
                if let Some(text) = read(&path) {
                    if previous.as_ref() != Some(&text) && tx.try_send(text.clone()).is_ok() { previous = Some(text); }
                }
                if wait.recv_timeout(std::time::Duration::from_millis(500)).is_ok() { break; }
            }
        });
        Self { updates, stop }
    }
}
impl Drop for Watch { fn drop(&mut self) { let _ = self.stop.send(()); } }
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
fn contrast(foreground: &str, background: &str) -> String { ensure(foreground, background, 4.5) }
fn ensure(foreground: &str, background: &str, minimum: f64) -> String {
    // Keep this calculation aligned with ui/js/Contrast.js, including its rounded RGB result.
    const SEARCH_STEPS: usize = 18;
    let (fg, bg) = (rgb(foreground), rgb(background));
    if ratio(fg, bg) >= minimum { return foreground.into(); }
    let toward = if luminance(bg) > 0.179 { 0.0 } else { 1.0 };
    let (mut low, mut high) = (0.0, 1.0);
    let mut best = [toward; 3];
    for _ in 0..SEARCH_STEPS {
        let mid = (low + high) / 2.0;
        let candidate = fg.map(|c| c + (toward - c) * mid);
        if ratio(candidate, bg) >= minimum { best = candidate; high = mid; }
        else { low = mid; }
    }
    let [red, green, blue] = best.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{red:02x}{green:02x}{blue:02x}")
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

fn color(hex: &str, background: bool, depth: Depth) -> String {
    if depth == Depth::Truecolor { return ansi(hex, background); }
    let target = rgb(hex).map(|v| (v * 255.0).round() as i32);
    let range = if depth == Depth::Indexed { 16..256 } else { 0..16 };
    let index = range.min_by_key(|&index| {
        let candidate = indexed(index);
        target.into_iter().zip(candidate).map(|(a, b)| (a - b).pow(2)).sum::<i32>()
    }).unwrap_or(7);
    if depth == Depth::Indexed { format!("\x1b[{};5;{index}m", if background { 48 } else { 38 }) }
    else { format!("\x1b[{}m", if background { 40 } else { 30 } + index % 8 + if index >= 8 { 60 } else { 0 }) }
}
fn indexed(index: usize) -> [i32; 3] {
    const BASIC: [[i32; 3]; 16] = [[0,0,0],[128,0,0],[0,128,0],[128,128,0],[0,0,128],[128,0,128],[0,128,128],[192,192,192],
        [128,128,128],[255,0,0],[0,255,0],[255,255,0],[0,0,255],[255,0,255],[0,255,255],[255,255,255]];
    if index < 16 { BASIC[index] }
    else if index < 232 {
        let n = index - 16;
        [n / 36, n / 6 % 6, n % 6].map(|c| if c == 0 { 0 } else { 55 + c as i32 * 40 })
    } else { [8 + (index as i32 - 232) * 10; 3] }
}
#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
