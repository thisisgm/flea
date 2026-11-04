//! The HTML view's layout and paint: the page image under the frame, scrolled and zoomed, clipped to the frame's inner arc.
use super::HtmlView;
use crate::g2::pv2::code::{paint::refusal_frame, shape};
use flea::ui_logic_pv2::refusal::Refusal;
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, Bounds, Context, Corners, GlyphRun, Pixels,
    Render, RenderImage, Window,
};
use std::{rc::Rc, sync::Arc};

impl HtmlView {
    /// The frame's inner arc: Quick Look's page sits under its bar, so only the column rounds its top corners.
    pub fn corners(&self) -> Corners<Pixels> {
        let r = px(self.metrics.radius);
        let top = if self.metrics.round_top { r } else { px(0.) };
        Corners {
            top_left: top,
            top_right: top,
            bottom_left: r,
            bottom_right: r,
        }
    }

    /// Lays the view out before paint; a new device size restarts the settle, and an unchanged frame does no work.
    /// Hands back the pages to release and the too-large frame's caption, shaped once per catalog and scale.
    fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Vec<Arc<RenderImage>>, Option<Rc<GlyphRun>>) {
        self.bounds = Some(bounds);
        self.frame = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let laid = (self.frame.0, self.frame.1, window.scale_factor());
        if self.laid != Some(laid) {
            self.laid = Some(laid);
            let now = self.now(cx);
            self.doc
                .set_frame(f64::from(laid.0), f64::from(laid.1), f64::from(laid.2), now);
            self.flush(cx);
        }
        let end = (self.page_size().1 as f32 - self.frame.1).max(0.);
        self.scroll_y = self.scroll_y.min(end);
        self.scroll_x = self.scroll_x.min(self.pan_end());
        let caption = match (
            self.compact,
            self.doc.source_view(),
            self.refusal(cx),
            &self.catalog,
        ) {
            (false, false, Some(Refusal::TooLarge), Some(catalog)) => {
                let scale = laid.2.to_bits();
                match &self.caption {
                    Some((at, at_scale, run)) if Arc::ptr_eq(at, catalog) && *at_scale == scale => {
                        Some(Rc::clone(run))
                    }
                    _ => {
                        let typeface = &self.parts.typeface;
                        let run = Rc::new(shape::glyphs(
                            window,
                            &catalog.tr("document.too_large_frame"),
                            &typeface.font,
                            typeface.caption,
                        ));
                        self.caption = Some((Arc::clone(catalog), scale, Rc::clone(&run)));
                        Some(run)
                    }
                }
            }
            _ => None,
        };
        (std::mem::take(&mut self.released), caption)
    }

    /// Where the page draws, logical pixels from the frame's top left; None until a page is decoded.
    pub fn page_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.image.as_ref()?;
        let (w, h) = self.page_size();
        Some((-self.scroll_x, -self.scroll_y, w as f32, h as f32))
    }

    fn paint(
        &self,
        bounds: Bounds<Pixels>,
        caption: Option<&GlyphRun>,
        window: &mut Window,
        cx: &gpui::App,
    ) {
        self.painted.set(self.painted.get() + 1);
        if let Some(caption) = caption {
            let m = &self.parts.metrics;
            refusal_frame(
                window,
                cx,
                bounds,
                (0., m.state_mark, m.gap),
                &self.mark,
                caption,
                self.parts.palette.muted,
            );
            return;
        }
        if self.doc.source_view() || self.refusal(cx).is_some() {
            return;
        }
        let (Some((x, y, w, h)), Some((_, image, _))) = (self.page_rect(), &self.image) else {
            return;
        };
        let page = Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h)));
        let corners = self.corners();
        let _ = window.paint_image(bounds, page, corners, Arc::clone(image), 0, false);
        if let Some(veil) = self.palette.veil() {
            window.paint_quad(fill(bounds.intersect(&page), veil).corner_radii(corners));
        }
    }
}

impl Render for HtmlView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (laid, painted) = (cx.entity(), cx.entity());
        let ink = crate::list::Palette::inherited(
            &window.text_style(),
            cx.try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default(),
        )
        .foreground;
        let bars = crate::g2::pv1::scroller::overlay::surface(
            cx.entity(),
            ink,
            crate::compositor::corner(cx),
        );
        let page = canvas(
            move |bounds, window, cx| laid.update(cx, |view, cx| view.layout(bounds, window, cx)),
            move |bounds, (released, caption), window, cx| {
                for image in released {
                    let _ = window.drop_image(image);
                }
                painted
                    .read(cx)
                    .paint(bounds, caption.as_deref(), window, cx);
            },
        )
        .size_full();
        let source = self
            .doc
            .source_view()
            .then(|| self.source.clone())
            .flatten();
        div()
            .size_full()
            .relative()
            .on_scroll_wheel(cx.listener(|view, event, _, cx| {
                if !view.doc.source_view() {
                    view.wheel(event, cx);
                }
            }))
            .child(page)
            .child(bars)
            .children(source.map(|code| div().absolute().inset_0().child(code)))
    }
}
