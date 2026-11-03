//! GridTile.qml nameTip: the whole name under a hovered tile whose caption Flea elided, on the surface plate with a muted hairline frame.
use super::Drawn;
use crate::grid::geometry::{Metrics, Rect};
use gpui::{px, Font, GlyphRun, Hsla, Pixels, TextRun, Window};

/// The tip as the last frame laid it out, in grid pixels; its lines are shaped once per name and size.
pub struct Tip {
    pub listing: usize,
    pub name: String,
    pub rect: Rect,
    pub line: f32,
    size: Pixels,
    natural: f32,
    pub lines: Vec<GlyphRun>,
}

fn run(text: &str, font: &Font, size: Pixels, window: &Window) -> GlyphRun {
    let style = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, size, &[style], None))
}

// Text.Wrap: break at the last blank that fits, else at the last char that fits, never an empty line.
fn wrap(name: &str, full: &GlyphRun, width: f32) -> Vec<std::ops::Range<usize>> {
    let layout = full.line();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < name.len() {
        let left = f32::from(layout.x_for_index(start));
        let mut end = layout
            .index_for_x(px(left + width))
            .unwrap_or(name.len())
            .min(name.len());
        if end < name.len() {
            if let Some(blank) = name[start..end]
                .rfind(char::is_whitespace)
                .filter(|&at| at > 0)
            {
                end = start + blank + 1;
            }
        }
        if end <= start {
            end = start + name[start..].chars().next().map_or(1, char::len_utf8);
        }
        lines.push(start..end);
        start = end;
    }
    lines
}

/// Lays the tip out under the tile's painted label, centred on its cell: the name's own width plus a gap each side, capped at two minimum cells.
pub fn prepare(
    kept: Option<Tip>,
    drawn: &Drawn,
    name: &str,
    metrics: &Metrics,
    font: &Font,
    window: &Window,
) -> Tip {
    let (listing, cell, label_bottom) = (
        drawn.listing,
        drawn.cell,
        drawn.caption.y + drawn.painted.height,
    );
    let size = px(metrics.caption);
    let gap = metrics.gap;
    let shaped = kept.filter(|tip| tip.name == name && tip.size == size);
    let (lines, line, natural) = match shaped {
        Some(tip) => (tip.lines, tip.line, tip.natural),
        None => {
            let full = run(name, font, size, window);
            let natural = f32::from(full.line().width);
            let width = (natural + 2. * gap).min(metrics.min_cell * 2.) - 2. * gap;
            let line = f32::from(full.line().ascent + full.line().descent).ceil();
            let lines = wrap(name, &full, width.max(1.))
                .into_iter()
                .map(|range| run(name[range].trim_end(), font, size, window))
                .collect();
            (lines, line, natural)
        }
    };
    let width = (natural + 2. * gap).min(metrics.min_cell * 2.);
    let height = lines.len() as f32 * line + gap;
    let rect = Rect {
        x: cell.x + ((cell.width - width) / 2.).round(),
        y: label_bottom + metrics.hairline,
        width,
        height,
    };
    Tip {
        listing,
        name: name.to_owned(),
        rect,
        line,
        size,
        natural,
        lines,
    }
}
