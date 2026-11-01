//! The code viewport's existing glyph runs also provide selection hit testing.
use super::{paint::Frame, CodeView};
use crate::g2::pv2::selection::{Chunk, Lines, Span};
use gpui::{point, px, size, Bounds, Context, MouseDownEvent, MouseMoveEvent, Pixels, Window};
use std::sync::Arc;

impl CodeView {
    pub(super) fn selection_frame(&mut self, bounds: Bounds<Pixels>, frame: &Frame) {
        self.selection.spans.clear();
        self.selection.contiguous = true;
        self.selection.clip = Bounds::new(
            bounds.origin + point(px(0.), px(self.metrics.top)),
            size(
                bounds.size.width,
                (bounds.size.height - px(self.metrics.top)).max(px(0.)),
            ),
        );
        let Some(advance) = self.advance else { return };
        for (top, shaped) in &frame.rows {
            let y = bounds.top() + px(self.metrics.top + top);
            for piece in &shaped.pieces {
                if piece.len == 0 && piece.run.line().len > 0 {
                    continue;
                }
                let rect = Bounds::new(
                    point(bounds.left() + px(self.text_left(advance)) + piece.x, y),
                    size(piece.run.line().width.max(advance), px(self.metrics.row)),
                );
                self.selection.spans.push(Span {
                    chunk: shaped.line,
                    start: piece.start,
                    len: piece.len,
                    bounds: rect,
                    clip: rect.intersect(&self.selection.clip),
                    line: Arc::clone(piece.run.line()),
                    reach: rect.right(),
                });
            }
        }
    }

    /// The caller's bytes or the original read give whole lines; without them only the held rows can be copied.
    pub(super) fn selection_chunks(&mut self) {
        if self.selection_original.is_none() {
            if let Some(text) = &self.lines.text {
                self.selection_original = Some(Some(document_lines(text)));
            }
        }
        if let Some(Some(original)) = &self.selection_original {
            self.selection.lines = Some(original.clone());
            return;
        }
        self.selection.lines = None;
        let (from, to) = self.lines.needed(self.view());
        for n in from..=to {
            if let Some(runs) = self.lines.row(n) {
                let mut raw: String = runs.iter().map(|(_, text)| text.as_str()).collect();
                if n < self.lines.shown() {
                    raw.push('\n');
                }
                self.selection.chunks.insert(n - 1, Chunk::new(raw));
            }
        }
    }

    pub(super) fn text_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.scroll_press(event.position, cx) {
            self.selection.release(cx);
            return;
        }
        if self.refusal().is_some() {
            return;
        }
        self.start_selection_read(cx);
        self.selection_chunks();
        self.selection.press(event, event.position, window, cx);
        self.selection.set_pending(self.selection_read.is_some());
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn text_motion(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.dragging() && event.pressed_button == Some(gpui::MouseButton::Left) {
            self.selection_chunks();
            self.selection.motion(event.position, window, cx);
            cx.notify();
        } else {
            self.scroll_pointer(
                event.position,
                event.pressed_button == Some(gpui::MouseButton::Left),
                cx,
            );
        }
    }
}

/// One document's lines, cut on demand from a single shared copy.
pub(super) fn document_lines(text: &str) -> Lines {
    let text: Arc<str> = text.into();
    let starts = Arc::new(crate::g2::pv2::markdown::line_starts(&text));
    Lines::new(text, starts)
}
