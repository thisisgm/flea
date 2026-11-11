//! Page size, pan, zoom and scroll stay with the HTML viewport.
use super::*;
use gpui::ScrollWheelEvent;

impl HtmlView {
    /// The page's drawn size at the current zoom, logical pixels; zero until a page is decoded.
    pub fn page_size(&self) -> (f64, f64) {
        let decoded = self.image.as_ref().map_or((0, 0), |(_, _, size)| *size);
        flea::ui_logic_doc_preview::html::picture_size(
            f64::from(self.frame.0),
            self.doc.zoom,
            decoded,
        )
    }

    /// The caller's read of a remote page for Source, quoting the generation `set_path` returned for that page. A read
    /// from any earlier generation, a returned page's superseded read included, is dropped, so it never draws or fails.
    pub fn set_source_text(
        &mut self,
        generation: u64,
        text: Option<String>,
        read_failed: bool,
        cx: &mut Context<Self>,
    ) {
        if generation == 0 || generation != self.read_gen {
            return;
        }
        self.source_text = text;
        self.source_read_failed = read_failed;
        self.sync_source(true, cx);
        cx.notify();
    }

    /// Pans a page wider than the frame sideways, never past either edge (PreviewHtml.qml contentWidth).
    pub fn pan_by(&mut self, pixels: f32, cx: &mut Context<Self>) {
        self.scroll_x = (self.scroll_x + pixels).clamp(0., self.pan_end());
        cx.notify();
    }

    // How far a zoomed page can pan before its right edge meets the frame's.
    pub(super) fn pan_end(&self) -> f32 {
        (self.page_size().0 as f32 - self.frame.0).max(0.)
    }

    /// j and k scroll the page or Source by rows; the cursor never moves.
    pub fn scroll_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        if let (true, Some(source)) = (self.doc.source_view(), &self.source) {
            source.update(cx, |code, cx| code.scroll_rows(rows, cx));
            return;
        }
        self.scroll_by(rows as f32 * self.metrics.row_height, cx);
    }

    pub fn scroll_by(&mut self, pixels: f32, cx: &mut Context<Self>) {
        let end = (self.page_size().1 as f32 - self.frame.1).max(0.);
        self.scroll_y = (self.scroll_y + pixels).clamp(0., end);
        cx.notify();
    }

    pub fn zoom_by(&mut self, steps: i64, cx: &mut Context<Self>) {
        self.doc.zoom_by(steps);
        self.scroll_x = self.scroll_x.min(self.pan_end());
        cx.notify();
    }

    pub fn zoom_fit(&mut self, cx: &mut Context<Self>) {
        self.doc.zoom_fit();
        self.scroll_y = 0.;
        self.bars = Default::default();
        self.scroll_x = 0.;
        cx.notify();
    }

    /// The gap under the page's end, -1 with no page: a short page covers the viewport's bottom edge (html-preview F9).
    pub fn end_gap(&self) -> f32 {
        if self.image.is_none() {
            return -1.;
        }
        (self.frame.1 - (self.page_size().1 as f32 - self.scroll_y)).max(0.)
    }

    pub(super) fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = crate::input::scroll::wheel_delta(&event.delta);
        self.pan_by(-f32::from(delta.x), cx);
        self.scroll_by(-f32::from(delta.y), cx);
    }
}

impl crate::g2::quick_look_overlay::scroller::overlay::Owner for HtmlView {
    fn overlay(&mut self) -> &mut crate::g2::quick_look_overlay::scroller::overlay::Overlay {
        &mut self.bars
    }
    fn viewport(
        &self,
        bounds: gpui::Bounds<gpui::Pixels>,
    ) -> Option<(crate::g2::quick_look_overlay::scroller::View, (f64, f64))> {
        if self.doc.source_view() || self.image.is_none() {
            return None;
        }
        let content = self.page_size();
        Some((
            crate::g2::quick_look_overlay::scroller::View {
                bounds,
                content_w: content.0,
                content_h: content.1,
                sideways: true,
            },
            (f64::from(self.scroll_x), f64::from(self.scroll_y)),
        ))
    }
    fn position(&mut self, position: (f64, f64), cx: &mut Context<Self>) {
        self.scroll_x = position.0 as f32;
        self.scroll_y = position.1 as f32;
        cx.notify();
    }
}
