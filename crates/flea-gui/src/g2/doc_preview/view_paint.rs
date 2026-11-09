//! The PDF view's layout and paint: each page one image, the stack scrolled under the frame, the single page fitted and zoomed.
use super::PdfView;
use flea::ui_logic_doc_preview::pdf;
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, Bounds, ContentMask, Context, Corners, Hsla,
    Pixels, Render, RenderImage, Window,
};
use std::sync::Arc;

impl PdfView {
    /// Quick Look's zoom and the zoomed content's size; the column is never zoomed.
    pub(super) fn zoomed(&self) -> (f64, (f64, f64)) {
        let zoom = self.viewer.as_ref().map_or(1., |viewer| viewer.zoom);
        (zoom, (self.frame.0 * zoom, self.frame.1 * zoom))
    }

    /// Lays the surface out in its bounds before paint, and hands back the images paint must release from the atlas.
    pub(super) fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Vec<Arc<RenderImage>> {
        self.frame = (f64::from(bounds.size.width), f64::from(bounds.size.height));
        let gap = f64::from(self.metrics.gap);
        if self.surface.continuous {
            self.surface
                .set_frame(self.frame.0, self.frame.1, f64::from(scale), gap);
        } else {
            let margin = f64::from(self.metrics.margin);
            let (_, content) = self.zoomed();
            let inner = (
                (content.0 - 2. * margin).max(0.),
                (content.1 - 2. * margin).max(0.),
            );
            self.surface.set_fitted(
                (self.frame.0 - 2. * margin).max(0.),
                (self.frame.1 - 2. * margin).max(0.),
            );
            self.surface
                .set_frame(inner.0, inner.1, f64::from(scale), gap);
            self.pan = (
                self.pan.0.min(content.0 - self.frame.0).max(0.),
                self.pan.1.min(content.1 - self.frame.1).max(0.),
            );
        }
        self.flush(cx);
        self.surface.cache.take_released()
    }

    /// Where the single page draws inside the frame, logical pixels from the frame's top left, None until a page is shown.
    pub fn single_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let s = &self.surface;
        s.single_image()?;
        let margin = f64::from(self.metrics.margin);
        let (w, h) = pdf::fit(s.width, s.view_height, s.page_aspect());
        let x = margin + (s.width - w) / 2. - self.pan.0;
        let y = margin + (s.view_height - h) / 2. - self.pan.1;
        Some((x, y, w, h))
    }

    /// The stack's pages any part of the frame shows that hold an image, with each page's top inside the frame.
    pub fn drawn_pages(&self) -> impl Iterator<Item = (usize, f64, f64)> + '_ {
        let s = &self.surface;
        s.slots.iter().filter_map(move |slot| {
            s.image(slot.page)?;
            let top = s.layout.tops.get(slot.page)? - s.view_top;
            let height = *s.layout.heights.get(slot.page)?;
            (top + height > 0. && top < s.view_height).then_some((slot.page, top, height))
        })
    }

    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        self.painted.set(self.painted.get() + 1);
        let veil = self.palette.veil();
        let s = &self.surface;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if s.continuous {
                for (page_no, top, height) in self.drawn_pages() {
                    let Some(image) = s.image(page_no) else {
                        continue;
                    };
                    let rect = Bounds::new(
                        bounds.origin + point(px(0.), px(top as f32)),
                        size(px(s.width as f32), px(height as f32)),
                    );
                    page(window, rect, image, veil);
                }
            } else if let (Some((x, y, w, h)), Some(image)) = (self.single_rect(), s.single_image())
            {
                let rect = Bounds::new(
                    bounds.origin + point(px(x as f32), px(y as f32)),
                    size(px(w as f32), px(h as f32)),
                );
                page(window, rect, image, veil);
            }
        });
    }
}

// The page's own paper is inside its raster, so a page that has not landed draws nothing and never flashes white.
fn page(window: &mut Window, rect: Bounds<Pixels>, image: &Arc<RenderImage>, veil: Option<Hsla>) {
    let _ = window.paint_image(rect, rect, Corners::default(), Arc::clone(image), 0, false);
    if let Some(veil) = veil {
        window.paint_quad(fill(rect, veil));
    }
}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A live theme switch repaints the window; the veil follows the new ground in that frame.
        if let Some(theme) = cx.try_global::<crate::palette::Theme>() {
            self.palette.background = theme.background;
        }
        let (laid, painted) = (cx.entity(), cx.entity());
        let ink = crate::list::Palette::inherited(
            &window.text_style(),
            cx.try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default(),
        )
        .foreground;
        let bars = crate::g2::quick_look_overlay::scroller::overlay::surface(
            cx.entity(),
            ink,
            crate::compositor::corner(cx),
        );
        div()
            .relative()
            .size_full()
            .on_scroll_wheel(cx.listener(|view, event, _, cx| view.wheel(event, cx)))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let scale = window.scale_factor();
                        laid.update(cx, |view, cx| view.layout(bounds, scale, cx))
                    },
                    move |bounds, released, window, cx| {
                        for image in released {
                            let _ = window.drop_image(image);
                        }
                        painted.read(cx).paint(bounds, window);
                    },
                )
                .size_full(),
            )
            .child(bars)
    }
}
