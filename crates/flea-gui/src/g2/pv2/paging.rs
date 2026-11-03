//! Document paging uses each reader's viewport, on either host surface.
use super::{csv::CsvView, html::HtmlView, markdown::MarkdownView, office::OfficeView};
use flea::ui_logic_pv2::{code_view::page_step, office::State};
use gpui::Context;

// `host` is the column frame's height, used while a reader has not laid out its own viewport.
fn viewport(own: f32, host: f32) -> f32 {
    if own > 0. {
        own
    } else {
        host
    }
}

impl MarkdownView {
    pub fn page(&mut self, direction: i64, host: f32, cx: &mut Context<Self>) {
        if self.shown_view() == super::markdown::View::Source {
            let row = self.source.frame.line.max(1.);
            let height = viewport(self.source.frame.height, viewport(self.frame.1, host));
            let step = page_step(height, row);
            let partial = self.source.offset % row;
            self.scroll_by(direction as f32 * step - partial, cx);
        } else {
            self.scroll_by(direction as f32 * viewport(self.frame.1, host), cx);
        }
    }
}

impl HtmlView {
    pub fn page(&mut self, direction: i64, host: f32, cx: &mut Context<Self>) {
        if let (true, Some(source)) = (self.doc.source_view(), self.source.clone()) {
            let host = viewport(self.frame.1, host);
            source.update(cx, |source, cx| source.page(direction, host, cx));
        } else {
            self.scroll_by(direction as f32 * viewport(self.frame.1, host), cx);
        }
    }
}

impl CsvView {
    /// PreviewScroll.qml pageBy: one body viewport under the header, never past either end.
    pub fn page(&mut self, direction: i64, cx: &mut Context<Self>) {
        let geometry = self.feed.geometry;
        let height = (self.frame.1 - geometry.head_height).max(0.);
        let end = (self.feed.body_rows() as f64 * geometry.row_height - height).max(0.);
        self.scroll.1 = (self.scroll.1 + direction as f64 * height).clamp(0., end);
        cx.notify();
    }
}

impl OfficeView {
    pub fn page(&mut self, direction: i64, cx: &mut Context<Self>) -> bool {
        if self.office.state != State::Ready {
            return false;
        }
        if self.office.wants_sheets() {
            self.table.update(cx, |table, cx| table.page(direction, cx));
        } else {
            self.pdf.update(cx, |pdf, cx| pdf.turn(direction, cx));
        }
        true
    }
}
