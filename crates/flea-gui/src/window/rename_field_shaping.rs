//! Rename caption metrics and glyph shaping, shared by the editor frame and refusal band.
use gpui::{GlyphRun, Pixels, TextRun, Window};
use std::rc::Rc;

/// The height of a refusal's caption line, as its shaped ascent and descent.
pub(crate) fn caption_line(
    window: &Window,
    text: &str,
    font: gpui::Font,
    pixels: Pixels,
) -> Pixels {
    let run = shape(window, text, font, pixels);
    let line = run.line();
    line.ascent + line.descent
}

pub(super) fn shape(window: &Window, text: &str, font: gpui::Font, pixels: Pixels) -> Rc<GlyphRun> {
    let run = TextRun {
        len: text.len(),
        font,
        color: Default::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    Rc::new(GlyphRun::new(window.text_system().layout_line(
        text,
        pixels,
        &[run],
        None,
    )))
}
