//! Shared tile geometry and ground ink, including a chooser whose entry holds focus.
use super::{Drawn, GridState, Ink};
use crate::grid::geometry::Rect;
use gpui::{point, px, size, Bounds, GlyphRun, Hsla, Pixels, TextRun, Window};

pub(in crate::grid::element) fn bounds_of(
    origin: gpui::Point<Pixels>,
    rect: Rect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(origin.x + px(rect.x), origin.y + px(rect.y)),
        size(px(rect.width), px(rect.height)),
    )
}

pub(super) fn label_run(text: &str, state: &GridState, size: Pixels, window: &Window) -> GlyphRun {
    let run = TextRun {
        len: text.len(),
        font: state.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(window.text_system().layout_line(text, size, &[run], None))
}

// A run's baseline inside a fixed-height caption line, centred on the line box.
pub(super) fn baseline(run: &GlyphRun, top: Pixels, line: f32, window: &Window) -> Pixels {
    let layout = run.line();
    window.pixel_snap(top + (px(line) - layout.ascent - layout.descent) / 2. + layout.ascent)
}

// Both grid boards place the caption ink two pixels below the natural font top.
pub(super) fn caption_baseline(run: &GlyphRun, top: Pixels, window: &Window) -> Pixels {
    const TEXT_TOP: f32 = 2.;
    window.pixel_snap(top + px(TEXT_TOP) + run.line().ascent)
}

#[cfg(test)]
#[path = "caption_tests.rs"]
mod tests;

// The tile's own ground: the plate it sits on over the pane, which a cut picture fades toward.
pub(super) fn plate(
    drawn: &Drawn,
    ink: &Ink,
    focused: bool,
    selected: Option<Hsla>,
) -> Option<Hsla> {
    if drawn.cursor {
        Some(if focused {
            ink.accent.opacity(super::CURSOR_WASH)
        } else {
            ink.foreground.opacity(super::CURSOR_WASH)
        })
    } else if drawn.selected {
        Some(if focused {
            selected.unwrap_or_else(|| ink.foreground.opacity(super::MARK_PLATE))
        } else {
            ink.foreground.opacity(crate::list::TRACK)
        })
    } else if drawn.hovered && ink.hover_wash {
        Some(ink.foreground.opacity(super::HOVER))
    } else {
        None
    }
}

impl Ink {
    /// The window's roles and Settings > View > Highlight on hover, read once per frame in prepaint.
    pub(super) fn for_window(window: &Window, cx: &gpui::App) -> Self {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        Self {
            hover_wash: crate::list::hover_wash::HoverWash::on(cx),
            ..Self::inherited(&window.text_style(), theme)
        }
    }
}
