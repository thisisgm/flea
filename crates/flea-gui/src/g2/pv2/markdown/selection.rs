//! Selection follows the rendered text's layout order; Source keeps the file's original bytes.
use super::{MarkdownView, View};
use crate::g2::pv2::selection::{Chunk, Lines, Span};
use flea::ui_logic_pv2::markdown::{
    markdown_flow::{Atom, Laid},
    markdown_layout::Item,
};
use gpui::{point, px, size, Bounds, Context, GlyphRun, MouseDownEvent, MouseMoveEvent, Window};
use std::sync::Arc;

pub(super) fn chunk(block: usize, text: usize) -> usize {
    (block << 32) | text
}

fn text(laid: &Laid) -> String {
    let mut out = String::new();
    for (i, line) in laid.lines.iter().enumerate() {
        if i > 0 {
            out.push_str(laid.lines[i - 1].after);
        }
        for atom in &line.atoms {
            if let Atom::Text { text, .. } = atom {
                out.push_str(text);
            }
        }
    }
    out
}

/// A table row's rule, which only a table draws in the faint wash.
fn row_rule(item: &Item) -> bool {
    matches!(item, Item::Rule { wash: true, .. })
}

/// What follows a text item: a tab before the next cell of its table row, else a line break.
fn after(rest: &[Item]) -> char {
    let next = rest
        .iter()
        .find(|item| matches!(item, Item::Text { .. }) || row_rule(item));
    if matches!(next, Some(Item::Text { .. })) && rest.iter().any(row_rule) {
        '\t'
    } else {
        '\n'
    }
}

fn chunks(
    items: &[Item],
    id: usize,
    ordinal: &mut usize,
    out: &mut std::collections::BTreeMap<usize, Chunk>,
) {
    for (i, item) in items.iter().enumerate() {
        match item {
            Item::Text { laid, .. } => {
                out.entry(chunk(id, *ordinal)).or_insert_with(|| {
                    let mut value = text(laid);
                    value.push(after(&items[i + 1..]));
                    Chunk::new(value)
                });
                *ordinal += 1;
            }
            Item::Table { items, .. } => chunks(items, id, ordinal, out),
            _ => {}
        }
    }
}

impl MarkdownView {
    pub(super) fn selection_start_frame(&mut self, cx: &mut Context<Self>) {
        let stamp = (
            self.doc.seq,
            self.doc.generation,
            self.doc.version,
            self.doc.parsed.blocks.as_ptr() as usize,
            self.wrap,
            self.shown_view() == View::Source,
        );
        if self.selection_stamp != Some(stamp)
            || self.status_sentence().is_some()
            || (self.shown_view() == View::Source && self.source_refused().is_some())
        {
            self.selection.clear();
            self.selection_stamp = Some(stamp);
        }
        if self.selection.waiting() && !self.selection.dragging() {
            self.selection_chunks(true);
            self.selection.complete_copy(cx);
        }
        self.selection.spans.clear();
        self.selection.contiguous = self.shown_view() == View::Source;
        self.selection.clip = Bounds::new(
            point(px(0.), px(0.)),
            size(px(self.frame.0), px(self.frame.1)),
        );
    }

    /// Source cuts lines only when a range reaches them; rendered text covers the shown blocks, and `whole` adds the range's.
    pub(super) fn selection_chunks(&mut self, whole: bool) {
        if self.shown_view() == View::Source {
            self.selection.lines = self
                .doc
                .text
                .clone()
                .map(|text| Lines::new(text, Arc::clone(&self.doc.starts)));
            return;
        }
        let range = self.selection.range();
        let mut blocks = self.visible();
        if let Some(range) = range.as_ref().filter(|_| whole) {
            blocks = blocks.start.min(range.start.chunk >> 32)
                ..blocks.end.max((range.end.chunk >> 32) + 1);
        }
        for i in blocks {
            if let Some(Some(laid)) = self.doc.laid.get(i) {
                chunks(&laid.geometry.items, i, &mut 0, &mut self.selection.chunks);
            }
        }
        if !whole {
            return;
        }
        self.selection.gaps.clear();
        for i in range
            .as_ref()
            .map_or(0..0, |r| (r.start.chunk >> 32)..(r.end.chunk >> 32) + 1)
        {
            if let Some(None) = self.doc.laid.get(i) {
                self.selection.gaps.push(chunk(i, 0)..chunk(i + 1, 0));
            }
        }
        let gaps = &self.selection.gaps;
        self.selection.set_pending(range.is_some_and(|range| {
            gaps.iter()
                .any(|gap| range.start.chunk < gap.end && gap.start <= range.end.chunk)
        }));
    }

    pub(super) fn text_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection_chunks(false);
        let local = event.position - point(px(self.bounds.0), px(self.bounds.1));
        self.selection.press(event, local, window, cx);
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
            self.selection_chunks(false);
            let local = event.position - point(px(self.bounds.0), px(self.bounds.1));
            self.selection.motion(local, window, cx);
            cx.notify();
        }
    }

    /// Ends a drag with its text ready to copy; true when no range is left, so the release may open a link.
    pub(super) fn text_release(&mut self, cx: &mut Context<Self>) -> bool {
        if self.selection.dragging() {
            self.selection_chunks(true);
            self.selection.release(cx);
        }
        self.selection.range().is_none()
    }

    pub(super) fn selection_run(
        &self,
        index: usize,
        run: &GlyphRun,
        origin: gpui::Point<gpui::Pixels>,
        accent: gpui::Hsla,
        window: &mut Window,
    ) {
        let Some(span) = self.selection.spans.get(index) else {
            return;
        };
        if let Some(part) = self.selection.part(span.chunk, span.start, span.len) {
            let reach = self
                .selection
                .wash_end(span, part.clone())
                .map(|reach| origin.x + (reach - span.bounds.left()));
            let band = Bounds::new(
                origin - point(px(0.), run.line().ascent),
                size(span.bounds.size.width, span.bounds.size.height),
            );
            crate::g2::pv2::selection::paint(
                run,
                origin,
                band,
                part,
                reach,
                (accent, self.palette.foreground),
                window,
            );
        }
    }
}

pub(super) fn span(
    selection: &mut crate::g2::pv2::selection::Selection,
    id: usize,
    start: usize,
    x: f32,
    baseline: f32,
    run: &GlyphRun,
) -> usize {
    let index = selection.spans.len();
    let line = run.line();
    let bounds = Bounds::new(
        point(px(x), px(baseline) - line.ascent),
        size(line.width.max(px(1.)), line.ascent + line.descent),
    );
    selection.spans.push(Span {
        chunk: id,
        start,
        len: line.len,
        bounds,
        clip: selection.clip.intersect(&bounds),
        line: Arc::clone(line),
        reach: bounds.right(),
    });
    index
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
