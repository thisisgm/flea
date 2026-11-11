//! A contents result centers once on open and washes only its captured typed run.
use super::{shape::Shaped, CodeView};
use gpui::{point, px, size, Bounds, Context, Hsla, Pixels, Point, Window};
use std::ops::Range;

pub(super) struct Match {
    line: usize,
    text: String,
    run: Option<Range<usize>>,
    wash: Hsla,
    pending: bool,
}

impl CodeView {
    pub(crate) fn reveal_match(
        &mut self,
        number: usize,
        text: &str,
        run: Option<(usize, usize)>,
        accent: Hsla,
        cx: &mut Context<Self>,
    ) {
        let split = run.filter(|&(a, b)| {
            a < b && b <= text.len() && text.is_char_boundary(a) && text.is_char_boundary(b)
        });
        let (text, run) = split.map_or_else(
            || (text.to_owned(), None),
            |(a, b)| {
                let parts = flea::ui_logic_doc_preview::code::pairs(&[
                    (0, &text[..a]),
                    (1, &text[a..b]),
                    (0, &text[b..]),
                ]);
                let start = parts[0].1.len();
                let end = start + parts[1].1.len();
                (
                    parts.into_iter().map(|(_, s)| s).collect(),
                    Some(start..end),
                )
            },
        );
        self.matched = Some(Match {
            line: number.max(1),
            text,
            run,
            wash: accent.opacity(0.14),
            pending: true,
        });
        self.revision += 1;
        cx.notify();
    }

    pub(super) fn center_match(&mut self, cx: &mut Context<Self>) {
        let Some(matched) = self.matched.as_mut().filter(|m| m.pending) else {
            return;
        };
        if self.frame.1 <= 0. || self.lines.total == 0 {
            return;
        }
        let rows = (self.frame.1 / self.metrics.row.max(1.)).floor().max(1.) as usize;
        self.top = matched
            .line
            .saturating_sub(1)
            .saturating_sub(rows / 2)
            .min(self.lines.total.saturating_sub(rows));
        self.offset = 0.;
        matched.pending = false;
        self.flush(cx);
    }

    pub(super) fn paint_match(
        &self,
        shaped: &Shaped,
        origin: Point<Pixels>,
        row: Pixels,
        window: &mut Window,
    ) {
        let Some(matched) = self.matched.as_ref().filter(|m| m.line == shaped.line + 1) else {
            return;
        };
        let Some(run) = &matched.run else { return };
        if self
            .prepared(shaped.line)
            .is_none_or(|p| p.text != matched.text)
        {
            return;
        }
        for piece in &shaped.pieces {
            let (at, line) = (piece.start, piece.run.line());
            let (a, b) = (run.start.max(at), run.end.min(at + piece.len));
            if a < b {
                let left = piece.x + line.x_for_index(a - at);
                let right = piece.x + line.x_for_index(b - at);
                window.paint_quad(gpui::fill(
                    Bounds::new(origin + point(left, px(0.)), size(right - left, row)),
                    matched.wash,
                ));
            }
        }
    }
}
