//! A figure's theme, its cache key and the geometry it draws at: fitted to the pane, or its fenced source when it fails.

use super::markdown_model::FigureKind;
use crate::json::escape;

/// The first and last printable ASCII characters whose advances a diagram sizes its labels from.
pub const ADVANCE_FIRST: u8 = 32;
pub const ADVANCE_LAST: u8 = 126;
/// Advances go in thousandths of an em, and the x-height in hundredths of a pixel, so a font that settles late changes the request.
pub const ADVANCE_ROUNDING: f64 = 1000.0;
pub const X_HEIGHT_ROUNDING: f64 = 100.0;
/// A failed figure shows at most this many characters of its source.
pub const FALLBACK_CHARS: usize = 2000;
/// One canvas margin per diagram side, which two diagrams in a row lend back once each.
pub const CANVAS_MARGIN: f64 = 1.0;

/// The theme a figure is asked under: the hex inks, the body font and, for a diagram, the label advances.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Theme {
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub font: String,
    pub body_px: u32,
    pub ex_px: f64,
    pub advances: Vec<i64>,
    pub bold_advances: Vec<i64>,
    pub muted: String,
    pub surface: String,
}

/// One advance table entry per printable ASCII character, each the advance over the font size in thousandths.
pub fn advance_table(px: f64, advance: &dyn Fn(char) -> f64) -> Vec<i64> {
    (ADVANCE_FIRST..=ADVANCE_LAST)
        .map(|code| (advance(code as char) / px * ADVANCE_ROUNDING).round() as i64)
        .collect()
}

/// The x-height as the helper receives it, in hundredths of a pixel.
pub fn ex_px(x_height: f64) -> f64 {
    (x_height * X_HEIGHT_ROUNDING).round() / X_HEIGHT_ROUNDING
}

// A number as JavaScript writes it: no fraction for a whole value, and no minus for zero.
fn js_number(n: f64) -> String {
    if n == 0.0 {
        "0".to_string()
    } else {
        format!("{n}")
    }
}

/// FNV-1a over the table's comma-joined text, "0" for none.
pub fn table_digest(table: &[i64]) -> String {
    if table.is_empty() {
        return "0".to_string();
    }
    let text = table
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let hash = text.bytes().fold(0x811c_9dc5_u32, |hash, b| {
        (hash ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    format!("{hash:x}")
}

/// The cache key of a figure: its kind, every theme field it is drawn under, its display mode and its source.
pub fn cache_key(kind: FigureKind, source: &str, theme: &Theme, display: bool) -> String {
    let fields = [
        theme.bg.clone(),
        theme.fg.clone(),
        theme.accent.clone(),
        theme.muted.clone(),
        String::new(),
        theme.surface.clone(),
        String::new(),
        theme.font.clone(),
        theme.body_px.to_string(),
        js_number(theme.ex_px),
        table_digest(&theme.advances),
        table_digest(&theme.bold_advances),
    ];
    format!("{}\n{}\n{display}\n{source}", kind.name(), fields.join("|"))
}

fn table_json(table: &[i64]) -> String {
    format!(
        "[{}]",
        table
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// The theme as the helper's request carries it, in the order the request has always written it.
pub fn theme_json(theme: &Theme) -> String {
    format!(
        "{{\"bg\":\"{}\",\"fg\":\"{}\",\"accent\":\"{}\",\"font\":\"{}\",\"bodyPx\":{},\"exPx\":{},\"advances\":{},\"boldAdvances\":{},\"muted\":\"{}\",\"surface\":\"{}\"}}",
        escape(&theme.bg),
        escape(&theme.fg),
        escape(&theme.accent),
        escape(&theme.font),
        theme.body_px,
        js_number(theme.ex_px),
        table_json(&theme.advances),
        table_json(&theme.bold_advances),
        escape(&theme.muted),
        escape(&theme.surface)
    )
}

/// One request line: {"id":3,"kind":"math","source":"x^2","display":true,"theme":{...}} and its newline.
pub fn request_line(
    id: u64,
    kind: FigureKind,
    source: &str,
    display: bool,
    theme: &Theme,
) -> String {
    format!(
        "{{\"id\":{id},\"kind\":\"{}\",\"source\":\"{}\",\"display\":{display},\"theme\":{}}}\n",
        kind.name(),
        escape(source),
        theme_json(theme)
    )
}

/// The source a failed figure shows: a long one cut at FALLBACK_CHARS with the count of what was left out.
pub fn fallback_body(source: &str) -> String {
    let count = source.encode_utf16().count();
    if count <= FALLBACK_CHARS {
        return source.to_string();
    }
    let units: Vec<u16> = source.encode_utf16().take(FALLBACK_CHARS).collect();
    format!(
        "{}\u{2026} ({} more)",
        String::from_utf16_lossy(&units),
        count - FALLBACK_CHARS
    )
}

/// A drawn figure's size in a pane: never wider than the pane, its ratio kept, whole pixels so the raster stays 1:1.
pub fn fit(natural_w: f64, natural_h: f64, width: f64) -> (f64, f64) {
    if natural_w <= 0.0 {
        return (0.0, 0.0);
    }
    let w = natural_w.min(width).round();
    (w, (natural_h * (w / natural_w)).round())
}

/// A display formula centres in the pane; a diagram and an inline formula start at its left edge.
pub fn centred(kind: FigureKind, display: bool, inline: bool) -> bool {
    kind == FigureKind::Math && display && !inline
}

/// What two figures in a row lend back: one canvas margin for each diagram of the pair.
pub fn pair_trim(kind: FigureKind, next: Option<FigureKind>) -> f64 {
    let margin = |kind: FigureKind| {
        if kind == FigureKind::Mermaid {
            CANVAS_MARGIN
        } else {
            0.0
        }
    };
    next.map_or(0.0, |next| margin(kind) + margin(next))
}

/// The theme a figure of a kind is asked under: a diagram carries the label advances, a formula none.
pub fn theme_of_kind(
    kind: FigureKind,
    base: &Theme,
    advances: &[i64],
    bold_advances: &[i64],
) -> Theme {
    let mermaid = kind == FigureKind::Mermaid;
    Theme {
        advances: if mermaid {
            advances.to_vec()
        } else {
            Vec::new()
        },
        bold_advances: if mermaid {
            bold_advances.to_vec()
        } else {
            Vec::new()
        },
        ..base.clone()
    }
}
