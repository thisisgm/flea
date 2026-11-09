use flea::colours;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::io::Read;

// The one background fallback, and ui/Theme.qml carries the same literal for the same reason.
const BACKGROUND_FALLBACK: &str = "#101315";
// The ground is the terminal's own default background, so its padding and the strip past the last whole cell match the cells.
const GROUND: &str = "\x1b[49m";
// A theme file larger than 64 KiB is refused, and the watcher reads the file again every half second.
const THEME_BYTES: usize = 64 * 1024;
const THEME_POLL: std::time::Duration = std::time::Duration::from_millis(500);
// A changed read counts only when a second read this much later matches it, longer than a rewrite in place takes.
const THEME_SETTLE: std::time::Duration = std::time::Duration::from_millis(50);
// ui/Theme.qml's textContrast, frameContrast and hueFloor, and the Qt.darker step its metadata ink takes.
const TEXT_CONTRAST: f64 = 4.5;
const FRAME_CONTRAST: f64 = 3.0;
const HUE_FLOOR: f64 = 0.2;
const DARKER_STEP: f64 = 1.4;
// Foreground washes over the background, first the marked row's alpha search from max to min by step, then the fixed washes.
const MARK_ALPHA_MAX: f64 = 0.35;
const MARK_ALPHA_MIN: f64 = 0.12;
const MARK_ALPHA_STEP: f64 = 0.01;
const SELECTED_ALPHA: f64 = 0.18;
const CURSOR_ALPHA: f64 = 0.18;
const TAB_ALPHA: f64 = 0.08;
const MATCH_ALPHA: f64 = 0.14;
const BORDER_ALPHA: f64 = 0.12;
// Overlay inks: the rule is the muted frame at 40%, a default button's wash the raw accent at 14%, an unavailable row the window's 55%, and the card scrim mixes everything halfway to the ground.
const RULE_ALPHA: f64 = 0.40;
const WASH_ALPHA: f64 = 0.14;
const UNAVAILABLE_ALPHA: f64 = 0.55;
const SCRIM_ALPHA: f64 = 0.5;
// The prompt field's 7% fill, and its selection's accent wash eased down from 24% only as far as foreground text needs.
const FIELD_ALPHA: f64 = 0.07;
const STEM_ALPHA_MAX: f64 = 0.24;
const STEM_ALPHA_MIN: f64 = 0.06;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Depth { Mono, Sixteen, Indexed, #[default] Truecolor }
impl Depth {
    // NO_COLOR, set and not empty, asks for no colour at all, so it is read before any probe and before the first frame.
    pub fn initial() -> Self {
        if no_color(std::env::var_os("NO_COLOR")) { Self::Mono } else { Self::Sixteen }
    }
    // Without a fill to tell it apart, the cursor row reads as reverse video.
    pub fn reverses(self) -> bool { matches!(self, Self::Mono | Self::Sixteen) }
    // The row attribute that stands in for a missing fill: reverse video on the cursor, underline on a NO_COLOR mark.
    pub fn row_attribute(self, cursor: bool, marked: bool) -> &'static str {
        if cursor && self.reverses() { "\x1b[7m" } else if marked && self == Self::Mono { "\x1b[4m" } else { "" }
    }
    pub fn detect(cancel: &AtomicBool) -> Self {
        if Self::initial() == Self::Mono { return Self::Mono; }
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
    // The window's control frame (a media track) and the card frame eased to 3:1 under the menu wash.
    pub track: String,
    pub frame: String,
    pub rule: String,
    pub wash: String,
    pub unavailable: String,
    pub unavailable_word: String,
    pub field: String,
    pub stem: String,
    pub ext: String,
    // Syntax inks in bat's ANSI order: red, green, yellow, blue, magenta, cyan.
    pub syntax: [String; super::syntax::SYNTAX_INKS],
    // The ground as #rrggbb, for a decoder that veils a page with it.
    pub ground: String,
    pub depth: Depth,
    // The same palette behind a card's scrim; None on the scrim itself.
    pub dim: Option<Box<Theme>>,
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
        Self { dim: Some(Box::new(Self::build(text, depth, true))), ..Self::build(text, depth, false) }
    }
    fn build(text: &str, depth: Depth, scrim: bool) -> Self {
        let values = parse(text);
        let pick = |keys: &[&str], fallback: &str| {
            keys.iter().find_map(|key| values.get(*key)).map(String::as_str).unwrap_or(fallback).to_owned()
        };
        let foreground = pick(&["foreground", "color7"], "#cacccc");
        let background = pick(&["background", "color0"], BACKGROUND_FALLBACK);
        let accent = pick(&["accent", "color4", "blue"], "#cacccc");
        let surface = pick(&["dark_background", "background", "selection", "color0"], BACKGROUND_FALLBACK);
        let darker = rgb(&foreground).map(|v| (v * 255.0 / DARKER_STEP).round() as u8);
        let darker = format!("#{:02x}{:02x}{:02x}", darker[0], darker[1], darker[2]);
        let muted = contrast(&contrast(&pick(&["muted"], &darker), &background), &surface);
        let muted_frame = ensure(&pick(&["muted"], &darker), &background, FRAME_CONTRAST);
        let meta = contrast(&contrast(&darker, &background), &surface);
        let mut mark_alpha = MARK_ALPHA_MAX;
        while mark_alpha > MARK_ALPHA_MIN && ratio(rgb(&foreground), rgb(&blend(&foreground, &background, mark_alpha))) < TEXT_CONTRAST { mark_alpha -= MARK_ALPHA_STEP; }
        let field = blend(&foreground, &background, FIELD_ALPHA);
        let mut stem_alpha = STEM_ALPHA_MAX;
        while stem_alpha > STEM_ALPHA_MIN && ratio(rgb(&foreground), rgb(&blend(&accent, &field, stem_alpha))) < TEXT_CONTRAST { stem_alpha = ((stem_alpha - MARK_ALPHA_STEP) * 100.0).round() / 100.0; }
        let error = pick(&["urgent"], "#a55555");
        let channels = rgb(&error);
        let high = channels.into_iter().fold(0.0_f64, f64::max);
        let low = channels.into_iter().fold(1.0_f64, f64::min);
        let error = if high == 0.0 || (high - low) / high <= HUE_FLOOR { foreground.clone() } else { contrast(&error, &background) };
        let encode = |hex: &str, bg| color(&if scrim { blend(hex, &background, SCRIM_ALPHA) } else { hex.to_owned() }, bg, depth);
        let syntax = [["red", "color1"], ["green", "color2"], ["yellow", "color3"], ["blue", "color4"], ["magenta", "color5"], ["cyan", "color6"]]
            .map(|keys| encode(&contrast(&pick(&keys, &foreground), &background), false));
        Self {
            foreground: encode(&foreground, false), muted: encode(&muted, false), meta: encode(&meta, false),
            accent: encode(&ensure(&accent, &surface, FRAME_CONTRAST), false), error: encode(&error, false),
            background: GROUND.into(),
            symlink: encode(&contrast(&pick(&["cyan", "color6"], "#94e2d5"), &background), false),
            executable: encode(&contrast(&pick(&["green", "color2"], "#a6e3a1"), &background), false),
            selected: encode(&blend(&foreground, &background, SELECTED_ALPHA), true),
            cursor: encode(&blend(&accent, &background, CURSOR_ALPHA), true),
            marked: encode(&blend(&foreground, &background, mark_alpha), true),
            tab: encode(&blend(&foreground, &background, TAB_ALPHA), true),
            match_fill: encode(&blend(&foreground, &background, MATCH_ALPHA), true),
            border: encode(&blend(&foreground, &background, BORDER_ALPHA), false),
            track: encode(&muted_frame, false),
            frame: encode(&ensure(&ensure(&muted_frame, &background, FRAME_CONTRAST), &blend(&foreground, &background, TAB_ALPHA), FRAME_CONTRAST), false),
            rule: encode(&blend(&muted_frame, &background, RULE_ALPHA), false),
            wash: encode(&blend(&accent, &background, WASH_ALPHA), true),
            unavailable: encode(&blend(&error, &background, UNAVAILABLE_ALPHA), false),
            unavailable_word: encode(&blend(&muted, &background, UNAVAILABLE_ALPHA), false),
            stem: encode(&blend(&accent, &field, stem_alpha), true), ext: encode(&ensure(&muted, &field, TEXT_CONTRAST), false),
            field: encode(&field, true), syntax, ground: background.clone(), depth, dim: None,
        }
    }
}
fn read(path: &std::path::Path) -> Option<String> {
    let file = crate::backend::regfile::open_if_regular(path, 0).ok()?;
    let mut bytes = Vec::new();
    file.take((THEME_BYTES + 1) as u64).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= THEME_BYTES).then(|| String::from_utf8_lossy(&bytes).into_owned())
}
// A changed read is published only if it yields a colour, ui/js/Palette.js's isPalette test, and a second read after the pause matches it, so a rewrite caught empty or part way waits for the next poll.
fn settled(path: &std::path::Path, previous: Option<&str>, pause: impl FnOnce()) -> Option<String> {
    let first = read(path).filter(|text| previous != Some(text.as_str()) && !parse(text).is_empty())?;
    pause();
    (read(path)? == first).then_some(first)
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
            let mut previous: Option<String> = None;
            loop {
                let mut stopped = false;
                if let Some(text) = settled(&path, previous.as_deref(), || stopped = wait.recv_timeout(THEME_SETTLE).is_ok()) {
                    if tx.try_send(text.clone()).is_ok() { previous = Some(text); }
                }
                if stopped || wait.recv_timeout(THEME_POLL).is_ok() { break; }
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
    for (key, value) in colours::entries(text) {
        out.insert(key.into(), value.into());
        if matches!(key, "red" | "color1") { out.insert("urgent".into(), value.into()); }
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
fn contrast(foreground: &str, background: &str) -> String { ensure(foreground, background, TEXT_CONTRAST) }
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

pub(super) fn no_color(value: Option<std::ffi::OsString>) -> bool { value.is_some_and(|value| !value.is_empty()) }
fn color(hex: &str, background: bool, depth: Depth) -> String {
    if depth == Depth::Mono { return String::new(); }
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
#[cfg(test)]
#[path = "ground_tests.rs"]
mod ground_tests;
